// Page side of the web front: start screen (the click unlocks audio), loading progress, canvas layout (4:3, CSS
// scaling), input → worker (picture coordinates 640×480 as in the SDL window), audio sink (AudioWorklet,
// ScriptProcessor fallback), the settings dialog (H1750-H1755: the keys of xf::front::settings, kept in localStorage),
// pause while the tab is hidden (an option, H1730), the saves panel (export / import of the slots, H1734; www/saves.js).
'use strict';
const W = 640, H = 480;
const $ = (id) => document.getElementById(id);
const canvas = $('screen'), stage = $('stage'), startBtn = $('start'), keysBox = $('keys'), setBox = $('settings');
let worker = null, ready = false, audioCtx = null;

// ---- settings (H1750: the keys and values of xf::front::settings — the engine checks and applies them, `xf_set`).
// localStorage may be missing / blocked (private mode, sandboxed previews): then nothing is kept.
const SETTINGS = 'xf-settings';
const settings = (() => {
  let v = {};
  try { v = JSON.parse(localStorage.getItem(SETTINGS)) || {}; } catch (e) { /* defaults */ }
  // the older names (scale pixelated / smooth, timers, hidden)
  if (v.scale === 'pixelated') v.scale = 'nearest';
  if (v.scale === 'smooth') v.scale = 'linear';
  if (v.timers && !v.long_timers) v.long_timers = v.timers;
  if (v.hidden && !v.pause_hidden) v.pause_hidden = v.hidden;
  delete v.timers; delete v.hidden;
  return v;
})();
function keep(k, v) {
  settings[k] = v;
  try { localStorage.setItem(SETTINGS, JSON.stringify(settings)); } catch (e) { /* not kept */ }
}
// `?long-timers=nt|wine` (C191 / H1203) and `?subs=en|ru|auto|off` (H956) preselect for this page load
{
  const q = new URLSearchParams(location.search);
  const t = q.get('long-timers'), su = q.get('subs');
  if (t === 'nt' || t === 'wine') settings.long_timers = t;
  if (['auto', 'en', 'ru', 'off'].includes(su)) settings.subs = su;
}
const field = (k) => setBox.querySelector(`[data-key="${k}"]`);
// the engine's keys as `key = value` lines for init (the page's own scale / fullscreen / pause_hidden, and the
// subtitle language, which init takes as m.subs, left out)
function engineSettings() {
  return Object.entries(settings).filter(([k]) => !['scale', 'fullscreen', 'pause_hidden', 'subs'].includes(k))
    .map(([k, v]) => `${k} = ${v}`).join('\n');
}
// "pause when hidden" (H1730): on (default) = a hidden tab stops the game clock and the sound (no CPU, H1681); off =
// the game runs on in a hidden tab (its clock follows the real time, nothing is drawn, the sound plays)
const pauseHidden = () => settings.pause_hidden !== 'off';

// ---- layout: the largest 4:3 box that fits the stage, snapped to device pixels
function layout() {
  const r = stage.getBoundingClientRect();
  const dpr = devicePixelRatio || 1;
  const s = Math.max(0.1, Math.min(r.width / W, r.height / H));
  canvas.style.width = Math.floor(W * s * dpr) / dpr + 'px';
  canvas.style.height = Math.floor(H * s * dpr) / dpr + 'px';
}
if (typeof ResizeObserver === 'function') new ResizeObserver(layout).observe(stage);
addEventListener('resize', layout);
layout();

function setScale(v) {
  canvas.classList.toggle('smooth', v === 'linear');
  field('scale').value = v;
  keep('scale', v);
  previewMark();
}
if (settings.scale === 'linear') setScale('linear');

const volume = () => (Number.isFinite(+settings.volume) ? +settings.volume : 100) / 100;
function showVolume() { field('volume').value = Math.round(volume() * 100); $('volpct').textContent = Math.round(volume() * 100) + ' %'; }
showVolume();

