// Authoring helper: render only our path-based SVG; no disc or recovered artwork.
// SHARP_MODULE may point to an existing Sharp installation on the authoring host.
// Existing entries keep their bytes (installed Home Screen icons cache by URL);
// pass --force to re-render everything after changing the artwork.
import { createRequire } from 'node:module';
import { readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
const sharp = createRequire(import.meta.url)(process.env.SHARP_MODULE || 'sharp');
const root = new URL('../', import.meta.url);
const source = await readFile(new URL('web/fury-remastered.svg', root));
const sourceSha = createHash('sha256').update(source).digest('hex');
const metadataUrl = new URL('web/fury-app-icons.json', root);
let previous = null;
try { previous = JSON.parse(await readFile(metadataUrl, 'utf8')); } catch { /* first render */ }
const reuse = !process.argv.includes('--force') && previous?.source_sha256 === sourceSha;
const inner = source.toString().replace(/^[\s\S]*?<svg\b[^>]*>/, '').replace(/<\/svg>\s*$/, '');
const background = `<defs><linearGradient id="home-red" x2="0" y2="1"><stop stop-color="#a61818"/><stop offset=".48" stop-color="#740a0a"/><stop offset="1" stop-color="#2a0202"/></linearGradient></defs>
<path fill="url(#home-red)" d="M0 0h512v512H0z"/>`;
// Home Screen ("any"): the wordmark nearly fills the square; iOS shows it uncropped.
const square = `<svg xmlns="http://www.w3.org/2000/svg" width="512" height="512" viewBox="0 0 512 512">${background}
<svg x="-10" y="167" width="532" height="177.333" viewBox="0 0 960 320">${inner}</svg></svg>`;
// Maskable: Android crops to any shape inside the central 80% circle (radius 204.8).
// A 372x124 wordmark's corners sit 196px from the centre, inside that safe zone.
const maskable = `<svg xmlns="http://www.w3.org/2000/svg" width="512" height="512" viewBox="0 0 512 512">${background}
<svg x="70" y="194" width="372" height="124" viewBox="0 0 960 320">${inner}</svg></svg>`;
const png = (svg, size) => sharp(Buffer.from(svg)).resize(size, size).flatten({ background: '#2a0202' }).png().toBuffer();
const entry = (data, fields) => ({ ...fields, sha256: createHash('sha256').update(data).digest('hex'), base64: data.toString('base64') });
const kept = (list, file) => reuse ? list?.find(item => item.file === file) : null;
const icons = [];
for (const size of [180, 192, 512]) {
  const file = `fury-remastered-home-${size}-v1.png`;
  icons.push(kept(previous?.icons, file) ?? entry(await png(square, size), { size, file }));
}
const maskableIcons = [];
for (const size of [192, 512]) {
  const file = `fury-remastered-maskable-${size}-v1.png`;
  maskableIcons.push(kept(previous?.maskable, file) ?? entry(await png(maskable, size), { size, file }));
}
// favicon.ico: an ICO directory of PNG images (supported by every current browser).
let favicon = reuse && previous?.favicon?.file === 'favicon.ico' ? previous.favicon : null;
if (!favicon) {
  const sizes = [16, 32, 48], images = await Promise.all(sizes.map(size => png(square, size)));
  const header = Buffer.alloc(6 + 16 * sizes.length);
  header.writeUInt16LE(0, 0); header.writeUInt16LE(1, 2); header.writeUInt16LE(sizes.length, 4);
  let offset = header.length;
  sizes.forEach((size, i) => {
    const at = 6 + 16 * i;
    header.writeUInt8(size, at); header.writeUInt8(size, at + 1);
    header.writeUInt16LE(1, at + 4); header.writeUInt16LE(32, at + 6);
    header.writeUInt32LE(images[i].length, at + 8); header.writeUInt32LE(offset, at + 12);
    offset += images[i].length;
  });
  favicon = entry(Buffer.concat([header, ...images]), { file: 'favicon.ico', sizes });
}
await writeFile(metadataUrl, JSON.stringify({ source_sha256: sourceSha, icons, maskable: maskableIcons, favicon }, null, 2) + '\n');
console.log('Rendered authored Fury Remastered icons: Home Screen 180/192/512, maskable 192/512 and favicon.ico 16/32/48.');
