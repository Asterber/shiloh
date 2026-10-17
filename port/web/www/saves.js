// The page's access to the save slots (H1733 / H1734): where they live, and export / import of slot files.
// The store is the one the worker's engine reads (`env.xf_js_save` in worker.js): xf-serve's /saves/ when the server
// keeps the saves (`--saves-in server`, the home deploy), else this browser's IndexedDB `xf-saves` (store `files`,
// key = file name: `slots.txt` + `<NAME>.x`, the window's saves-dir format). Every storage access is in try/catch:
// a failure is reported, never thrown at the page.
'use strict';
const XfSaves = (() => {
  const MAX_SAVE = 8 << 20; // xf-serve's cap (the original's saves are ~160 KB)

  // xf::save::sanitize: A–Z 0–9 space -(), upper case, at most 21, no outer spaces
  const sanitize = (n) => [...n].filter((c) => /[A-Za-z0-9 \-(),]/.test(c)).map((c) => c.toUpperCase()).slice(0, 21).join('').trim();
  const isSlotFile = (f) => f === 'slots.txt' || (f.endsWith('.x') && f.length > 2 && sanitize(f.slice(0, -2)) === f.slice(0, -2));
  // a save = an HDB with the header 5 / 0x501 (xf-serve's check_slot_file)
  function isSave(b) {
    if (!b || b.length < 0x18 || b.length > MAX_SAVE) return false;
    const v = new DataView(b.buffer, b.byteOffset, b.byteLength);
    return v.getUint32(0) === 5 && v.getUint32(0x14) === 0x501;
  }

  const req = (r) => new Promise((ok, fail) => { r.onsuccess = () => ok(r.result); r.onerror = () => fail(r.error); });
  const done = (tx) => new Promise((ok, fail) => { tx.oncomplete = () => ok(); tx.onerror = tx.onabort = () => fail(tx.error); });

  let mode = null; // 'server' | 'idb' | 'memory'
  let idb = null;

  async function openIdb() {
    const open = indexedDB.open('xf-saves', 1);
    open.onupgradeneeded = () => open.result.createObjectStore('files');
    return req(open);
  }

  // where the slots live: the server's choice (/saves-mode, H1733), else what is available
  async function detect() {
    if (mode) return mode;
    let server = null;
    try {
      const r = await fetch('/saves-mode', { cache: 'no-store' });
      if (r.ok) server = (await r.text()).trim(); // 'server' | 'browser'
    } catch (e) { /* an older xf-serve: probe /saves/ */ }
    if (server !== 'browser') {
      try {
        const r = await fetch('/saves/', { cache: 'no-store' });
        if (r.ok) { mode = 'server'; return mode; }
      } catch (e) { /* no server saves */ }
    }
    try { idb = await openIdb(); mode = 'idb'; } catch (e) { console.warn('[xf] no IndexedDB:', e); mode = 'memory'; }
    return mode;
  }

  async function read(name) {
    if (mode === 'server') {
      const r = await fetch('/saves/' + encodeURIComponent(name), { cache: 'no-store' });
      if (r.status === 404) return null;
      if (!r.ok) throw new Error(`${name}: HTTP ${r.status}`);
      return new Uint8Array(await r.arrayBuffer());
    }
    if (mode !== 'idb') return null;
    const v = await req(idb.transaction('files', 'readonly').objectStore('files').get(name));
    return v ? new Uint8Array(v) : null;
  }

  async function write(name, bytes) {
    if (mode === 'server') {
      const r = await fetch('/saves/' + encodeURIComponent(name), { method: 'PUT', body: bytes });
      if (!r.ok) throw new Error(`${name}: HTTP ${r.status} ${(await r.text().catch(() => '')).trim()}`);
      return;
    }
    if (mode !== 'idb') throw new Error('no browser storage here (saves last only until the page closes)');
    const tx = idb.transaction('files', 'readwrite');
    tx.objectStore('files').put(bytes, name);
    await done(tx);
  }

  // the slot files present (names)
  async function files() {
    if (mode === 'server') {
      const r = await fetch('/saves/', { cache: 'no-store' });
      if (!r.ok) throw new Error('/saves/: HTTP ' + r.status);
      return new Set((await r.text()).split('\n').filter(Boolean).map((l) => l.split('\t')[1]));
    }
    if (mode !== 'idb') return new Set();
    return new Set(await req(idb.transaction('files', 'readonly').objectStore('files').getAllKeys()));
  }

  // the three slot names (Slots::open: a name without its file is empty)
  async function slots() {
    const have = await files();
    const t = have.has('slots.txt') ? await read('slots.txt') : null; // (no 404 in the console for a first visit)
    const names = ['', '', ''];
    if (t) new TextDecoder().decode(t).split('\n').slice(0, 3).forEach((l, i) => { names[i] = l.trim(); });
    for (let i = 0; i < 3; i++) if (names[i] && !have.has(names[i] + '.x')) names[i] = '';
    return names;
  }
  const writeSlots = (names) => write('slots.txt', new TextEncoder().encode(names.join('\n') + '\n'));

  // ---- zip (no library): stored entries out; stored or deflated entries in (DecompressionStream, where available)
  const CRC = (() => {
    const t = new Uint32Array(256);
    for (let n = 0; n < 256; n++) { let c = n; for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1; t[n] = c >>> 0; }
    return t;
  })();
  function crc32(b) { let c = 0xffffffff; for (let i = 0; i < b.length; i++) c = CRC[(c ^ b[i]) & 0xff] ^ (c >>> 8); return (c ^ 0xffffffff) >>> 0; }

  function zip(files) { // [{name, bytes}]
    const enc = new TextEncoder(), parts = [], dir = [];
    const d = new Date();
    const time = (d.getHours() << 11) | (d.getMinutes() << 5) | (d.getSeconds() >> 1);
    const date = ((d.getFullYear() - 1980) << 9) | ((d.getMonth() + 1) << 5) | d.getDate();
    let off = 0;
    for (const f of files) {
      const name = enc.encode(f.name), crc = crc32(f.bytes), n = f.bytes.length;
      const h = new DataView(new ArrayBuffer(30));
      [[0, 0x04034b50, 4], [4, 20, 2], [6, 0, 2], [8, 0, 2], [10, time, 2], [12, date, 2], [14, crc, 4], [18, n, 4], [22, n, 4], [26, name.length, 2], [28, 0, 2]]
        .forEach(([o, v, s]) => (s === 4 ? h.setUint32(o, v, true) : h.setUint16(o, v, true)));
      const c = new DataView(new ArrayBuffer(46));
      [[0, 0x02014b50, 4], [4, 20, 2], [6, 20, 2], [8, 0, 2], [10, 0, 2], [12, time, 2], [14, date, 2], [16, crc, 4], [20, n, 4], [24, n, 4],
        [28, name.length, 2], [30, 0, 2], [32, 0, 2], [34, 0, 2], [36, 0, 2], [38, 0, 4], [42, off, 4]]
        .forEach(([o, v, s]) => (s === 4 ? c.setUint32(o, v, true) : c.setUint16(o, v, true)));
      parts.push(h, name, f.bytes);
      dir.push(c, name);
      off += 30 + name.length + n;
    }
    const size = dir.reduce((s, p) => s + p.byteLength, 0);
    const e = new DataView(new ArrayBuffer(22));
    [[0, 0x06054b50, 4], [8, files.length, 2], [10, files.length, 2], [12, size, 4], [16, off, 4]]
      .forEach(([o, v, s]) => (s === 4 ? e.setUint32(o, v, true) : e.setUint16(o, v, true)));
    return new Blob([...parts, ...dir, e], { type: 'application/zip' });
  }

  async function inflate(b) {
    if (typeof DecompressionStream !== 'function') throw new Error('this browser cannot unpack compressed zips (store the files uncompressed)');
    const s = new Blob([b]).stream().pipeThrough(new DecompressionStream('deflate-raw'));
    return new Uint8Array(await new Response(s).arrayBuffer());
  }

  async function unzip(b) { // → [{name (base name), bytes}]
    const v = new DataView(b.buffer, b.byteOffset, b.byteLength);
    let e = -1;
    for (let i = b.length - 22; i >= Math.max(0, b.length - 22 - 0xffff); i--) if (v.getUint32(i, true) === 0x06054b50) { e = i; break; }
    if (e < 0) throw new Error('not a zip file');
    const count = v.getUint16(e + 10, true);
    let p = v.getUint32(e + 16, true);
    const out = [], dec = new TextDecoder();
    for (let k = 0; k < count; k++) {
      if (v.getUint32(p, true) !== 0x02014b50) throw new Error('broken zip directory');
      const method = v.getUint16(p + 10, true), csize = v.getUint32(p + 20, true), size = v.getUint32(p + 24, true);
      const nl = v.getUint16(p + 28, true), xl = v.getUint16(p + 30, true), cl = v.getUint16(p + 32, true), lh = v.getUint32(p + 42, true);
      const full = dec.decode(b.subarray(p + 46, p + 46 + nl));
      p += 46 + nl + xl + cl;
      const name = full.split('/').pop();
      if (!name || full.startsWith('__MACOSX/') || name.startsWith('.') || size > MAX_SAVE) continue;
      const start = lh + 30 + v.getUint16(lh + 26, true) + v.getUint16(lh + 28, true);
      const data = b.subarray(start, start + csize);
      if (method === 0) out.push({ name, bytes: data.slice() });
      else if (method === 8) out.push({ name, bytes: await inflate(data) });
      else throw new Error(`${name}: unsupported zip compression ${method}`);
    }
    return out;
  }

  // ---- export
  async function exportSlot(i) {
    const names = await slots();
    const n = names[i];
    if (!n) throw new Error(`slot ${i + 1} is empty`);
    const b = await read(n + '.x');
    return { name: n + '.x', blob: new Blob([b], { type: 'application/octet-stream' }) };
  }
  // all slots: a zip of slots.txt + the slots' files — the window's saves dir layout (unzip it there to continue)
  async function exportAll() {
    const names = await slots();
    const files = [{ name: 'slots.txt', bytes: new TextEncoder().encode(names.join('\n') + '\n') }];
    for (const n of names) if (n) files.push({ name: n + '.x', bytes: await read(n + '.x') });
    if (files.length === 1) throw new Error('no saves yet');
    const d = new Date(), pad = (x) => String(x).padStart(2, '0');
    return { name: `xf-saves-${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}.zip`, blob: zip(files) };
  }

  // ---- import
  // one save file into slot i, named after the file (sanitized; a name another slot holds leaves that slot, as
  // Slots::store); returns the name
  async function importSlot(i, fileName, bytes) {
    if (!isSave(bytes)) throw new Error(`${fileName}: not an X-Files save (.x)`);
    const name = sanitize(fileName.replace(/\.x$/i, '')) || `SLOT ${i + 1}`;
    const names = await slots();
    await write(name + '.x', bytes);
    for (let j = 0; j < 3; j++) if (j !== i && names[j] === name) names[j] = '';
    names[i] = name;
    await writeSlots(names);
    return name;
  }
  // a zip from exportAll (or the window's saves dir: slots.txt + <NAME>.x, also as several picked files): replaces
  // the three slots; returns the new names
  async function importAll(picked) { // [{name, bytes}]
    let files = [];
    for (const f of picked) files = files.concat(/\.zip$/i.test(f.name) ? await unzip(f.bytes) : [f]);
    const list = files.find((f) => f.name === 'slots.txt');
    if (!list) throw new Error('no slots.txt in the files (export "all slots" makes one)');
    const names = new TextDecoder().decode(list.bytes).split('\n').slice(0, 3).map((l) => l.trim());
    while (names.length < 3) names.push('');
    const saves = new Map(files.filter((f) => f.name.endsWith('.x')).map((f) => [f.name, f.bytes]));
    for (let i = 0; i < 3; i++) {
      const n = names[i];
      if (!n) continue;
      const b = saves.get(n + '.x');
      if (!isSlotFile(n + '.x') || !b) { names[i] = ''; continue; }
      if (!isSave(b)) throw new Error(`${n}.x: not an X-Files save`);
    }
    if (names.every((n) => !n)) throw new Error('none of the slots in slots.txt has its .x file');
    for (const n of names) if (n) await write(n + '.x', saves.get(n + '.x'));
    await writeSlots(names);
    return names;
  }

  return { detect, slots, exportSlot, exportAll, importSlot, importAll, get mode() { return mode; }, _zip: zip, _unzip: unzip, sanitize };
})();
