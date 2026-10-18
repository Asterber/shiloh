// Web Worker: runs the engine (pkg/xf_web.wasm, see port/web/src/lib.rs). Game files are read with synchronous
// XMLHttpRequests (allowed in workers) from xf-serve: whole files, or byte ranges with a Range header.
// Messages in:  {t:'init', canvas?, audioPort?, rate, volume, hyp, start?: 'menu' | 'new', saves?: 'server' | 'idb' |
//               'memory' (the page's XfSaves.detect), pauseHidden}, {t:'move', x, y, inside},
//               {t:'down', b, x, y, inside}, {t:'wheel', lines, x, y, inside}, {t:'key', key}, {t:'volume', v}, {t:'subs', lang} (also in init: m.subs = 'auto' | 'en' | 'ru' | 'off'), {t:'long-timers', nt} (also in init:
//               m.longTimers = 'wine' | 'nt'), {t:'visible', v} (the page's visibility), {t:'pause-hidden', v}
//               (H1730: hidden + on = the loop stops, no CPU; hidden + off = the game runs on, nothing drawn),
//               {t:'saves-reload'} (the page imported slot files, H1734), {t:'set', key, value} (a setting, H1750: xf_set;
//               init's m.settings = `key = value` lines), {t:'pause', v} (the settings dialog shows: the loop stops),
//               {t:'get-settings'}, {t:'alt', key: 'A'..'Z' | '0'..'9', shift} (H2060: the original's Alt hotkeys)
// Messages out: {t:'ready'}, {t:'status', ...}, {t:'pointer', p}, {t:'error', msg}, {t:'frame', buf} (no OffscreenCanvas),
//               {t:'quit'} (the game menu's Quit), {t:'saves', where}, {t:'progress', stage, got, total} (loading),
//               {t:'saves-reloaded'}, {t:'open-settings'} (the menu screen's "Port settings" button),
//               {t:'settings', text, crop: {buf, w, h}} (the engine's settings and a crop of the frame, H1752),
//               {t:'alt', r} (xf_alt's result: the page's part of an Alt hotkey)
// Save slots (env.xf_js_save): xf-serve's /saves/ when the server keeps the saves (`--saves-in server`, H1733 — the
// window's file format, they survive a refresh and are shared by every browser on the LAN), else this browser's
// IndexedDB (kept in `local`, a write also goes to IndexedDB), else memory only. The page decides (www/saves.js).
'use strict';
const W = 640, H = 480;
let x = null;            // wasm exports
let ctx = null;          // OffscreenCanvas 2D context, or null → frames posted to the page
let audioPort = null, rate = 48000;
// H1791–H1793: the sink's last report {c: frames taken, a: s until the next one reaches the graph's output, b: s taken
// at once, t: when it came} and the AudioContext's own latency (s, from the page)
let sinkReport = { c: 0, a: 0, b: 0, t: 0 }, outLatency = 0;
let audioKind = 'worklet', pumpTarget = 0;
// H1791: the queue depth kept in the sink, frames — the AudioWorklet takes 128 frames at a time (80 ms covers a worker
// frame late by 60 ms: a file read, a decode); the ScriptProcessor 1024 at once (100 ms); a hidden running tab 1 s
function audioTarget() {
  const s = hidden && !pauseHidden ? 1 : audioKind === 'script' ? 0.1 : 0.08;
  return Math.round(rate * s);
}
let pending = null;      // bytes of the last fetch, until xf_js_take
let loading = true;      // until the engine has started (fetches are reported to the page's loading screen)
let hidden = false;      // the page is hidden: nothing drawn; the clock stops unless pauseHidden is off
let pauseHidden = true;  // H1730 / H1681: hidden = paused (default); off = the game runs on in a hidden tab
let paused = false;      // H1753: the settings dialog shows — no ticks, nothing drawn
let loop = 0;            // the running loop's id: a stale animation-frame callback (after hide / show) does nothing
const BG_MS = 100;        // H1731: the hidden-tab loop's period (a timer; the browser may stretch it — the clock
                         // follows the real elapsed time and catches up in steps of at most STEP_MS)