// one change from the dialog: kept, then the page's own or sent to the engine (which checks it again)
function changed(k, v) {
  if (k === 'fullscreen') { if ((v === 'on') !== !!document.fullscreenElement) toggleFull(); return; }
  if (k === 'scale') { setScale(v); return; }
  keep(k, k === 'volume' ? +v : v);
  if (k === 'volume') showVolume();
  if (!worker) return;
  if (k === 'pause_hidden') worker.postMessage({ t: 'pause-hidden', v: pauseHidden() });
  // H956: the subtitle language (a language also switches the captions on)
  else if (k === 'subs') worker.postMessage({ t: 'subs', lang: v });
  else worker.postMessage({ t: 'set', key: k, value: String(v) });
}
for (const el of setBox.querySelectorAll('[data-key]')) {
  const k = el.dataset.key;
  if (k !== 'volume' && k !== 'fullscreen' && settings[k] !== undefined) el.value = settings[k];
  el.addEventListener(el.type === 'range' ? 'input' : 'change', () => changed(k, el.value));
}

// ---- the settings dialog: opened by F10, the bar's button or the menu screen's "Port settings" (worker
// 'open-settings'); the game is paused while it shows; the engine reports the session's state (F1 / F3 / the game's
// captions preference, the values the game data offers) and a crop of the frame for the scale preview
function previewMark() {
  const smooth = settings.scale === 'linear';
  $('pv-near-f').classList.toggle('on', !smooth);
  $('pv-smooth-f').classList.toggle('on', smooth);
}
previewMark();
function showSettings(on) {
  setBox.hidden = !on;
  $('setbtn').setAttribute('aria-expanded', String(on));
  if (on) { showKeys(false); if (!savesBox.hidden) showSaves(false); }
  if (worker && ready) worker.postMessage({ t: 'pause', v: on });
  if (on && worker && ready) worker.postMessage({ t: 'get-settings' });
}
$('setbtn').onclick = () => showSettings(setBox.hidden);
$('setclose').onclick = () => showSettings(false);
setBox.addEventListener('pointerdown', (e) => e.stopPropagation());
function engineState(text, crop) {
  const v = {};
  for (const l of text.split('\n')) { const i = l.indexOf('='); if (i > 0 && l[0] !== '#') v[l.slice(0, i).trim()] = l.slice(i + 1).trim(); }
  // H2305: only the values the game data offers (`choices.<key> = a,b`); an option with one value is not shown
  for (const [key, list] of Object.entries(v)) {
    if (!key.startsWith('choices.')) continue;
    const el = field(key.slice(8)), ok = list.split(',');
    if (!el) continue;
    for (const o of el.options) o.hidden = o.disabled = !ok.includes(o.value);
    el.closest('tr').hidden = ok.length < 2;
  }
  for (const k of ['overlay', 'captions', 'subs', 'long_timers', 'game_lang']) if (v[k] !== undefined) field(k).value = v[k];
  if (v.overlay !== undefined) keep('overlay', v.overlay);
  // H2100: the host's Preferences (the original's registry) the engine wrote after a Save
  if (v.host_prefs !== undefined) keep('host_prefs', v.host_prefs);
  if (v.captions && v.captions !== 'game') keep('captions', v.captions);
  $('capnow').textContent = v.captions_now ? `(now ${v.captions_now})` : '';
  field('fullscreen').value = document.fullscreenElement ? 'on' : 'off';
  if (crop) for (const id of ['pv-near', 'pv-smooth']) $(id).getContext('2d').putImageData(new ImageData(new Uint8ClampedArray(crop.buf), crop.w, crop.h), 0, 0);
}

