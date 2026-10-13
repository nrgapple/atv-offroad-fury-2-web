// Verify delivered home-screen metadata and exact PNG sizes, host or public HTTPS.
import assert from 'node:assert/strict';
const base=process.env.ATV_TEST_URL || 'http://127.0.0.1:4327';
async function get(path){const response=await fetch(new URL(path,base));assert.equal(response.status,200,path);return response;}
for(const path of ['/index.html','/play.html']) {
 const html=await (await get(path)).text();
 assert.match(html,/<meta name="apple-mobile-web-app-title" content="ATV Fury 2">/);
 assert.match(html,/<link rel="apple-touch-icon" sizes="180x180" href="\/apple-touch-icon.png\?v=1">/);
 assert.match(html,/<link rel="manifest" href="\/manifest.webmanifest">/);
}
const manifest=await (await get('/manifest.webmanifest')).json();
assert.equal(manifest.name,'ATV Offroad Fury 2');assert.equal(manifest.short_name,'ATV Fury 2');assert.equal(manifest.start_url,'/play.html');assert.equal(manifest.display,'standalone');
for(const [path,size] of [['/apple-touch-icon.png?v=1',180],['/app-icon-512.png',512]]) {
 const response=await get(path);assert.match(response.headers.get('content-type'),/^image\/png/);
 const png=Buffer.from(await response.arrayBuffer());assert.deepEqual(png.subarray(0,8),Buffer.from([137,80,78,71,13,10,26,10]));assert.equal(png.readUInt32BE(16),size);assert.equal(png.readUInt32BE(20),size);
}
assert.match(await (await get('/app-icon.svg')).text(),/<title>ATV Offroad Fury 2<\/title>/);
console.log('PASS: landing/game Apple touch identity, standalone manifest, authored SVG and 180/512px PNGs served successfully. Existing iOS bookmark caching needs hardware verification.');
