// Original-resource motorcycle jump/bailout gate. Only ordinary riding/stunt
// inputs are supplied; solver pose, contact, velocity and wreck state are observed.
import assert from 'node:assert/strict';
import {writeFile} from 'node:fs/promises';
import {captureSimulation} from './capture_simulation.mjs';
import {captureCrashIsolation,verifyCrashIsolation,verifyCrashImpacts} from './crash_isolation.mjs';
import {assertNoErrors,base,button as roleButton,launch,newPage} from './browser/harness.mjs';
const browser=await launch();
try {
 const errors=[],page=await newPage(browser,{viewport:{width:390,height:844},hasTouch:true},errors);
 await captureCrashIsolation(page);await captureSimulation(page,'bikeFrames');
 await page.addInitScript(mode=>{
  if(mode)localStorage.setItem('atv-crash-mode',mode);
  const post=Worker.prototype.postMessage;let held=false;
  Worker.prototype.postMessage=function(message,...args){
   if(message.op===50&&window.bikeDriving){
    const command=JSON.parse(new TextDecoder().decode(message.bytes)),f=window.atvTerrainStatus?.frame;
    if(f?.speed_magnitude>20&&!f.grounded.some(Boolean)&&f.wreck.phase==='Riding')held=true;
    if(f?.wreck.phase!=='Riding')held=false;
    command.count=4;
    command.input={...command.input,throttle:1,brake:0,steer:0,lean:0,reverse:false,touch_drive:null,preload_axis:null,stunt_modifiers:held?1:0,stunt_x:0,stunt_y:held?1:0};
    message={...message,bytes:new TextEncoder().encode(JSON.stringify(command))};
   }
   return post.call(this,message,...args);
  };
 },process.env.ATV_CRASH_MODE);
 await page.goto(`${base}/play.html`);
 const button=name=>roleButton(page,name);
 await button('Press START').click({timeout:120000});await button('Single Player').click();await button('Quad and Rider').click();
 for(let i=0;i<20;i++)await button('Previous quad').click(); // 27: MX 50 Race
 await page.waitForFunction(()=>[...document.querySelectorAll('.model-view canvas')].every(c=>c.dataset.selection==='27:0'));
 await button('Done').click();await button('Freestyle').click();await button('Ride Caribou Peak').click();
 await page.waitForFunction(()=>document.querySelector('#frontend').hidden&&window.atvTerrainStatus?.playerModel?.quad_id===27,null,{timeout:120000});
 // Portrait touch control accepts movement and cancels without latched drive.
 const cdp=await page.context().newCDPSession(page);
 await cdp.send('Input.dispatchTouchEvent',{type:'touchStart',touchPoints:[{id:1,x:180,y:620,radiusX:6,radiusY:6,force:1}]});
 await page.waitForFunction(()=>window.atvTerrainStatus.input?.touch_drive?.held);
 await cdp.send('Input.dispatchTouchEvent',{type:'touchMove',touchPoints:[{id:1,x:210,y:610,radiusX:6,radiusY:6,force:1}]});
 await page.waitForFunction(()=>Math.abs(window.atvTerrainStatus.input?.steer)>.05);
 await cdp.send('Input.dispatchTouchEvent',{type:'touchCancel',touchPoints:[]});await page.waitForFunction(()=>!window.atvTerrainStatus.input?.touch_drive?.held);
 await button('Pause').click();await button('Reset ATV').click();
 await page.evaluate(()=>window.bikeDriving=true);
 await page.waitForFunction(()=>window.bikeFrames.some(s=>s.frame.wreck.phase!=='Riding'),null,{timeout:60000});
 await page.waitForFunction(()=>window.bikeFrames.some(s=>s.frame.wreck.phase!=='Riding'&&s.frame.rider_world_matrix&&Math.hypot(...s.frame.rider_world_matrix.slice(12,15).map((v,i)=>v-s.frame.origin[i]))>1),null,{timeout:15000});
 const crashed=await page.evaluate(()=>window.atvTerrainStatus.frame);
 if(process.env.ATV_MOTORCYCLE_CRASH_SCREENSHOT)await page.screenshot({path:process.env.ATV_MOTORCYCLE_CRASH_SCREENSHOT});
 await page.evaluate(()=>window.bikeDriving=false);
 await page.waitForFunction(n=>window.atvTerrainStatus.frame.reset_count>n,crashed.reset_count,{timeout:20000});
 assert.equal(await page.evaluate(()=>window.atvTerrainStatus.frame.wreck.phase),'Riding');
 const frames=await page.evaluate(()=>window.bikeFrames),isolation=await verifyCrashIsolation(page),impact=verifyCrashImpacts(frames);
 if(process.env.ATV_CRASH_MODE==='gorn') {
  assert.equal(await page.evaluate(()=>window.atvTerrainStatus.crashMode),'gorn');
  assert(frames.every(s=>!s.frame.funny_crash || s.frame.funny_crash.blood.drops.length/7+s.frame.funny_crash.blood.splats.length/10<=184));
  assert.equal(await page.evaluate(()=>window.atvTerrainStatus.frame.funny_crash??null),null);
 }
 assert(frames.some(s=>!s.frame.grounded.some(Boolean)));assert(frames.some(s=>s.frame.stunt));
 assert(frames.every(s=>s.frame.origin.every(Number.isFinite)));assertNoErrors(errors);assert.equal(await page.evaluate(()=>window.atvTerrainStatus.glError),0);
 await writeFile(process.env.ATV_MOTORCYCLE_CRASH_REPORT||'work/verification/motorcycle-crash.json',JSON.stringify({passed:true,touch:true,isolation,impact,crashed,frames,errors}));
 console.log(JSON.stringify({passed:true,touch:true,frames:frames.length,isolation,impact}));
} finally {await browser.close();}