// ---- full screen: the stage only (the picture fills the screen); where the browser offers a keyboard lock, Esc is
// kept for the game's menu (the user holds Esc to leave full screen)
function toggleFull() {
  if (document.fullscreenElement) { document.exitFullscreen().catch(() => {}); return; }
  const el = stage;
  const req = el.requestFullscreen || el.webkitRequestFullscreen;
  if (!req) { showError('full screen is not available in this browser'); return; }
  Promise.resolve(req.call(el, { navigationUI: 'hide' }))
    .then(() => navigator.keyboard && navigator.keyboard.lock && navigator.keyboard.lock(['Escape']))
    .catch(() => {});
}
$('full').onclick = toggleFull;
document.addEventListener('fullscreenchange', () => {
  $('full').textContent = document.fullscreenElement ? 'exit full screen' : 'full screen';
  field('fullscreen').value = document.fullscreenElement ? 'on' : 'off';
  if (!document.fullscreenElement && navigator.keyboard && navigator.keyboard.unlock) navigator.keyboard.unlock();
  layout();
});

// ---- the controls panel
function showKeys(on) {
  keysBox.hidden = !on;
  $('keysbtn').setAttribute('aria-expanded', String(on));
  if (on && !setBox.hidden) showSettings(false);
}
$('keysbtn').onclick = () => showKeys(keysBox.hidden);
keysBox.addEventListener('pointerdown', (e) => e.stopPropagation());
startBtn.querySelector('u').addEventListener('click', (e) => { e.stopPropagation(); showKeys(true); });

// the bar's controls give the focus back after use (else Space / Enter would press a focused button again, not reach
// the game)
for (const el of document.querySelectorAll('#bar button, #bar select, #bar input, #settings select, #settings input, #settings button')) {
  el.addEventListener(el.tagName === 'BUTTON' ? 'click' : 'change', () => el.blur());
}

// a page point → 640×480 picture point; inside = over the picture (clicks on the bars go nowhere)
function toPicture(e) {
  const r = canvas.getBoundingClientRect();
  const x = Math.floor(((e.clientX - r.left) / r.width) * W);
  const y = Math.floor(((e.clientY - r.top) / r.height) * H);
  return { x, y, inside: x >= 0 && y >= 0 && x < W && y < H };
}

// ---- audio
async function audioSink() {
  const ctx = new AudioContext();
  audioCtx = ctx;
  const ch = new MessageChannel();
  let kind = 'worklet';
  try {
    if (!ctx.audioWorklet) throw new Error('no AudioWorklet (not a secure context?)');
    await ctx.audioWorklet.addModule('audio.js');
    const node = new AudioWorkletNode(ctx, 'xf-sink', { numberOfInputs: 0, outputChannelCount: [2] });
    node.connect(ctx.destination);
    node.port.postMessage({ port: ch.port1 }, [ch.port1]);
  } catch (e) {
    console.info('[xf] AudioWorklet unavailable, ScriptProcessor fallback:', e.message);
    await new Promise((ok, fail) => {
      const s = document.createElement('script');
      s.src = 'audio.js'; s.onload = ok; s.onerror = fail;
      document.head.appendChild(s);
    });
    const q = new self.XfAudioQueue(ctx.sampleRate);
    ch.port1.onmessage = (m) => q.push(m.data);
    // 1024 frames (~21 ms; was 2048): the buffer adds to the sound's latency (H1793); the page's main thread is idle
    // (the engine runs in the worker)
    const sp = ctx.createScriptProcessor(1024, 0, 2);
    sp.onaudioprocess = (ev) => {
      const buf = ev.outputBuffer;
      q.pull(buf.getChannelData(0), buf.getChannelData(1));
      // H1793: this buffer plays from ev.playbackTime; the first frame not taken after it
      const a = Math.max(0, ev.playbackTime - ctx.currentTime) + buf.length / ctx.sampleRate;
      ch.port1.postMessage({ c: q.consumed, a, b: buf.length / ctx.sampleRate });
    };
    sp.connect(ctx.destination);
    kind = 'script';
  }
  await ctx.resume();
  return { port: ch.port2, rate: ctx.sampleRate, kind, latency: outputLatency(ctx) };
}
// H1793: the AudioContext's own latency, graph output → speaker (baseLatency + outputLatency where the browser has them)
function outputLatency(ctx) {
  return (ctx.baseLatency || 0) + (ctx.outputLatency || 0);
}
// a browser may suspend the context (Safari after an interruption, autoplay rules): any later gesture resumes it
function wakeAudio() {
  if (audioCtx && audioCtx.state === 'suspended' && !document.hidden) audioCtx.resume().catch(() => {});
}