const STEP_MS = 250, CATCH_MAX = 240; // at most 60 s of game time per hidden iteration (the rest the next one)
const FRAME_MS = 1000 / 60 - 2; // at most ~60 frames a second (the window: vsync + 10 ms sleep); a 120 Hz display
                         // would otherwise double the work for no visible gain
const stats = { req: 0, bytes: 0, frames: 0, peak: 0 };
const dec = new TextDecoder();
const enc = new TextEncoder();

let saves = 'memory';   // 'server' | 'idb' | 'memory'
const local = new Map(); // file name → bytes (IndexedDB / memory modes)
let idb = null;

function idbReq(r) { return new Promise((ok, fail) => { r.onsuccess = () => ok(r.result); r.onerror = () => fail(r.error); }); }

// Where the saves live: the page's choice (`want`, www/saves.js: the server's /saves-mode, H1733), else probe the
// server; else open IndexedDB and read every slot file into `local`.
async function initSaves(want) {
  if (want === 'server' || !want) {
    try {
      const r = await fetch('/saves/', { cache: 'no-store' });
      if (r.ok) { saves = 'server'; return; }
    } catch (e) { /* no server saves */ }
  }
  if (want === 'memory') { saves = 'memory'; return; }
  try {
    const open = indexedDB.open('xf-saves', 1);
    open.onupgradeneeded = () => open.result.createObjectStore('files');
    idb = await idbReq(open);
    await idbLoad();
    saves = 'idb';
  } catch (e) {
    console.warn('[xf] no IndexedDB — saves last until the page is closed:', e);
    saves = 'memory';
  }
}

// every slot file of IndexedDB into `local` (at the start, and after the page imported files: H1734)
async function idbLoad() {
  const st = idb.transaction('files', 'readonly').objectStore('files');
  const [keys, vals] = await Promise.all([idbReq(st.getAllKeys()), idbReq(st.getAll())]);
  local.clear();
  keys.forEach((k, i) => local.set(k, new Uint8Array(vals[i])));
}

function idbPut(name, bytes) {
  if (!idb) return;
  try {
    const tx = idb.transaction('files', 'readwrite');
    tx.objectStore('files').put(bytes, name);
    tx.onerror = () => postMessage({ t: 'error', msg: 'save to browser storage failed: ' + tx.error });
  } catch (e) { postMessage({ t: 'error', msg: 'save to browser storage failed: ' + e }); }
}

const saveUrl = (name) => '/saves/' + encodeURIComponent(name);

const mem = () => new Uint8Array(x.memory.buffer);
const str = (p, n) => dec.decode(mem().subarray(p, p + n));

function url(path) {
  const m = /^(data|fonts|cache|en)\/(.+)$/.exec(path);
  if (!m) return null;
  return '/' + m[1] + '/' + m[2].split('/').map(encodeURIComponent).join('/');
}

