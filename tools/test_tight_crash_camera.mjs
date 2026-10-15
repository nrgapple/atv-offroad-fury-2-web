// Tight crash-camera integration gate using real riding inputs. Supplies authored throttle
// and held-stunt inputs through the existing WASM input interface; never sets
// velocity, pose, wreck state, damage or effects state. Results remain private.
import assert from 'node:assert/strict';
import {mkdir,writeFile} from 'node:fs/promises';
import {assertNoErrors,base,button,launch,newPage,waitForScreen} from './browser/harness.mjs';
import {captureSimulation} from './capture_simulation.mjs';
import {captureCrashIsolation,verifyCrashIsolation,captureCrashDraws,verifyCrashImpacts} from './crash_isolation.mjs';
const browser=await launch();
await mkdir('work/verification',{recursive:true});
try {
 const errors=[],page=await newPage(browser,{viewport:{width:Number(process.env.ATV_CRASH_WIDTH||1280),height:844}},errors);
 await captureCrashIsolation(page);await captureCrashDraws(page);await captureSimulation(page,'crashFrames');
 await page.addInitScript(()=>{
  localStorage.setItem('atv-camera-shake','true'); // Obsolete preference must have no effect.
  window.cameraSamples=[];
  const sample=()=>{const s=window.atvTerrainStatus,c=s?.cameraPresentation?.crash;
    if(c)window.cameraSamples.push({c,camera:s.camera});requestAnimationFrame(sample);};requestAnimationFrame(sample);
  // Fresh browser uses the default-On crash setting.
  const post=Worker.prototype.postMessage;let held=false;
  Worker.prototype.postMessage=function(message,...args){
   if(message.op===50&&window.crashDriving){
    const source=JSON.parse(new TextDecoder().decode(message.bytes)),f=window.atvTerrainStatus?.frame;
    if(f?.speed_magnitude>55&&!f.grounded.some(Boolean)&&f.wreck.phase==='Riding')held=true;
    if(f?.wreck.phase!=='Riding')held=false;
    source.count=4;
    source.input={...source.input,throttle:1,brake:0,steer:0,lean:0,reverse:false,touch_drive:null,preload_axis:null,stunt_modifiers:held?1:0,stunt_x:0,stunt_y:held?1:0};
    message={...message,bytes:new TextEncoder().encode(JSON.stringify(source))};
   }
   return post.call(this,message,...args);
  };
 });
 await page.goto(`${base}/play.html?drive=1&environment=env01`);
 await page.waitForFunction(()=>window.atvTerrainStatus?.simulation,null,{timeout:120000});
 assert.equal(await page.evaluate(()=>window.atvTerrainStatus.funnyCrashes),true);
 await page.keyboard.press('Escape');
 await button(page,'Reset ATV').waitFor();
 await page.evaluate(()=>window.crashDriving=true);
 await button(page,'Reset ATV').click();
 await page.waitForFunction(()=>window.atvTerrainStatus.frame.funny_crash?.detached_parts>=1&&window.atvTerrainStatus.frame.funny_crash.rider_node_matrices.length>0&&window.atvTerrainStatus.frame.funny_crash.age>.6,null,{timeout:60000});
 await page.evaluate(()=>window.crashDriving=false);
 const crash=await page.evaluate(()=>window.atvTerrainStatus.frame);
 // Pieces nearest the actual contact break; a severe hit sheds at least one.
 assert(crash.funny_crash.detached_parts>=1&&crash.funny_crash.detached_parts<=7);
 await page.waitForFunction(()=>crashDraws.quad>0&&crashDraws.rider>0);
 if(process.env.ATV_CRASH_SCREENSHOT)await page.screenshot({path:process.env.ATV_CRASH_SCREENSHOT});
 await page.waitForFunction(n=>window.atvTerrainStatus.frame.reset_count>n,crash.reset_count,{timeout:20000});
 assert.equal(await page.evaluate(()=>window.atvTerrainStatus.frame.funny_crash??null),null);
 const cameraSamples=await page.evaluate(()=>window.cameraSamples);
 assert(cameraSamples.length>10,'sample actual rendered crash framing');
 assert(cameraSamples.every(({c})=>c.distance<=32 && c.target.every(Number.isFinite)));
 const separated=cameraSamples.filter(({c})=>c.separation>=22);
 assert(separated.length>0,'route must exercise rider-priority separation');
 assert(separated.every(({c})=>c.bikeWeight===0 && Math.hypot(...c.target.map((v,i)=>v-c.rider[i]))<1e-8));
 assert.equal(await page.evaluate(()=>window.atvTerrainStatus.cameraShake),undefined);
 await page.keyboard.press('Escape');await waitForScreen(page,'pause');
 assert.equal(await page.getByRole('button',{name:/Camera shake/}).count(),0);
 const isolation=await verifyCrashIsolation(page);
 const frames=await page.evaluate(()=>window.crashFrames);
 const impact=verifyCrashImpacts(frames);
 assert(impact.aerial>0,'Bailout must show rider ragdoll with intact airborne ATV before its impact');
 assert(impact.impacts>0);
 assert(frames.every(s=>!s.frame.funny_crash||[...s.frame.funny_crash.quad_node_matrices.flat(),...s.frame.funny_crash.rider_node_matrices.flat()].every(Number.isFinite)));
 assertNoErrors(errors);assert.equal(await page.evaluate(()=>window.atvTerrainStatus.glError),0);
 await writeFile(process.env.ATV_CRASH_TRACE||'work/verification/tight-crash-camera.json',JSON.stringify({crash,isolation,impact,cameraSamples,frames,errors}));
 console.log('PASS: tight crash shot, 32ft dolly cap, rider priority at separation, obsolete shake ignored, rendered breakup/recovery and unchanged native gameplay.');
} finally {await browser.close();}