// ---- hidden tab: with "pause when hidden" on the worker stops its loop (no CPU, the game clock stops) and the audio
// context is suspended; off, the worker goes on without drawing and the sound plays on
document.addEventListener('visibilitychange', () => {
  const v = !document.hidden, pause = pauseHidden();
  if (worker) worker.postMessage({ t: 'visible', v });
  $('paused').hidden = v || !ready || !pause;
  if (audioCtx && (pause || v)) (v ? audioCtx.resume() : audioCtx.suspend()).catch(() => {});
});

// ---- loading: the wasm download (~20 %), then the game files the engine reads at start (~80 %, against the byte
// count of the last start in this browser — a first start guesses)
const LOAD_BYTES = 'xf-start-bytes';
let expectBytes = 55 << 20;
try { expectBytes = +localStorage.getItem(LOAD_BYTES) || expectBytes; } catch (e) { /* default */ }
let lastFiles = 0;
function progress(m) {
  let f = 0, text = 'loading…';
  if (m.stage === 'wasm') { f = 0.2 * (m.got / m.total); text = `loading the engine… ${(m.got / 1048576).toFixed(1)} / ${(m.total / 1048576).toFixed(1)} MiB`; }
  else if (m.stage === 'files') { lastFiles = m.got; f = 0.2 + 0.78 * Math.min(1, m.got / expectBytes); text = `reading game files… ${m.n} files, ${(m.got / 1048576).toFixed(1)} MiB`; }
  else if (m.stage === 'start') { f = 0.2; text = 'starting…'; }
  $('progress').firstChild.style.width = (f * 100).toFixed(1) + '%';
  $('startmsg').textContent = text;
}

let frameCtx = null;
async function start() {
  if (startBtn.disabled) return;
  startBtn.disabled = true;
  $('progress').hidden = false;
  $('startmsg').textContent = 'loading…';
  let audio = null;
  try { audio = await audioSink(); } catch (e) { console.warn('[xf] no sound:', e); }
  const savesAt = await XfSaves.detect().catch(() => undefined);
  worker = new Worker('worker.js');
  worker.onmessage = onWorker;
  worker.onerror = (e) => showError('worker: ' + e.message);
  const msg = { t: 'init', rate: audio ? audio.rate : 48000, volume: volume(), settings: engineSettings(), subs: settings.subs || 'auto',
    audioPort: audio && audio.port, audioKind: audio && audio.kind, audioLatency: audio ? audio.latency : 0,
    hidden: document.hidden, pauseHidden: pauseHidden(), saves: savesAt };
  // test mode (?every=MS&frames=N): like `xf --frames N --dump DIR --hyp --every MS` — N ticks of MS game-ms each,
  // then the clock stops (compare the picture with the native dump's last frame)
  const q = new URLSearchParams(location.search);
  if (q.has('every')) { msg.every = +q.get('every'); msg.frames = +(q.get('frames') || 1); msg.start = 'new'; }
  // the start (H1660 / H1661): the intro movies then the main menu; `?start=menu` = the menu at once, `?start=new` = New
  // Game at once (the test mode always starts with New Game, as `xf --frames N --dump DIR`)
  if (q.has('start')) msg.start = q.get('start');
  // H1836: the game language (voice-over + the game's texts) when the server has the English data (xf-serve --en-data)
  if (q.has('game-lang')) msg.gameLang = q.get('game-lang');
  const transfer = audio ? [audio.port] : [];
  if (canvas.transferControlToOffscreen) {
    msg.canvas = canvas.transferControlToOffscreen();
    transfer.push(msg.canvas);
  } else {
    frameCtx = canvas.getContext('2d', { alpha: false });
  }
  worker.postMessage(msg, transfer);
  // the output latency may change (another output device, Bluetooth): the worker gets it again every 2 s
  if (audio) setInterval(() => worker.postMessage({ t: 'audio-latency', s: outputLatency(audioCtx) }), 2000);
}
startBtn.onclick = start;