const imports = {
  env: {
    xf_js_log(p, n, level) {
      const s = str(p, n);
      if (level) { console.error('[xf] ' + s); postMessage({ t: 'error', msg: s }); } else console.log('[xf] ' + s);
    },
    xf_js_fetch(p, n, off, len) {
      const u = url(str(p, n));
      if (!u) return -1;
      if (len === 0) { pending = new Uint8Array(0); return 0; }
      const r = new XMLHttpRequest();
      r.open('GET', u, false);
      r.responseType = 'arraybuffer';
      if (len > 0) r.setRequestHeader('Range', `bytes=${off}-${off + len - 1}`);
      try { r.send(); } catch (e) { return -1; }
      if (r.status !== 200 && r.status !== 206) return -1;
      let b = new Uint8Array(r.response);
      if (len > 0 && r.status === 200) b = b.subarray(off, off + len); // the server ignored the range
      stats.req++; stats.bytes += b.length;
      if (!x || loading) postMessage({ t: 'progress', stage: 'files', got: stats.bytes, n: stats.req });
      pending = b;
      return b.length;
    },
    // op 0 read (→ pending), 1 write, 2 size; −1 = none / failed
    xf_js_save(op, np, nl, dp, dl) {
      const name = str(np, nl);
      if (saves !== 'server') {
        if (op === 1) { const b = mem().slice(dp, dp + dl); local.set(name, b); idbPut(name, b); return 0; }
        const b = local.get(name);
        if (!b) return -1;
        if (op === 0) pending = b;
        return b.length;
      }
      const r = new XMLHttpRequest();
      r.open(['GET', 'PUT', 'HEAD'][op], saveUrl(name), false);
      if (op === 0) r.responseType = 'arraybuffer';
      try { r.send(op === 1 ? mem().slice(dp, dp + dl) : null); } catch (e) { return -1; }
      if (op === 1) {
        if (r.status === 204 || r.status === 200) return 0;
        postMessage({ t: 'error', msg: `save ${name}: HTTP ${r.status} ${r.responseText || ''}`.trim() });
        return -1;
      }
      if (r.status !== 200) return -1;
      if (op === 2) return +(r.getResponseHeader('Content-Length') || 0);
      pending = new Uint8Array(r.response);
      return pending.length;
    },
    xf_js_take(dst) {
      mem().set(pending, dst);
      pending = null;
    },
  },
};

function setting(k, v) {
  const [kp, kn] = putString(k), [vp, vn] = putString(v);
  x.xf_set(kp, kn, vp, vn);
  x.xf_free(kp, kn); x.xf_free(vp, vn);
}

// the settings as the engine has them, and an 80×50 crop of the frame around the pointer (the dialog's scale preview)
function settingsState() {
  const p = x.xf_settings();
  const text = p ? str(p, x.xf_settings_len()) : '';
  const cw = 80, ch = 50, m = x.xf_mouse();
  const [mx, my] = m === 0xffffffff ? [W / 2, H / 2] : [m >>> 16, m & 0xffff];
  const cx = Math.max(0, Math.min(W - cw, mx - cw / 2)), cy = Math.max(0, Math.min(H - ch, my - ch / 2));
  const f = new Uint8Array(x.memory.buffer, x.xf_frame(), W * H * 4);
  const buf = new Uint8ClampedArray(cw * ch * 4);
  for (let r = 0; r < ch; r++) buf.set(f.subarray(((cy + r) * W + cx) * 4, ((cy + r) * W + cx + cw) * 4), r * cw * 4);
  postMessage({ t: 'settings', text, crop: { buf: buf.buffer, w: cw, h: ch } }, [buf.buffer]);
}

function putString(s) {
  const b = enc.encode(s);
  const p = x.xf_alloc(b.length);
  mem().set(b, p);
  return [p, b.length];
}

// H956: the subtitle pack by language name ('auto' | 'en' | 'ru' | 'off'); `on` = also switch the captions on
function setSubs(lang, on) {
  const [p, n] = putString(String(lang));
  const r = x.xf_subs(p, n, on);
  x.xf_free(p, n);
  return r;
}

// the engine module, its download reported to the page (Content-Length known: a progress bar)
async function fetchWasm(u) {
  const r = await fetch(u);
  if (!r.ok) throw new Error(u + ': HTTP ' + r.status + ' — run port/web/build.sh');
  const total = +(r.headers.get('Content-Length') || 0);
  if (!r.body || !total) return r.arrayBuffer();
  const parts = [], rd = r.body.getReader();
  let got = 0;
  for (;;) {
    const { done, value } = await rd.read();
    if (done) break;
    parts.push(value);
    got += value.length;
    postMessage({ t: 'progress', stage: 'wasm', got: Math.min(got, total), total });
  }
  const buf = new Uint8Array(got);
  let o = 0;
  for (const p of parts) { buf.set(p, o); o += p.length; }
  return buf;
}

