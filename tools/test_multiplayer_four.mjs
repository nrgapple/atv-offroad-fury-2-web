// Opt-in four-client Rust/WASM multiplayer browser gate; never deploys or uploads.
// From the repository root, with Playwright available to Node:
//   ATV_BROWSER_GPU=metal ATV_TEST_URL=http://127.0.0.1:4186 node tools/test_multiplayer_four.mjs
// Public HTTPS verification must be explicitly selected:
//   ATV_BROWSER_GPU=metal ATV_TEST_URL=https://atv-offroad-fury-2.champtoday.workers.dev node tools/test_multiplayer_four.mjs
// Set PLAYWRIGHT_MODULE to an installed Playwright package path when needed.
// Optional ATV_EXPECTED_COMMIT must equal the full /deployment.json commit SHA.
// URL defaults to the host-only integrated Worker on port 4186. Metal flags are
// opt-in for macOS; measurements cover four automated contexts sharing one host,
// synthetic focus, and possible background throttling, not independent devices.
// Reports/screenshots remain under ignored private/reports. Room creation and
// joining exercise the normal public API; this test never changes hosted assets.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {mkdir,writeFile} from 'node:fs/promises';
import {fileURLToPath} from 'node:url';
import {dirname,join} from 'node:path';
const {chromium}=createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE||'playwright');
const root=dirname(dirname(fileURLToPath(import.meta.url)));
const base=(process.env.ATV_TEST_URL||'http://127.0.0.1:4186').replace(/\/$/,'');
const reachability=new URL(base).origin==='https://atv-offroad-fury-2.champtoday.workers.dev'?'public HTTPS':['localhost','127.0.0.1','[::1]'].includes(new URL(base).hostname)?'host-only':'caller-provided URL';
const gpuFlags=process.env.ATV_BROWSER_GPU==='metal'?['--enable-gpu','--use-angle=metal']:[];
const output=join(root,'private/reports/multiplayer-four-browser.json');
const image=join(root,'private/reports/devlog-images/multiplayer-four-browser.png');
const report={started:new Date().toISOString(),base,reachability,expectedCommit:process.env.ATV_EXPECTED_COMMIT||null,conditions:{browser:'headless Chromium',gpuFlags,viewport:[1280,896],clients:4,documentHasFocusOverride:true,diagnostics:'performance',contention:'Other host workload is not measured; compare concurrent probe logs before attributing timing outliers.',timingScope:'Four automated clients on one Mac sharing CPU/GPU; not four separate player devices.'},models:[{quad:1,rider:0},{quad:14,rider:1},{quad:26,rider:0},{quad:2,rider:1}],errors:[],failedResponses:[],stages:[],movement:[],samples:[],limitations:['Physical phones/controllers, geographically separated machines and Internet latency diversity are not tested.','Focus events are dispatched before gathering each client so synthetic/background Chromium focus does not prevent normal app audio activation. Background-tab throttling remains possible.','Per-peer GL draw matching is disabled before the timing window; game diagnostics are enabled.']};
const save=()=>writeFile(output,JSON.stringify(report,null,2)+'\n');
await mkdir(join(root,'private/reports/devlog-images'),{recursive:true});
const browser=await chromium.launch({headless:true,args:report.conditions.gpuFlags});
const pages=await Promise.all(report.models.map(()=>browser.newPage({viewport:{width:1280,height:896},hasTouch:true})));
const button=(page,name)=>page.getByRole('button',{name,exact:true});
const state=page=>page.evaluate(()=>window.atvTerrainStatus);
for(let index=0;index<pages.length;index++) {
 const page=pages[index];page.on('pageerror',error=>report.errors.push({client:index,error:error.message}));page.on('response',response=>{if(response.status()>=400)report.failedResponses.push({client:index,status:response.status(),url:response.url()});});
 await page.addInitScript(()=>{
  Object.defineProperty(document,'hasFocus',{value:()=>true});localStorage.setItem('atv.diagnostics.v1','performance');
  window.__four={armed:false,draws:{},ambiguous:0};
  const prototype=WebGL2RenderingContext.prototype,names=new WeakMap();
  const location=prototype.getUniformLocation,matrix=prototype.uniformMatrix4fv,draw=prototype.drawElements;let model=null,matched=[];
  prototype.getUniformLocation=function(program,name){const result=location.call(this,program,name);if(result)names.set(result,name);return result;};
  prototype.uniformMatrix4fv=function(uniform,transpose,value,...args){
   const name=names.get(uniform);
   if(name==='model'){model=Array.from(value);matched=[];}
   if(window.__four.armed&&name?.startsWith('quadBones')) {
    const peers=window.atvTerrainStatus?.multiplayer?.remote??[];
    const equal=(a,b)=>a?.length===b?.length&&a.every((v,i)=>Math.fround(v)===b[i]);
    matched=peers.filter(p=>equal(p.world_matrix,model)&&equal(p.quad_node_matrices.flat(),value)||equal(p.rider_world_matrix,model)&&equal(p.rider_node_matrices.flat(),value)).map(p=>p.peer);
   }
   return matrix.call(this,uniform,transpose,value,...args);
  };
  prototype.drawElements=function(...args){if(window.__four.armed){if(matched.length>1)window.__four.ambiguous++;for(const peer of matched)window.__four.draws[peer]=(window.__four.draws[peer]??0)+1;}return draw.apply(this,args);};
 });
}
async function garage(page,{quad,rider}) {
 await button(page,'Quad and Rider').click();
 const next=quad-1,previous=27-quad;
 for(let n=0;n<Math.min(next,previous);n++)await button(page,next<=previous?'Next quad':'Previous quad').click();
 if(rider===1)await button(page,'Change rider').click();
 await button(page,'Done').click();
}
async function focus(page) {await page.evaluate(()=>window.dispatchEvent(new Event('focus')));}
function finite(value) {return typeof value==='number'?Number.isFinite(value):value&&typeof value==='object'?Object.values(value).every(finite):true;}
function summary(s) {return {model:s.playerModel,net:s.multiplayer?.state,remotes:s.multiplayer?.remote.map(p=>({peer:p.peer,model:p.atv_model,rider:p.rider_model,position:p.position,state:p.rider_state,stale:p.stale,extrapolated_ms:p.extrapolated_ms,nodes:[p.quad_node_matrices.length,p.rider_node_matrices.length]})),draws:s.multiplayerDraws,audio:s.audio?.multiplayer,performance:s.performance,physicsRoundTripMs:s.physicsRoundTripMs,physicsBatchMs:s.physicsBatchMs,physicsBatchSteps:s.physicsBatchSteps,focused:s.focused,paused:s.paused,gl:s.glError,multiplayerError:s.multiplayerError,audioError:s.audioError};}
try {
 const host=pages[0];await host.goto(base+'/play.html');
 report.deployment=await host.evaluate(async()=>{const response=await fetch('/deployment.json',{cache:'no-store'});if(!response.ok)throw new Error(`Deployment identity unavailable (${response.status}).`);return response.json();});
 assert.match(report.deployment.commit,/^[a-f0-9]{40}$/,'deployment identity must contain a complete commit SHA');
 if(report.expectedCommit)assert.equal(report.deployment.commit,report.expectedCommit,'public/host deployment does not match ATV_EXPECTED_COMMIT');
 await save();
 await button(host,'Press START').click({timeout:120000});await button(host,'Multiplayer').click();await host.getByRole('textbox',{name:'Rider name',exact:true}).fill('FourHost');await garage(host,report.models[0]);await button(host,'Create lobby').click();
 await host.waitForFunction(()=>window.atvTerrainStatus.multiplayer?.state.code,null,{timeout:20000});report.code=(await state(host)).multiplayer.state.code;report.stages.push('actual UI host room created');await save();console.log('HOST',report.code);
 for(let i=1;i<4;i++) {
  const page=pages[i];await page.goto(`${base}/play.html?room=${report.code}`);await page.getByRole('textbox',{name:'Rider name',exact:true}).fill(`Guest${report.models[i].quad}`,{timeout:120000});await garage(page,report.models[i]);await button(page,'Join code').click();await page.waitForFunction(()=>window.atvTerrainStatus.multiplayer?.state.peer!=null,null,{timeout:20000});console.log('JOIN',i,report.models[i]);
 }
 await host.waitForFunction(()=>window.atvTerrainStatus.multiplayer?.state.lobby?.players.length===4,null,{timeout:20000});report.roster=(await state(host)).multiplayer.state.lobby.players;assert.equal(report.roster.length,4);report.stages.push('four selected models/rigs admitted');await save();
 await host.bringToFront();await button(host,'Ride together').click();
 await Promise.all(pages.map(page=>page.waitForFunction(()=>window.atvTerrainStatus.simulation||window.atvTerrainStatus.multiplayerError,null,{timeout:180000})));
 await Promise.all(pages.map(page=>page.waitForFunction(()=>window.atvTerrainStatus.multiplayer?.remote.length===3&&window.atvTerrainStatus.multiplayerDraws>0||window.atvTerrainStatus.multiplayerError,null,{timeout:120000})));
 for(const page of pages)await focus(page);
 for(const page of pages)await page.evaluate(()=>window.__four.armed=true);
 await Promise.all(pages.map(page=>page.waitForFunction(()=>{const s=window.atvTerrainStatus;return s.multiplayer?.remote.length===3&&s.audio.multiplayer.voices===3&&s.multiplayer.remote.every(p=>(window.__four.draws[p.peer]??0)>0)||s.multiplayerError;},null,{timeout:60000})));
 const connected=await Promise.all(pages.map(state));
 for(let index=0;index<4;index++) {
  const s=connected[index];assert.equal(s.multiplayerError,undefined);assert.equal(s.audioError,undefined);assert.equal(s.glError,0);assert.equal(s.playerModel.quad_id,report.models[index].quad);assert.equal(s.playerModel.rider_id,report.models[index].rider);
  assert.equal(s.multiplayer.remote.length,3);assert.equal(s.audio.multiplayer.voices,3);assert(s.multiplayerDraws>0);assert(finite(s.frame));assert(finite(s.multiplayer.remote));
  const expected=report.models.filter((_,i)=>i!==index).map(m=>m.quad).sort((a,b)=>a-b);assert.deepEqual(s.multiplayer.remote.map(p=>p.atv_model).sort((a,b)=>a-b),expected);
  assert(s.multiplayer.remote.every(p=>p.quad_node_matrices.length===27&&p.rider_node_matrices.length===(p.rider_model?27:18)));
 }
 report.renderEvidence=await Promise.all(pages.map(page=>page.evaluate(()=>structuredClone(window.__four))));report.connected=connected.map(summary);report.stages.push('every client received and drew three remote peers and started three engine voices');console.log('CONNECTED',JSON.stringify(report.connected.map(s=>({model:s.model.quad_id,remotes:s.remotes.map(p=>p.model),draws:s.draws,voices:s.audio.voices}))));await save();
 for(const page of pages)await page.evaluate(()=>window.__four.armed=false);
 for(let driver=0;driver<4;driver++) {
  const page=pages[driver],peer=(await state(page)).multiplayer.state.peer,observers=pages.filter((_,i)=>i!==driver);
  const before=await Promise.all(observers.map(observer=>observer.evaluate(peer=>window.atvTerrainStatus.multiplayer.remote.find(p=>p.peer===peer).position,peer)));
  await page.bringToFront();await page.locator('#game').focus();await focus(page);await page.keyboard.down('KeyW');
  await page.waitForFunction(()=>window.atvTerrainStatus.frame.applied_input.throttle>.5,null,{timeout:20000});
  await Promise.all(observers.map((observer,index)=>observer.waitForFunction(({peer,initial})=>{const p=window.atvTerrainStatus.multiplayer?.remote.find(p=>p.peer===peer)?.position;return p&&Math.hypot(...p.map((v,i)=>v-initial[i]))>3;},{peer,initial:before[index]},{timeout:45000})));
  await page.keyboard.up('KeyW');const after=await Promise.all(observers.map(observer=>observer.evaluate(peer=>window.atvTerrainStatus.multiplayer.remote.find(p=>p.peer===peer).position,peer)));
  const movement={driver,peer,quad:report.models[driver].quad,distances:after.map((p,i)=>Math.hypot(...p.map((v,j)=>v-before[i][j])))};report.movement.push(movement);console.log('MOVE',JSON.stringify(movement));await save();
 }
 for(const page of pages)await focus(page);
 await pages[0].keyboard.press('F3'); // Cycle performance -> debug for a representative readable screenshot.
 await pages[0].keyboard.press('F3');await pages[0].keyboard.press('F3'); // Return to performance.
 await pages[0].screenshot({path:image});report.screenshot=image;
 // At least ten seconds without draw-matching instrumentation so 120-frame
 // rolling diagnostics can refresh; sample all clients without foreground swaps.
 for(let tick=0;tick<12;tick++) {await pages[0].waitForTimeout(1000);const values=await Promise.all(pages.map(state));report.samples.push({at:new Date().toISOString(),clients:values.map(summary)});if(tick%4===3){await save();console.log('TIMING',tick,JSON.stringify(values.map(s=>({fps:s.performance.fps,p95:s.performance.p95Ms,cpu:s.performance.renderCpuMs,worker:s.physicsRoundTripMs,rtt:s.multiplayer.state.rttMs}))))}}
 report.final=report.samples.at(-1).clients;
 assert.deepEqual(report.errors,[]);assert.deepEqual(report.failedResponses,[]);
 await host.close();await Promise.all(pages.slice(1).map(page=>page.waitForFunction(()=>!window.atvTerrainStatus.multiplayer?.state.code,null,{timeout:20000})));
 report.hostEnd=await Promise.all(pages.slice(1).map(state));report.hostEnd=report.hostEnd.map(s=>({remote:s.multiplayer.remote.length,voices:s.audio.multiplayer.voices,impacts:s.audio.multiplayer.impacts,code:s.multiplayer.state.code??null,models:s.multiplayer.models,notice:s.multiplayerError??null}));
 for(const s of report.hostEnd){assert.equal(s.remote,0);assert.equal(s.voices,0);assert.equal(s.impacts,0);assert.equal(s.code,null);assert.deepEqual(s.models,[]);}
 report.stages.push('closing host ends all three guests and disposes peer audio/GPU caches');report.passed=true;report.completed=new Date().toISOString();await save();console.log('PASS',JSON.stringify({movement:report.movement,final:report.final.map(s=>({quad:s.model.quad_id,rtt:s.net.rttMs,fps:s.performance.fps,p95:s.performance.p95Ms,cpu:s.performance.renderCpuMs,physicsRoundTripMs:s.physicsRoundTripMs})),hostEnd:report.hostEnd}));
} catch(error) {report.passed=false;report.failure={message:error.message,stack:error.stack};report.failure.states=await Promise.all(pages.map(async page=>{try{return page.isClosed()?null:summary(await state(page));}catch{return null;}}));await save();console.error('FAIL',error);throw error;} finally {await browser.close();}