function showError(m) {
  $('err').textContent = m;
  console.error('[xf]', m);
  if (!ready) { $('startmsg').textContent = m; $('startmsg').classList.add('err'); }
}

function onWorker(e) {
  const m = e.data;
  switch (m.t) {
    case 'progress': if (!ready) progress(m); break;
    case 'ready':
      ready = true; startBtn.hidden = true;
      if (!setBox.hidden) showSettings(true);
      if (lastFiles) try { localStorage.setItem(LOAD_BYTES, String(lastFiles)); } catch (err) { /* not kept */ }
      break;
    // the game menu's Quit: back to the start screen (a fresh page — the canvas went to the worker)
    case 'quit': worker.terminate(); location.reload(); break;
    case 'test-done': document.body.dataset.test = 'done'; break;
    case 'open-settings': showSettings(true); break;
    case 'settings': engineState(m.text, m.crop); break;
    // H2060: the page's part of an Alt hotkey — the volume (H2061) kept and shown, captions / hotspots read back
    case 'alt':
      if (m.r >= 0x100) { keep('volume', m.r - 0x100); showVolume(); }
      else if (m.r === 3) worker.postMessage({ t: 'get-settings' });
      else if (m.r === 2) console.log('[xf] Alt+X: exit - nothing to do in the browser (close the tab)');
      break;
    case 'error': showError(m.msg); break;
    case 'saves-reloaded': if (!savesBox.hidden) showSaves(true); break;
    case 'pointer': canvas.style.cursor = ['none', 'pointer', 'default'][m.p] || 'default'; break;
    case 'frame': frameCtx.putImageData(new ImageData(new Uint8ClampedArray(m.buf), W, H), 0, 0); break;
    case 'status': {
      const view = m.view === 0xffffffff ? '-' : '0x' + m.view.toString(16);
      const perf = m.fps ? ` · ${m.fps.toFixed(0)} fps, ${m.ms.toFixed(1)} ms` : '';
      $('status').textContent = `view ${view} · game ${(m.now / 1000).toFixed(1)} s${perf} · ${m.req} reads, ${m.mib.toFixed(1)} MiB${m.sound ? ' · ♪' : ''}`;
      break;
    }
  }
}

// ---- the saves panel (H1734): where the slots live; download one slot (its .x file) or all (a zip of slots.txt + the
// .x files — the window's saves dir layout); upload a .x into a slot, or a zip / slots.txt + .x files for all three.
const savesBox = $('saves');
const WHERE = { server: 'on the server (every browser that opens this page shares them)', idb: 'in this browser (IndexedDB) — download them to keep a copy',
  memory: 'in memory only (this browser keeps no storage here): they end with the page' };
