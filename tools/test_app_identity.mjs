// Verify delivered home-screen metadata and exact PNG sizes, host or public HTTPS.
import assert from 'node:assert/strict';
const base=process.env.ATV_TEST_URL || 'http://127.0.0.1:4327';
async function get(path){const response=await fetch(new URL(path,base),{headers:{'User-Agent':'Fury-app-identity-verifier/1.0'}});assert.equal(response.status,200,path);return response;}
const appleIcons=new Set();
for(const path of ['/index.html','/play.html']) {
 const html=await (await get(path)).text();
 assert.match(html,/<meta name="apple-mobile-web-app-title" content="Fury Remastered">/);
 const icon=html.match(/<link rel="apple-touch-icon" sizes="180x180" href="(\/app-icons\/[a-f0-9]{64}\.png)">/);assert.ok(icon);appleIcons.add(icon[1]);
 assert.match(html,/<link rel="manifest" href="\/manifest.webmanifest">/);
}
const manifest=await (await get('/manifest.webmanifest')).json();
assert.equal(manifest.name,'Fury Remastered');assert.equal(manifest.short_name,'Fury Remastered');assert.equal(manifest.start_url,'/play.html');assert.equal(manifest.display,'standalone');
assert.equal(appleIcons.size,1);
const pngs=[[[...appleIcons][0],180],...manifest.icons.map(icon=>[icon.src,Number(icon.sizes.split('x')[0])]),['/apple-touch-icon.png',192],['/apple-touch-icon-precomposed.png',192],['/app-icon-512.png',512]];
for(const [path,size] of pngs) {
 const response=await get(path);assert.match(response.headers.get('content-type'),/^image\/png/);
 const png=Buffer.from(await response.arrayBuffer());assert.deepEqual(png.subarray(0,8),Buffer.from([137,80,78,71,13,10,26,10]));assert.equal(png.readUInt32BE(16),size);assert.equal(png.readUInt32BE(20),size);
}
assert.match(await (await get('/fury-remastered.svg')).text(),/<title(?:\s[^>]*)?>Fury Remastered<\/title>/);
console.log('PASS: Fury Remastered page identity/vector branding, standalone manifest, original-logo 180/192/512px PNGs and Safari/legacy aliases served successfully. Existing iOS bookmark caching needs hardware verification.');
