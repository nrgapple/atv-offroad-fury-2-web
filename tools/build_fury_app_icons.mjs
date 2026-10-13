// Authoring helper: render only our path-based SVG; no disc or recovered artwork.
// SHARP_MODULE may point to an existing Sharp installation on the authoring host.
import { createRequire } from 'node:module';
import { readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
const sharp = createRequire(import.meta.url)(process.env.SHARP_MODULE || 'sharp');
const root = new URL('../', import.meta.url);
const source = await readFile(new URL('web/fury-remastered.svg', root));
const inner = source.toString().replace(/^[\s\S]*?<svg\b[^>]*>/, '').replace(/<\/svg>\s*$/, '');
const square = `<svg xmlns="http://www.w3.org/2000/svg" width="512" height="512" viewBox="0 0 512 512">
<defs><linearGradient id="home-red" x2="0" y2="1"><stop stop-color="#a61818"/><stop offset=".48" stop-color="#740a0a"/><stop offset="1" stop-color="#2a0202"/></linearGradient></defs>
<path fill="url(#home-red)" d="M0 0h512v512H0z"/>
<svg x="-10" y="167" width="532" height="177.333" viewBox="0 0 960 320">${inner}</svg></svg>`;
const icons = [];
for (const size of [180,192,512]) {
  const data = await sharp(Buffer.from(square)).resize(size,size).flatten({background:'#2a0202'}).png().toBuffer();
  icons.push({size,file:`fury-remastered-home-${size}-v1.png`,sha256:createHash('sha256').update(data).digest('hex'),base64:data.toString('base64')});
}
await writeFile(new URL('web/fury-app-icons.json',root),JSON.stringify({source_sha256:createHash('sha256').update(source).digest('hex'),icons},null,2)+'\n');
console.log('Rendered authored Fury Remastered Home Screen icons at 180, 192 and 512px.');