function savesMsg(t, err) { $('savesmsg').textContent = t; $('savesmsg').classList.toggle('err', !!err); }
function download({ name, blob }) {
  const a = document.createElement('a');
  a.href = URL.createObjectURL(blob);
  a.download = name;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(a.href), 10000);
}
// a file input's files → [{name, bytes}]
const readPicked = (input) => Promise.all([...input.files].map(async (f) => ({ name: f.name, bytes: new Uint8Array(await f.arrayBuffer()) })));
const reloadWorker = () => { if (worker && ready) worker.postMessage({ t: 'saves-reload' }); };
let importSlot = 0;
async function showSaves(on) {
  savesBox.hidden = !on;
  $('savesbtn').setAttribute('aria-expanded', String(on));
  if (!on) return;
  if (!keysBox.hidden) showKeys(false);
  if (!setBox.hidden) showSettings(false);
  try {
    const mode = await XfSaves.detect();
    $('saveswhere').textContent = WHERE[mode];
    const names = await XfSaves.slots();
    const rows = $('slots');
    rows.textContent = '';
    names.forEach((n, i) => {
      const tr = rows.insertRow();
      tr.insertCell().textContent = `${i + 1}`;
      tr.insertCell().textContent = n || '(empty)';
      const c = tr.insertCell();
      const dl = document.createElement('button');
      dl.type = 'button'; dl.textContent = 'download'; dl.disabled = !n;
      dl.onclick = () => XfSaves.exportSlot(i).then(download).catch((e) => savesMsg(String(e.message || e), true));
      const up = document.createElement('button');
      up.type = 'button'; up.textContent = 'upload…'; up.disabled = mode === 'memory';
      up.onclick = () => { importSlot = i; $('slotfile').value = ''; $('slotfile').click(); };
      c.append(dl, ' ', up);
    });
    $('saveall').disabled = names.every((n) => !n);
    $('loadall').disabled = mode === 'memory';
  } catch (e) {
    savesMsg('the saves cannot be read: ' + (e.message || e), true);
  }
}
$('savesbtn').onclick = () => { savesMsg(''); showSaves(savesBox.hidden); };
savesBox.addEventListener('pointerdown', (e) => e.stopPropagation());
// keys typed in the panel stay there (Esc closes it), not the game's
savesBox.addEventListener('keydown', (e) => { e.stopPropagation(); if (e.key === 'Escape') { e.preventDefault(); showSaves(false); } });
$('savesclose').onclick = () => showSaves(false);
$('saveall').onclick = () => XfSaves.exportAll().then(download).catch((e) => savesMsg(String(e.message || e), true));
$('loadall').onclick = () => { $('allfiles').value = ''; $('allfiles').click(); };
$('slotfile').onchange = async () => {
  try {
    const [f] = await readPicked($('slotfile'));
    if (!f) return;
    const names = await XfSaves.slots();
    const i = importSlot;
    if (names[i] && !confirm(`Replace slot ${i + 1} (${names[i]}) with ${f.name}?`)) return;
    const n = await XfSaves.importSlot(i, f.name, f.bytes);
    savesMsg(`slot ${i + 1} = ${n}`);
    reloadWorker();
  } catch (e) { savesMsg(String(e.message || e), true); }
  showSaves(true);
};
$('allfiles').onchange = async () => {
  try {
    const picked = await readPicked($('allfiles'));
    if (!picked.length) return;
    const names = await XfSaves.slots();
    if (names.some(Boolean) && !confirm('Replace all three slots with the uploaded ones?')) return;
    const n = await XfSaves.importAll(picked);
    savesMsg('slots: ' + n.map((x) => x || '(empty)').join(', '));
    reloadWorker();
  } catch (e) { savesMsg(String(e.message || e), true); }
  showSaves(true);
};

