// Execute the browser's menu ABI against the actual packaged WASM bytes.
// Native Rust tests and file hashes cannot detect a stale compiled module.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
const path=process.argv[2]||'web/dist/atv_web.wasm';
const wasm=(await WebAssembly.instantiate(await readFile(path),{})).instance.exports;
function menu(page,width,height,speed_rush='normal') {
 const bytes=new TextEncoder().encode(JSON.stringify({page,width,height,selection:0,speed_rush,original_camera:false,crash_mode:'crazy'}));
 assert(wasm.input_resize(bytes.length));
 new Uint8Array(wasm.memory.buffer,wasm.input_ptr(),bytes.length).set(bytes);
 const ok=wasm.run(62,0),text=new TextDecoder().decode(new Uint8Array(wasm.memory.buffer,wasm.output_ptr(),wasm.output_len()));
 assert(ok,`Built WASM rejected current menu request: ${text}`);
 return JSON.parse(text);
}
for(const [width,height]of [[390,844],[1280,720]]) {
 for(const page of ['title','main','pause','controls','music','extras','alpha'])assert(menu(page,width,height).choices.length>0);
 assert.deepEqual(menu('settings',width,height).choices,['controls','hud-settings','music','extras','alpha','back']);
 for(const speed of ['off','low','normal','high']) {
  const frame=menu('hud-settings',width,height,speed);
  assert(frame.choices.includes('speed-rush'));
  assert(JSON.stringify(frame).includes(`Speed rush: ${speed[0].toUpperCase()+speed.slice(1)}`));
 }
}
console.log('PASS: actual built WASM accepts current frontend menu ABI, four settings sections and every speed option on desktop/portrait');
