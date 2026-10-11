import { setControlsActive } from './controls.js';
const $ = id => document.getElementById(id);
const worker = new Worker('worker.js', { type: 'module' });
const pending = new Map(); let serial = 0; let generation = 0; let navigation = 0;
let levels = []; let blobUrl; let selectedFile; let busy = false;
const decoder = new TextDecoder();
const BOOT_HASH = '11b80faea827c5550df607d24c8c4f38a9e44cf55e3ecac9e1daeb3aca9d7c49';
const status = (message, error = false) => { $('status').textContent = message; $('status').classList.toggle('error', error); };
worker.onmessage = ({ data }) => {
  if (data.ready) { status('Ready. Open your USA v2.00 disc or an extracted resource.'); return; }
  if (data.startupError) { status(`WebAssembly could not start: ${data.startupError}`, true); return; }
  const request = pending.get(data.id); if (!request) return;
  pending.delete(data.id);
  data.error ? request.reject(new Error(data.error)) : request.resolve(data.bytes);
};
worker.onerror = event => {
  status(`Decoder worker failed: ${event.message}`, true);
  for (const p of pending.values()) p.reject(new Error('Decoder worker failed; reload the page.'));
  pending.clear();
};
function codec(op, bytes, index = 0) {
  return new Promise((resolve, reject) => { const id = ++serial; pending.set(id, { resolve, reject }); worker.postMessage({ id, op, index, bytes }); });
}
async function json(op, bytes) { return JSON.parse(decoder.decode(await codec(op, bytes))); }
function size(bytes) { return bytes >= 1048576 ? `${(bytes / 1048576).toFixed(1)} MB` : `${(bytes / 1024).toFixed(1)} KB`; }
async function read(file, offset, bytes) {
  if (bytes > 128 * 1048576 || offset + bytes > file.size) throw new Error('Invalid or oversized file extent.');
  return new Uint8Array(await file.slice(offset, offset + bytes).arrayBuffer());
}
function clearPreview() {
  const audio = $('preview').querySelector('audio'); if (audio) { audio.pause(); audio.removeAttribute('src'); audio.load(); }
  if (blobUrl) URL.revokeObjectURL(blobUrl); blobUrl = undefined;
  $('preview').replaceChildren(); $('details').textContent = ''; $('fullscreen').disabled = true;
}
function renderList() {
  const level = levels.at(-1); $('crumbs').replaceChildren();
  levels.forEach((item, index) => {
    const button = document.createElement('button'); button.textContent = item.name;
    button.onclick = () => { if (busy) return; navigation++; levels = levels.slice(0, index + 1); $('filter').value = ''; renderList(); };
    $('crumbs').append(button);
  });
  $('files').replaceChildren(); if (!level) return;
  const query = $('filter').value.toLowerCase();
  const visible = level.entries.filter(e => e.name.toLowerCase().includes(query));
  $('count').textContent = `${visible.length} / ${level.entries.length}`;
  for (const entry of visible) {
    const button = document.createElement('button'); button.className = 'file';
    const name = document.createElement('span'); name.textContent = entry.name;
    const bytes = document.createElement('small'); bytes.textContent = size(entry.bytes);
    button.append(name, bytes);
    button.onclick = async () => {
      if (busy) return; busy = true;
      const token = generation; const nav = ++navigation;
      $('files').querySelectorAll('.active').forEach(e => e.classList.remove('active')); button.classList.add('active');
      status(`Reading ${entry.name}…`);
      try {
        const data = await level.load(entry);
        if (generation !== token || navigation !== nav) return;
        await preview(entry.name, data, token);
      } catch (error) { if (generation === token) status(error.message, true); }
      finally { if (generation === token) busy = false; }
    };
    $('files').append(button);
  }
  if (!visible.length) { const p = document.createElement('p'); p.className = 'empty'; p.textContent = 'No matching files.'; $('files').append(p); }
}
async function openIso(file, token) {
  let root;
  for (let sector = 16; sector < 64; sector++) {
    const pvd = await read(file, sector * 2048, 2048);
    if (decoder.decode(pvd.subarray(1, 6)) !== 'CD001') throw new Error('This is not an ISO9660 disc image. Extract the ISO from its ZIP first.');
    if (pvd[0] === 1) { root = await json(0, pvd); break; }
    if (pvd[0] === 255) break;
  }
  if (!root) throw new Error('No primary volume descriptor.');
  const queue = [{ prefix: '', ...root }]; const seen = new Set(); const entries = [];
  while (queue.length) {
    if (generation !== token) return;
    const dir = queue.pop(); const key = `${dir.lba}:${dir.bytes}`;
    if (seen.has(key) || seen.size > 10000) throw new Error('Invalid or cyclic directory tree.');
    seen.add(key);
    if (dir.bytes > 16 * 1048576) throw new Error('Directory is too large.');
    for (const entry of await json(1, await read(file, dir.lba * 2048, dir.bytes))) {
      const name = `${dir.prefix}/${entry.name}`;
      if (entry.directory) queue.push({ ...entry, prefix: name }); else entries.push({ ...entry, name });
    }
  }
  const boot = entries.find(e => e.name === '/SCUS_972.11');
  if (!boot) throw new Error('Expected USA v2.00 boot executable SCUS_972.11 was not found.');
  const hash = [...new Uint8Array(await crypto.subtle.digest('SHA-256', await read(file, boot.lba * 2048, boot.bytes)))].map(n => n.toString(16).padStart(2, '0')).join('');
  if (hash !== BOOT_HASH) throw new Error('Boot executable hash does not match the supported USA v2.00 build.');
  if (generation !== token) return;
  entries.sort((a, b) => a.name.localeCompare(b.name));
  levels = [{ name: 'Disc', entries, load: entry => read(file, entry.lba * 2048, entry.bytes) }]; renderList();
  status(`Boot executable verified · ${entries.length} disc files · all reads stay on this device`);
  $('selected').textContent = file.name;
  const p = document.createElement('p'); p.className = 'empty'; p.textContent = 'Try a .LSI archive for original loading-screen images, AUDIO/AUDIO.RES for sound effects, or STARTAPP.RES for original configuration files.'; $('preview').append(p);
}
async function preview(name, bytes, token) {
  const extension = name.split('.').at(-1).toUpperCase();
  clearPreview(); $('selected').textContent = name;
  if (extension === 'RES' || extension === 'LSI') {
    const entries = (await json(2, bytes)).map((entry, index) => ({ ...entry, index }));
    if (generation !== token) return;
    levels.push({ name: name.split('/').at(-1), entries, load: entry => codec(3, bytes, entry.index) });
    $('filter').value = ''; renderList();
    const p = document.createElement('p'); p.className = 'empty'; p.textContent = `${entries.length} original resource entries. Select an image, sound, or configuration file.`; $('preview').append(p);
    $('details').textContent = `Version 3 resource · ${size(bytes.length)}\nOnly the blocks needed for the selected member are decompressed.`;
  } else if (extension === 'VAG') {
    const info = await json(5, bytes); const wav = await codec(4, bytes);
    if (generation !== token) return;
    blobUrl = URL.createObjectURL(new Blob([wav], { type: 'audio/wav' }));
    const audio = document.createElement('audio'); audio.controls = true; audio.src = blobUrl; audio.preload = 'metadata'; $('preview').append(audio);
    $('details').textContent = `${info.channels} channel(s) · ${info.sample_rate} Hz · ${(info.frames / info.sample_rate).toFixed(2)} s\nLoop samples: ${info.loop_start ?? 'none'} → ${info.loop_end ?? 'none'}\nOne-pass source preview. Original SPU2 mixing, reverb, pitch curves, and looping playback are not reproduced here.`;
  } else if (extension === 'PST') {
    const info = await json(7, bytes); const rgba = await codec(8, bytes);
    if (generation !== token) return;
    const canvas = document.createElement('canvas'); canvas.width = info.width; canvas.height = info.height; canvas.setAttribute('aria-label', name);
    canvas.getContext('2d').putImageData(new ImageData(new Uint8ClampedArray(rgba.buffer), info.width, info.height), 0, 0); $('preview').append(canvas);
    $('fullscreen').disabled = false;
    $('details').textContent = `${info.width} × ${info.height} · original indexed texture\nPS2 palette ordering restored. Fullscreen preserves image proportions.`;
  } else if (bytes[0] === 0x7f && decoder.decode(bytes.subarray(1, 4)) === 'ELF') {
    const info = await json(6, bytes);
    if (generation !== token) return;
    const pre = document.createElement('pre'); pre.textContent = JSON.stringify({ entry: `0x${info.entry.toString(16)}`, segments: info.segments, direct_call_candidates: info.direct_call_candidates.length, caveat: info.caveat }, null, 2); $('preview').append(pre);
  } else {
    const pre = document.createElement('pre');
    const sample = bytes.subarray(0, 1048576);
    const isText = sample.length && sample.filter(c => c >= 32 && c <= 126 || [9, 10, 13].includes(c)).length / sample.length > .96;
    pre.textContent = isText ? decoder.decode(sample) : `Binary format not decoded yet. First 256 bytes:\n\n${Array.from(bytes.subarray(0, 256), b => b.toString(16).padStart(2, '0')).join(' ').match(/.{1,48}/g)?.join('\n') ?? ''}`;
    $('preview').append(pre); $('details').textContent = `${size(bytes.length)}${bytes.length > sample.length ? ' · text preview limited to 1 MiB' : ''}`;
  }
  status(`Opened ${name} · decoded locally`);
}
$('file').onchange = async event => {
  selectedFile = event.target.files[0]; if (!selectedFile) return;
  const token = ++generation; navigation++; busy = true; levels = []; clearPreview(); renderList(); $('filter').value = ''; $('count').textContent = '0 files';
  status(`Opening ${selectedFile.name}…`);
  try {
    if (selectedFile.name.toLowerCase().endsWith('.iso')) await openIso(selectedFile, token);
    else await preview(selectedFile.name, await read(selectedFile, 0, selectedFile.size), token);
  } catch (error) { if (generation === token) status(error.message, true); }
  finally { if (generation === token) busy = false; }
};
$('filter').oninput = renderList;
$('fullscreen').onclick = () => $('preview').requestFullscreen().catch(error => status(error.message, true));
window.addEventListener('pagehide', () => { clearPreview(); worker.terminate(); });
function mode(controls) {
  $('content-panel').hidden = controls; $('controls-panel').hidden = !controls;
  $('content-tab').setAttribute('aria-pressed', String(!controls)); $('controls-tab').setAttribute('aria-pressed', String(controls));
  if (controls) $('preview').querySelector('audio')?.pause();
  setControlsActive(controls);
}
$('content-tab').onclick = () => mode(false);
$('controls-tab').onclick = () => mode(true);
