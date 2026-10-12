// Original-content browser integration gate: cycle all 26 choices through the
// actual garage UI, alternate both rider rigs, then enter Freestyle and drive.
// Requires hosted original runtime content plus private/reports/engine-audio-configs/
// baseline JSON files. No disc bytes, extracted assets or credentials are embedded.
// Desktop Chromium uses Metal GPU; reports and screenshots stay under private/reports/.
// Run from any directory (the default service URL is host-only):
// PLAYWRIGHT_MODULE=/path/to/playwright ATV_TEST_URL=http://127.0.0.1:4186 \
//   node tools/test_player_models.mjs
// Optional: ATV_MODEL_FROM/TO, ATV_MODEL_REPORT (inside private/reports/ only),
// ATV_MODEL_SCREEN_PREFIX. Public URLs require an approved deployed checkpoint.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {createRequire} from 'node:module';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
const {chromium}=createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || 'playwright');
const base=process.env.ATV_TEST_URL || 'http://127.0.0.1:4186';
const root=fileURLToPath(new URL('..',import.meta.url));
const privateReports=path.join(root,'private/reports');
const output=path.resolve(root,process.env.ATV_MODEL_REPORT || 'private/reports/player-models-browser.json');
assert.ok(output.startsWith(privateReports+path.sep),'ATV_MODEL_REPORT must stay inside private/reports/');
fs.mkdirSync(path.dirname(output),{recursive:true});
fs.mkdirSync(path.join(privateReports,'devlog-images'),{recursive:true});
const imagePrefix=process.env.ATV_MODEL_SCREEN_PREFIX || 'singleplayer-garage';
assert.match(imagePrefix,/^[a-zA-Z0-9_-]+$/,'Screenshot prefix must be a safe filename stem');
const from=Number(process.env.ATV_MODEL_FROM || 1), to=Number(process.env.ATV_MODEL_TO || 26);
assert.ok(Number.isInteger(from)&&Number.isInteger(to)&&from>=1&&from<=to&&to<=26,'Model range must be 1..26');
const targetUrl=new URL(base);
const reachability=targetUrl.origin==='https://atv-offroad-fury-2.champtoday.workers.dev'?'public HTTPS':(targetUrl.hostname==='localhost'||targetUrl.hostname==='[::1]'||/^127\./.test(targetUrl.hostname))?'host-only':'caller-provided URL';
const report={started:new Date().toISOString(),base,reachability,models:[],errors:[],requests:[],limitations:['Desktop automated Chromium only; physical controllers and phones unverified.','Rendering observes actual WebGL pose draws; does not establish original PS2 fidelity.']};
const save=()=>fs.writeFileSync(output,JSON.stringify(report,null,2)+'\n');
const browser=await chromium.launch({headless:true,args:['--enable-gpu','--use-angle=metal','--autoplay-policy=no-user-gesture-required']});
const page=await browser.newPage({viewport:{width:1280,height:896}});
page.on('pageerror',e=>report.errors.push({type:'pageerror',message:e.message}));
page.on('console',m=>{if(m.type()==='error')report.errors.push({type:'console',message:m.text()});});
page.on('response',r=>{if(r.status()>=400)report.requests.push({url:r.url(),status:r.status()});});
await page.addInitScript(()=>{
 window.__modelProbe={engine:[],workerErrors:[],draws:{quad:0,rider:0},armed:false};
 const NativeWorker=window.Worker;
 window.Worker=class extends NativeWorker {
   constructor(...args){super(...args);this.probeRequests=new Map();this.addEventListener('message',event=>{const d=event.data,meta=this.probeRequests.get(d.id);if(d.error||d.startupError)window.__modelProbe.workerErrors.push(d.error||d.startupError);if(meta){this.probeRequests.delete(d.id);if(d.bytes)window.__modelProbe.engine.push({index:meta.index,config:JSON.parse(new TextDecoder().decode(d.bytes))});}});}
   postMessage(data,...args){if(data?.op===32)this.probeRequests.set(data.id,{index:data.index});return super.postMessage(data,...args);}
 };
 const proto=WebGL2RenderingContext.prototype,names=new WeakMap();
 const get=proto.getUniformLocation,matrix=proto.uniformMatrix4fv,draw=proto.drawElements,int=proto.uniform1i;
 proto.getUniformLocation=function(program,name){const location=get.call(this,program,name);if(location)names.set(location,name);return location;};
 let kind=null,enabled=false;
 proto.uniform1i=function(location,value){if(names.get(location)==='useQuadPose')enabled=Boolean(value);return int.call(this,location,value);};
 proto.uniformMatrix4fv=function(location,transpose,value,...args){
   if(window.__modelProbe.armed&&names.get(location)?.startsWith('quadBones')){
     const frame=window.atvTerrainStatus?.frame;
     const matches=matrices=>matrices?.length*16===value.length&&matrices.every((m,i)=>m.every((v,j)=>Math.fround(v)===value[i*16+j]));
     kind=matches(frame?.quad_node_matrices)?'quad':matches(frame?.rider_node_matrices)?'rider':null;
   }
   return matrix.call(this,location,transpose,value,...args);
 };
 proto.drawElements=function(...args){if(window.__modelProbe.armed&&enabled&&kind)window.__modelProbe.draws[kind]++;return draw.apply(this,args);};
});
const button=(name)=>page.getByRole('button',{name,exact:true});
const read=()=>page.evaluate(()=>{
 const s=window.atvTerrainStatus;
 function finite(v){return typeof v==='number'?Number.isFinite(v):v===null||typeof v!=='object'?true:Object.values(v).every(finite);}
 return {model:s.playerModel,simulation:s.simulation,quadNodes:s.animatedQuadNodes,riderNodes:s.animatedRiderNodes,quadTriangles:s.vehicleTriangles,riderTriangles:s.riderTriangles,quadTextures:s.vehicleTextures,riderTextures:s.riderTextures,gl:s.glError,frame:s.frame,finite:finite(s.frame)&&finite(s.camera),audio:s.audio,probe:structuredClone(window.__modelProbe),notice:document.querySelector('#menu-notice').textContent,status:document.querySelector('#status').textContent};
});
try{
 await page.goto(base+'/play.html');
 await button('Press START').click({timeout:120000});
 await button('Single Player').click();
 await button('Quad and Rider').click();
 // Initial selection is ID1/male. Use real garage actions to reach requested range.
 for(let id=1;id<from;id++){await button('Next quad').click();await button('Change rider').click();}
 for(let id=from;id<=to;id++){
   const started=Date.now(),rider=(id-1)%2;
   if(id>from){await button('Single Player').click();await button('Quad and Rider').click();await button('Next quad').click();await button('Change rider').click();}
   const garageText=await page.locator('#frontend').innerText();
   await button('Done').click();await button('Freestyle').click();await button('Ride Caribou Peak').click();
   await page.waitForFunction(({id,rider})=>document.querySelector('#frontend').hidden&&window.atvTerrainStatus?.simulation&&window.atvTerrainStatus?.playerModel?.quad_id===id&&window.atvTerrainStatus?.playerModel?.rider_id===rider,{id,rider},{timeout:120000});
   await page.waitForTimeout(350);
   const before=await read();
   assert.equal(before.model.quad,`/QUAD/BIKES${id<=13?'A':'B'}/ATV${String(id).padStart(2,'0')}.RES`);
   assert.equal(before.model.quad_materials,before.model.quad.replace('.RES','_MT.RES'));
   assert.equal(before.model.physics.toLowerCase(),`atv${String(id).padStart(2,'0')}s01.phy`);
   assert.equal(before.model.rider,rider?'/RIDER/FRIDER.RES':'/RIDER/MRIDER.RES');
   assert.equal(before.quadNodes,27);assert.equal(before.riderNodes,rider?27:18);
   assert.ok(before.quadTriangles>4000&&before.riderTriangles>0);assert.ok(before.quadTextures>0&&before.riderTextures>0);
   assert.ok(before.finite&&before.simulation);assert.equal(before.gl,0);
   const engine=before.probe.engine.at(-1);assert.equal(engine.index,id);
   const expected=JSON.parse(fs.readFileSync(path.join(privateReports,`engine-audio-configs/${String(id).padStart(2,'0')}.json`)));
   assert.equal(engine.config.engine_set,expected.engine_set);assert.equal(engine.config.stroke,expected.stroke);assert.deepEqual(engine.config.samples,expected.samples);
   await page.evaluate(()=>{window.__modelProbe.draws={quad:0,rider:0};window.__modelProbe.armed=true;});
   await page.keyboard.down('w');await page.waitForTimeout(1400);
   const driven=await read();await page.keyboard.up('w');
   await page.evaluate(()=>window.__modelProbe.armed=false);
   const horizontal=Math.hypot(driven.frame.origin[0]-before.frame.origin[0],driven.frame.origin[2]-before.frame.origin[2]);
   assert.ok(driven.finite&&driven.simulation);assert.equal(driven.gl,0);
   assert.ok(horizontal>0.2,`ID${id} throttle must move (${horizontal})`);
   assert.ok(driven.probe.draws.quad>0&&driven.probe.draws.rider>0,`ID${id} actual articulated draws`);
   assert.ok(driven.audio.ready&&driven.audio.context==='running'&&driven.audio.commands>0&&driven.audio.voices>0,`ID${id} engine voice`);
   assert.equal(driven.probe.workerErrors.length,0);assert.equal(report.errors.length,0);assert.equal(report.requests.length,0);
   const result={id,rider,label:driven.model.label,garageText,model:driven.model,quadNodes:driven.quadNodes,riderNodes:driven.riderNodes,quadTriangles:driven.quadTriangles,riderTriangles:driven.riderTriangles,quadDraws:driven.probe.draws.quad,riderDraws:driven.probe.draws.rider,horizontalFeet:horizontal,rpm:driven.frame.rpm,speed:driven.frame.speed,audio:{engineSet:engine.config.engine_set,stroke:engine.config.stroke,sampleCount:driven.audio.samples,voices:driven.audio.voices,commands:driven.audio.commands,context:driven.audio.context},elapsedMs:Date.now()-started};
   if([1,14,26].includes(id)){const screenshot=path.join(privateReports,`devlog-images/${imagePrefix}-model-${String(id).padStart(2,'0')}-browser.png`);await page.screenshot({path:screenshot});result.screenshot=screenshot;}
   report.models.push(result);save();console.log(JSON.stringify({passed:id,rider,feet:horizontal,elapsedMs:result.elapsedMs,draws:[result.quadDraws,result.riderDraws],engine:result.audio.engineSet}));
   if(id<to){await page.keyboard.press('Escape');await button('Quit to Main Menu').click();}
 }
 report.passed=true;report.completed=new Date().toISOString();save();
}catch(error){report.passed=false;report.failure={message:error.message,stack:error.stack,model:report.models.at(-1)?.id,next:report.models.length+from};try{report.failure.state=await read();await page.screenshot({path:path.join(privateReports,`devlog-images/${imagePrefix}-model-gate-failure.png`)});}catch{}save();throw error;
}finally{await browser.close();}