async function init(m) {
  hidden = !!m.hidden; // later {t:'visible'} messages update it
  pauseHidden = m.pauseHidden !== false;
  const [index, trace, enIndex, wasm] = await Promise.all([
    fetch('/data-index').then((r) => (r.ok ? r.text() : Promise.reject(new Error('/data-index: HTTP ' + r.status)))),
    fetch('/trace.jsonl').then((r) => (r.ok ? r.text() : '')).catch(() => ''),
    // H1837: the English data set (xf-serve --en-data DIR), optional
    fetch('/en-index').then((r) => (r.ok ? r.text() : '')).catch(() => ''),
    fetchWasm('pkg/xf_web.wasm'),
    initSaves(m.saves),
  ]);
  console.log('[xf] saves: ' + { server: 'on the server (/saves/)', idb: 'in this browser (IndexedDB)', memory: 'in memory only' }[saves]);
  postMessage({ t: 'saves', where: saves });
  const { instance } = await WebAssembly.instantiate(wasm, imports);
  x = instance.exports;
  if (m.canvas) ctx = m.canvas.getContext('2d', { alpha: false });
  audioPort = m.audioPort || null;
  rate = m.rate || 48000;
  outLatency = m.audioLatency || 0;
  if (audioPort) audioPort.onmessage = (e) => { sinkReport = { c: e.data.c, a: e.data.a, b: e.data.b, t: performance.now() }; };
  postMessage({ t: 'progress', stage: 'start' });
  const [ip, il] = putString(index);
  const [tp, tl] = putString(trace);
  const t0 = performance.now();
  // H1660 / H1661: start 0 = intro movies then the main menu, 1 = the menu at once, 2 = New Game at once
  const ok = x.xf_init(ip, il, tp, tl, m.hyp === false ? 0 : 1, { menu: 1, new: 2 }[m.start] || 0);
  x.xf_free(ip, il); x.xf_free(tp, tl);
  loading = false;
  if (!ok) throw new Error('engine start failed (see the console)');
  x.xf_audio(rate, m.volume);
  // H1791: the queue depth kept in the sink (audioTarget)
  audioKind = m.audioKind || 'worklet';
  x.xf_pump_target(audioTarget());
  if (m.longTimers) x.xf_long_timers(m.longTimers === 'nt' ? 1 : 0); // C191 / H1203: the bomb option
  // H1836 / H1837: the English data set (before the settings: their game language needs it); `?game-lang=en`
  if (enIndex) {
    const [ep, el] = putString(enIndex);
    const has = x.xf_en_data(ep, el);
    x.xf_free(ep, el);
    if (has && m.gameLang === 'en') setting('game_lang', 'en');
  }
  // H1750: the page's remembered settings (the engine checks each; a bad one is logged and skipped)
  for (const l of String(m.settings || '').split('\n')) {
    const i = l.indexOf('=');
    if (i > 0) setting(l.slice(0, i).trim(), l.slice(i + 1).trim());
  }
  if (m.subs && m.subs !== 'auto') setSubs(m.subs, 0); // H956: the subtitle language (init already loaded `auto`)
  console.log(`[xf] started in ${Math.round(performance.now() - t0)} ms, ${stats.req} requests, ${(stats.bytes / 1048576).toFixed(1)} MiB`);
  postMessage({ t: 'ready' });
  if (m.every > 0) { testRun(m.every, m.frames); return; }
  running = true;
  resume();
}

// (re)start the loop: the clock restarts from now (a paused stretch is not game time; the audio pump follows the
// sink's reports, H1792: no burst after it)
function resume() {
  last = performance.now();
  frameAt = 0;
  schedule();
}