// ---- input (pointer events: mouse, pen and touch). Touch: a tap = a left click, a long press (500 ms without moving)
// = a right click (skips a video), a press that moves = a left-button drag (idea icons, C107).
const LONG_PRESS = 500, SLOP = 10;
let touch = null; // {id, x0, y0, timer, p, down: the left button was sent}
function send(t, b, p) { worker.postMessage({ t, b, ...p }); }
addEventListener('pointermove', (e) => {
  if (!ready) return;
  const p = toPicture(e);
  if (touch && e.pointerId === touch.id && !touch.down && Math.hypot(e.clientX - touch.x0, e.clientY - touch.y0) > SLOP) {
    clearTimeout(touch.timer);
    touch.down = true;
    send('down', 0, touch.p);
  }
  worker.postMessage({ t: 'move', ...p });
});
stage.addEventListener('pointerdown', (e) => {
  wakeAudio();
  if (!keysBox.hidden) showKeys(false);
  if (!savesBox.hidden) showSaves(false);
  if (!setBox.hidden) { showSettings(false); return; }
  if (!ready || (e.button !== 0 && e.button !== 2)) return;
  e.preventDefault();
  const p = toPicture(e);
  worker.postMessage({ t: 'move', ...p });
  if (e.pointerType === 'touch') {
    if (touch) return; // a second finger
    touch = { id: e.pointerId, x0: e.clientX, y0: e.clientY, p, down: false };
    touch.timer = setTimeout(() => {
      if (!touch || touch.down) return;
      send('down', 2, touch.p);
      send('up', 2, touch.p);
      touch.down = 'right';
    }, LONG_PRESS);
    return;
  }
  send('down', e.button, p);
});
// C107: the button-up (anywhere — a drag may end off the stage) drops a dragged idea icon
function pointerEnd(e) {
  if (!ready) return;
  if (touch && e.pointerId === touch.id) {
    clearTimeout(touch.timer);
    const p = toPicture(e);
    if (touch.down === false && e.type === 'pointerup') { send('down', 0, touch.p); send('up', 0, touch.p); }
    else if (touch.down === true) send('up', 0, p);
    touch = null;
    return;
  }
  if (e.type !== 'pointerup' || (e.button !== 0 && e.button !== 2)) return;
  send('up', e.button, toPicture(e));
}
addEventListener('pointerup', pointerEnd);
addEventListener('pointercancel', pointerEnd);
stage.addEventListener('contextmenu', (e) => e.preventDefault());
// H1815 (port convenience — the original has no wheel): the wheel scrolls the dialogue box / the computer's texts; one
// notch = 100 px of pixel delta (a trackpad's small deltas add up), a line-mode delta of 3 lines, a page-mode delta of 1
let wheelAcc = 0;
stage.addEventListener('wheel', (e) => {
  if (!ready) return;
  e.preventDefault();
  wheelAcc += e.deltaMode === 1 ? e.deltaY * (100 / 3) : e.deltaMode === 2 ? e.deltaY * 100 : e.deltaY;
  const lines = Math.trunc(wheelAcc / 100);
  if (lines === 0) return;
  wheelAcc -= lines * 100;
  worker.postMessage({ t: 'wheel', lines, ...toPicture(e) });
}, { passive: false });
addEventListener('keydown', (e) => {
  wakeAudio();
  if (e.altKey && e.key === 'Enter') { e.preventDefault(); toggleFull(); return; }
  if (e.key === 'Escape' && !keysBox.hidden) { showKeys(false); e.preventDefault(); return; }
  if (e.key === 'Escape' && !savesBox.hidden) { showSaves(false); e.preventDefault(); return; }
  if (e.key === 'F10' || (e.key === 'Escape' && !setBox.hidden)) { e.preventDefault(); showSettings(setBox.hidden); return; }
  if (!setBox.hidden) return; // the dialog has the keys (the game is paused)
  // H2060 (H1868): the original's Alt hotkeys - Alt / Option + a letter or digit (by the physical key, e.code: macOS's
  // Option makes e.key a special character); the browser's own Alt shortcuts are kept from the page where it lets us
  if (ready && e.altKey && !e.ctrlKey && !e.metaKey) {
    const k = /^(?:Key([A-Z])|Digit([0-9])|Numpad([0-9]))$/.exec(e.code || '');
    if (k) {
      e.preventDefault();
      if (!e.repeat) worker.postMessage({ t: 'alt', key: k[1] || k[2] || k[3], shift: e.shiftKey });
      return;
    }
  }
  if (!ready || e.metaKey || e.ctrlKey || e.altKey) return;
  if (e.target instanceof HTMLSelectElement || e.target instanceof HTMLInputElement) {
    if (e.key.length === 1 || ['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'Home', 'End'].includes(e.key)) return; // the bar's control
  }
  if (e.key === 'F2') { setScale(canvas.classList.contains('smooth') ? 'nearest' : 'linear'); e.preventDefault(); return; }
  if (e.key === 'F11') return; // the browser's (Escape goes to the game: its menu, H169 / H404)
  if ([' ', 'Tab', 'Backspace', 'Enter', 'F1', 'F3', 'F4'].includes(e.key) || e.key.length === 1) e.preventDefault();
  worker.postMessage({ t: 'key', key: e.key });
  // F1 / F3 change settings too: the engine reports them back (kept)
  if (e.key === 'F1' || e.key === 'F3') worker.postMessage({ t: 'get-settings' });
});
