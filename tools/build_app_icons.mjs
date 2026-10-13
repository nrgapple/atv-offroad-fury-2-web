// Private-host-only export of the exact decoded title logo on an authored red
// gradient. Only hashed URL metadata enters Git; original pixels stay in R2.
import { createRequire } from 'node:module';
import { readFile, writeFile, mkdir, realpath } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
const root = fileURLToPath(new URL('..', import.meta.url));
const output = path.join(root, 'private/reports/app-icons');
await mkdir(output, { recursive: true });
const privateRoot = await realpath(path.join(root, 'private'));
if (!(await realpath(output)).startsWith(privateRoot + path.sep)) throw Error('Icons must remain in private storage');
const wasm = (await WebAssembly.instantiate(await readFile(path.join(root, 'web/dist/atv_web.wasm')), {})).instance.exports;
function decode(op, bytes, index = 0) {
  if (!wasm.input_resize(bytes.length)) throw Error('Input too large');
  new Uint8Array(wasm.memory.buffer, wasm.input_ptr(), bytes.length).set(bytes);
  if (!wasm.run(op, index)) throw Error(new TextDecoder().decode(new Uint8Array(wasm.memory.buffer, wasm.output_ptr(), wasm.output_len())));
  return new Uint8Array(wasm.memory.buffer, wasm.output_ptr(), wasm.output_len()).slice();
}
const archive = await readFile(path.join(root, 'private/extracted/UI/RESOURCE/PAGEA/PAGE01.RES'));
const members = JSON.parse(new TextDecoder().decode(decode(2, archive)));
const textures = ['mmlogo01.pst', 'mmlogo02.pst'].map(name => {
  const index = members.findIndex(m => m.name.toLowerCase().endsWith(name));
  if (index < 0) throw Error(`Missing logo member ${name}`);
  const pst = decode(3, archive, index);
  return { ...JSON.parse(new TextDecoder().decode(decode(7, pst))), rgba: Array.from(decode(8, pst)) };
});
const { chromium } = createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || 'playwright');
const browser = await chromium.launch({ headless: true });
try {
  const page = await browser.newPage();
  const pngs = await page.evaluate(textures => {
    const logo = document.createElement('canvas');
    logo.width = textures.reduce((sum, t) => sum + t.width, 0); logo.height = Math.max(...textures.map(t => t.height));
    const lc = logo.getContext('2d'); let x = 0;
    for (const t of textures) { lc.putImageData(new ImageData(new Uint8ClampedArray(t.rgba), t.width, t.height), x, 0); x += t.width; }
    return [180,192,512].map(size => {
      const canvas = document.createElement('canvas'); canvas.width = canvas.height = size;
      const c = canvas.getContext('2d', { alpha: false });
      const gradient = c.createLinearGradient(0, 0, 0, size);
      gradient.addColorStop(0, '#a61818'); gradient.addColorStop(.48, '#740a0a'); gradient.addColorStop(1, '#2a0202');
      c.fillStyle = gradient; c.fillRect(0,0,size,size);
      const width = size * .88, height = width * logo.height / logo.width;
      c.imageSmoothingEnabled = true; c.imageSmoothingQuality = 'high';
      c.drawImage(logo, (size-width)/2, (size-height)/2, width, height);
      return { size, base64: canvas.toDataURL('image/png').split(',')[1] };
    });
  }, textures);
  const icons = pngs.map(({ size, base64 }) => {
    const data = Buffer.from(base64, 'base64'), sha256 = createHash('sha256').update(data).digest('hex');
    return { size, sha256, bytes: data.length, url: `/app-icons/${sha256}.png`, data };
  });
  for (const icon of icons) await writeFile(path.join(output, `${icon.sha256}.png`), icon.data);
  const metadata = { version: 1, icons: icons.map(({ data, ...record }) => record) };
  await writeFile(path.join(root, 'web/app-icon-assets.json'), JSON.stringify(metadata, null, 2) + '\n');
  console.log(JSON.stringify(metadata, null, 2));
} finally { await browser.close(); }
