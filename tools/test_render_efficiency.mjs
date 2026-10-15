// Paired authored renderer checks against an immutable main build. Original
// content stays in private runtime storage; reports contain only hashes/metrics.
import assert from 'node:assert/strict';
import {mkdir,writeFile} from 'node:fs/promises';
import {assertNoErrors,base,launch,newPage,waitForGame} from './browser/harness.mjs';
const before=process.env.ATV_PERF_BASELINE||'http://127.0.0.1:4328',after=base;
const out=process.env.ATV_PERF_OUTPUT||'work/verification/performance';await mkdir(out,{recursive:true});
const browser=await launch();
const report={scope:'One host Mac, Chromium Metal; matched spawn patch/state/recovered follow camera; 4 serial alternating pairs of 80 frames after 20 warmups; render wall time includes gl.finish and excludes physics; phone viewport is not physical mobile hardware or battery measurement',maps:[],hud:[]};
const hooks=`
let auditSpawnCamera;
window.__renderAudit = {
 draw(disable=false,yaw=null) {
   focused=true;menuOpen=false;keys.clear();pointers.clear();
   if(yaw===null){auditSpawnCamera ??= cameraFrame;cameraFrame=auditSpawnCamera;}
   if(yaw!==null){cameraFrame=null;camera.x=simStart.origin[0];camera.y=simStart.origin[1]+30;camera.z=simStart.origin[2]+45;camera.yaw=yaw;camera.pitch=-.3;}
   window.__auditDisableCulling=disable; render(10000);
   const pixels=new Uint8Array(canvas.width*canvas.height*4);gl.readPixels(0,0,canvas.width,canvas.height,gl.RGBA,gl.UNSIGNED_BYTE,pixels);
   return {pixels,resolution:[canvas.width,canvas.height],visibility:diagnostics.visibility,triangles:diagnostics.triangles,terrain:diagnostics.terrainMesh};
 },
 restore(){vehicle.position=simStart.origin.slice();buildTerrainMesh();cameraFrame=auditSpawnCamera;},
 async geometry(x,z){
   vehicle.position=[x*terrain.spacing,vehicle.position[1],z*terrain.spacing];
   window.__auditCounts={uploads:0,bytes:0,deletes:0};const t=performance.now();buildTerrainMesh();const ms=performance.now()-t;
   const hashes=[];
   for(const chunk of chunks){
     gl.bindVertexArray(chunk.vao);gl.bindBuffer(gl.ARRAY_BUFFER,chunk.vertices);
     const vertices=new Uint8Array(gl.getBufferParameter(gl.ARRAY_BUFFER,gl.BUFFER_SIZE));gl.getBufferSubData(gl.ARRAY_BUFFER,0,vertices);
     gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER,chunk.indices);const indices=new Uint8Array(chunk.count*2);gl.getBufferSubData(gl.ELEMENT_ARRAY_BUFFER,0,indices);
     const bytes=new Uint8Array(vertices.length+indices.length);bytes.set(vertices);bytes.set(indices,vertices.length);
     hashes.push([...new Uint8Array(await crypto.subtle.digest('SHA-256',bytes))].map(b=>b.toString(16).padStart(2,'0')).join(''));
   }
   return{hashes,ms,counts:window.__auditCounts,metrics:diagnostics.terrainMesh};
 },
 timing(n=100){focused=true;menuOpen=false;const t=performance.now();for(let i=0;i<n;i++)render(10000);gl.finish();return{ms:(performance.now()-t)/n,counts:window.__auditCounts,visibility:diagnostics.visibility};},
 hudState(){return {frame:simFrame.hud,hud,frontend};}
};`;
async function create(base,env,viewport){
 const errors=[],p=await newPage(browser,{viewport,deviceScaleFactor:viewport.width===390?3:1,hasTouch:true},errors);
 await p.addInitScript(()=>{
  window.__auditFreeze=true;window.__auditCounts={uploads:0,bytes:0,deletes:0,draws:0,triangles:0};
  for(const name of ['bufferData','bufferSubData','deleteBuffer','drawElements','drawElementsInstanced']){const original=WebGL2RenderingContext.prototype[name];WebGL2RenderingContext.prototype[name]=function(...args){const a=window.__auditCounts;if(name==='deleteBuffer')a.deletes++;else if(name.startsWith('buffer')){a.uploads++;a.bytes+=typeof args[1]==='number'&&name==='bufferData'?args[1]:(name==='bufferData'?args[1]:args[2]).byteLength;}else{a.draws=(a.draws??0)+1;a.triangles=(a.triangles??0)+args[1]/3*(name==='drawElementsInstanced'?args[4]:1);}return original.apply(this,args);};}
 });
 await p.route('**/game.js*',async route=>{const r=await route.fetch();let source=await r.text();source=source.replace('frame = requestAnimationFrame(render);','if (!window.__auditFreeze) frame = requestAnimationFrame(render);').replace('if (simReady && !simulationPaused()) {','if (!window.__auditFreeze && simReady && !simulationPaused()) {');await route.fulfill({response:r,body:source+hooks});});
 await p.route('**/render-support.js*',async route=>{const r=await route.fetch();await route.fulfill({response:r,body:(await r.text()).replace('export function outsideView(bounds, planes) {','export function outsideView(bounds, planes) { if (globalThis.__auditDisableCulling) return false;')});});
 await p.goto(`${base}/play.html?drive=1&environment=${env}`);try { await waitForGame(p,{until:()=>window.atvTerrainStatus.ready&&window.__renderAudit}); } catch(e) { console.error(JSON.stringify({base,env,errors,status:await p.locator('#status').textContent(),state:await p.evaluate(()=>({ready:window.atvTerrainStatus?.ready,simulation:window.atvTerrainStatus?.simulation,hooks:Boolean(window.__renderAudit)}))})); throw e; }
 return{p,errors};
}
try{
 for(const env of (process.env.ATV_PERF_MAPS||'env01,env02,env03,env04,env05,env06,env07,env08,free05').split(',')){
  const viewport={width:390,height:844};const a=await create(before,env,viewport),b=await create(after,env,viewport);const rows=[];
  for(const yaw of [null,0,Math.PI/2,Math.PI,Math.PI*1.5]){
   const hashes=[];for(const side of [a,b])hashes.push(await side.p.evaluate(async yaw=>{window.__renderAudit.draw(true,yaw);window.__renderAudit.draw(true,yaw);const {pixels}=window.__renderAudit.draw(true,yaw);return [...new Uint8Array(await crypto.subtle.digest('SHA-256',pixels))].map(v=>v.toString(16).padStart(2,'0')).join('');},yaw));assert.equal(hashes[0],hashes[1],`${env}/${yaw}: baseline renderer pixels changed`);
   const comparison=await b.p.evaluate(({yaw})=>{window.__renderAudit.draw(true,yaw);window.__renderAudit.draw(true,yaw);const reference=window.__renderAudit.draw(true,yaw),culled=window.__renderAudit.draw(false,yaw);let changed=0,max=0;for(let i=0;i<reference.pixels.length;i++){const d=Math.abs(reference.pixels[i]-culled.pixels[i]);if(d){changed++;max=Math.max(max,d);}}return{yaw,changed,max,visibility:culled.visibility};},{yaw});
   assert.equal(comparison.changed,0,`${env}/${yaw}: culling changed pixels`);rows.push(comparison);
  }
  const geometry=[];
  if(['env01','env04','free05'].includes(env))for(const center of [[640,640],[700,690],[760,720],[5,5],[1278,1278],[640,640]]){
   const old=await a.p.evaluate(([x,z])=>window.__renderAudit.geometry(x,z),center),next=await b.p.evaluate(([x,z])=>window.__renderAudit.geometry(x,z),center);
   assert.deepEqual(next.hashes,old.hashes,`${env}/${center}: uploaded terrain samples/topology changed`);geometry.push({center,before:old,after:next});
  }
  // Restore the spawn patch/camera, then warm and alternate serial timings.
  for(const {p}of[a,b])await p.evaluate(()=>{window.__renderAudit.restore();window.__renderAudit.draw(false,null);window.__renderAudit.draw(false,null);window.__auditCounts={uploads:0,bytes:0,deletes:0,draws:0,triangles:0};window.__renderAudit.timing(20);});
  const timings=[];
  for(let i=0;i<4;i++){
   const order=i%2?[b,a]:[a,b],pair={};for(const side of order){await side.p.evaluate(()=>window.__auditCounts={uploads:0,bytes:0,deletes:0,draws:0,triangles:0});pair[side===a?'before':'after']=await side.p.evaluate(()=>window.__renderAudit.timing(80));}timings.push(pair);
  }
  if(env==='env01')await b.p.screenshot({path:`${out}/390-candidate.png`});
  assertNoErrors(a.errors);assertNoErrors(b.errors);const row={env,viewport,pixels:rows,geometry,timings};report.maps.push(row);console.log(JSON.stringify({env,pixels:rows,geometry:geometry.map(r=>({center:r.center,beforeMs:r.before.ms,afterMs:r.after.ms,before:r.before.counts,after:r.after.counts})),timings}));await a.p.close();await b.p.close();await writeFile(`${out}/paired-render.json`,JSON.stringify(report,null,2));
 }
 await writeFile(`${out}/paired-render.json`,JSON.stringify(report,null,2));
 console.log('PASS: matched renderer pixel output, original terrain GPU bytes and alternating fixed-state timings');
}finally{await browser.close();}