// test mode: `frames` ticks of `every` ms, one rendered frame each, then stop (input still works, the clock does not)
function testRun(every, frames) {
  for (let i = 0; i < frames; i++) x.xf_tick(every);
  const p = x.xf_frame();
  if (ctx && p) ctx.putImageData(new ImageData(new Uint8ClampedArray(x.memory.buffer, p, W * H * 4), W, H), 0, 0);
  postMessage({ t: 'status', view: x.xf_view(), now: x.xf_now(), frames, req: stats.req, mib: stats.bytes / 1048576 });
  postMessage({ t: 'test-done' });
}

let last = 0, statusAt = 0, frameAt = 0, running = false;
const work = { ms: 0, n: 0 }; // engine + draw time of the frames since the last status
const raf = typeof self.requestAnimationFrame === 'function' ? (f) => self.requestAnimationFrame(f) : (f) => setTimeout(f, 16);
// visible: one frame per animation frame; hidden + pauseHidden: nothing (no CPU); hidden + running: the timer loop
function schedule() {
  const id = ++loop;
  if (paused) return;
  if (!hidden) raf(() => frame(id));
  else if (!pauseHidden) setTimeout(() => background(id), BG_MS);
}

// H1731: the hidden tab's loop with "pause when hidden" off — the game clock follows the real elapsed time (a
// throttled or late timer is caught up in steps of at most STEP_MS, the window's per-tick cap), nothing is drawn, the
// sound is mixed ahead (the AudioContext keeps playing).
function background(id) {
  if (id !== loop || !hidden || pauseHidden || paused) return;
  try {
    if (catchUp(performance.now())) return;
  } catch (e) {
    postMessage({ t: 'error', msg: 'engine stopped: ' + e });
    return;
  }
  schedule();
}

// the game clock up to `now` in steps of at most STEP_MS (at most CATCH_MAX steps); true = the game quit
function catchUp(now) {
  for (let i = 0; i < CATCH_MAX && now - last >= 1; i++) {
    const dt = Math.min(STEP_MS, Math.floor(now - last));
    last += dt;
    x.xf_step(dt);
    if (x.xf_quit()) { postMessage({ t: 'quit' }); return true; }
  }
  pumpAudio(performance.now());
  return false;
}

function frame(id) {
  if (id !== loop || hidden || paused) return;
  try {
    const now = performance.now();
    if (now - frameAt < FRAME_MS) { schedule(); return; }
    frameAt = now;
    const dt = Math.max(0, Math.floor(now - last));
    last += dt;
    const p = x.xf_tick(Math.min(dt, 250));  // the window's `dt.min(250)`
    if (p) {
      const px = new Uint8ClampedArray(x.memory.buffer, p, W * H * 4);
      if (ctx) ctx.putImageData(new ImageData(px, W, H), 0, 0);
      else { const copy = px.slice(); postMessage({ t: 'frame', buf: copy.buffer }, [copy.buffer]); }
      stats.frames++;
    }
    const end = performance.now();
    pumpAudio(end);
    work.ms += end - now; work.n++;
    if (x.xf_quit()) { postMessage({ t: 'quit' }); return; }
    if (x.xf_settings_req()) postMessage({ t: 'open-settings' });
    // H2100: a Save wrote the host's Preferences (the settings' host_prefs): the page keeps them
    if (x.xf_prefs_saved && x.xf_prefs_saved()) settingsState();
    if (now - statusAt > 500) {
      const fps = statusAt ? (work.n * 1000) / (now - statusAt) : 0;
      postMessage({ t: 'status', view: x.xf_view(), now: x.xf_now(), frames: stats.frames, req: stats.req, mib: stats.bytes / 1048576, sound: stats.peak > 0,
        fps, ms: work.n ? work.ms / work.n : 0 });
      statusAt = now;
      stats.peak = 0; work.ms = 0; work.n = 0;
    }
  } catch (e) {
    postMessage({ t: 'error', msg: 'engine stopped: ' + e });
    return;
  }
  schedule();
}

// H1791–H1795: keep the sink's queue at its target from the sink's own report of what it took (the old pump kept a
// 120 ms lead over the wall clock: that lead was heard as the sound lagging the picture, and it grew with the audio
// clock's drift up to the sink's cap); the engine delays the picture by the estimated latency (xf_pump). In a hidden
// running tab (H1731) the loop wakes rarely: the target is 1 s (audio.js queues up to 1.5 s).
function pumpAudio(now) {
  if (!audioPort) return;
  const t = audioTarget();
  if (t !== pumpTarget) { pumpTarget = t; x.xf_pump_target(t); }
  const r = sinkReport;
  const n = x.xf_pump(r.c, r.a, r.b, outLatency, r.t ? (now - r.t) / 1000 : 0);
  if (n <= 0) return;
  const p = x.xf_pcm();
  if (!p) return;
  const out = new Float32Array(x.memory.buffer, p, n * 2).slice();
  for (let i = 0; i < out.length; i += 16) stats.peak = Math.max(stats.peak, Math.abs(out[i]));
  audioPort.postMessage(out, [out.buffer]);
}

onmessage = (e) => {
  const m = e.data;
  if (m.t === 'init') { init(m).catch((err) => postMessage({ t: 'error', msg: String(err && err.message || err) })); return; }
  if (m.t === 'visible' || m.t === 'pause-hidden') {
    const was = hidden && pauseHidden; // paused
    if (m.t === 'visible') hidden = !m.v; else pauseHidden = !!m.v;
    if (!x || loading || !running || paused) return; // the settings dialog's pause: its end restarts the clocks
    // H1730: from a paused stretch the clocks restart from now (no catch-up); a running hidden stretch is caught up
    // to now, then the loop goes on (animation frames when shown, the timer when hidden)
    if (was) { if (!(hidden && pauseHidden)) resume(); }
    else if (hidden && pauseHidden) ++loop; // pause: stale callbacks do nothing
    else { if (catchUp(performance.now())) return; schedule(); }
    return;
  }
  if (m.t === 'pause') {
    if (!!m.v === paused) return;
    paused = !!m.v;
    if (!x || loading || !running) return;
    if (paused) ++loop; // stale callbacks do nothing
    else resume();
    return;
  }
  if (!x) return;
  switch (m.t) {
    case 'move': postMessage({ t: 'pointer', p: x.xf_motion(m.x, m.y, m.inside ? 1 : 0) }); break;
    case 'down': x.xf_button(m.b, m.x, m.y, m.inside ? 1 : 0); break;
    case 'up': x.xf_release(m.b, m.x, m.y, m.inside ? 1 : 0); break;
    case 'wheel': x.xf_wheel(m.lines, m.x, m.y, m.inside ? 1 : 0); break;
    case 'key': { const [p, n] = putString(m.key); x.xf_key(p, n); x.xf_free(p, n); break; }
    // H2060: an Alt hotkey ('A'..'Z' / '0'..'9'); the page takes the volume / settings part (xf_alt's code)
    case 'alt': postMessage({ t: 'alt', r: x.xf_alt(String(m.key).charCodeAt(0), m.shift ? 1 : 0) }); break;
    case 'volume': x.xf_audio(rate, m.v); break;
    case 'audio-latency': outLatency = m.s || 0; break;
    case 'long-timers': x.xf_long_timers(m.nt ? 1 : 0); break;
    case 'game-lang': x.xf_game_lang(m.lang === 'en' ? 1 : m.lang === 'de' ? 2 : 0); break;
    // H1734: the page imported slot files — the mirror and the engine's slot names read again
    case 'saves-reload':
      (saves === 'idb' ? idbLoad() : Promise.resolve())
        .then(() => { x.xf_saves_reload(); postMessage({ t: 'saves-reloaded' }); })
        .catch((e) => postMessage({ t: 'error', msg: 'reading the imported saves failed: ' + e }));
      break;
    case 'subs': setSubs(m.lang, 1); break;
    case 'set': setting(m.key, m.value); break;
    case 'get-settings': settingsState(); break;
  }
};
