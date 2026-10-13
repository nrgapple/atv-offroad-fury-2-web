// Original-content browser gate for enhanced one-finger ground reverse/air tricks.
// Natural Caribou hop: no injected vehicle state, poses, velocity or forces.
import assert from 'node:assert/strict';
import {captureSimulation} from './capture_simulation.mjs';
import {createRequire} from 'node:module';
import {mkdirSync,writeFileSync} from 'node:fs';
const {chromium}=createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || 'playwright');
const browser=await chromium.launch({headless:true});
const base=process.env.ATV_TEST_URL || 'http://127.0.0.1:4327';
const output=process.env.ATV_ENHANCED_MOBILE_OUTPUT || 'work/verification/enhanced-mobile';
mkdirSync(output,{recursive:true});
const errors=[],results=[];
async function setup(viewport) {
 const page=await browser.newPage({viewport,hasTouch:true,isMobile:true});
 page.on('pageerror',e=>errors.push(e.message));
 await page.addInitScript(()=>{
  localStorage.setItem('atv-mobile-one-handed','true');
  window.rideFrames=[];window.orbitRequests=[];
  const post=Worker.prototype.postMessage;
  Worker.prototype.postMessage=function(data,...rest){if(data.op===50)window.orbitRequests.push(JSON.parse(new TextDecoder().decode(data.bytes)).orbit);return post.call(this,data,...rest);};
 });
 await captureSimulation(page,'rideFrames');
 await page.goto(new URL('/play.html?drive=1',base).href);
 await page.waitForFunction(()=>window.atvTerrainStatus?.simulation&&!document.querySelector('#touch-controls').hidden,null,{timeout:90000});
 const cdp=await page.context().newCDPSession(page);
 const touch=(type,points=[])=>cdp.send('Input.dispatchTouchEvent',{type,touchPoints:points.map(p=>({radiusX:6,radiusY:6,force:1,...p}))});
 const steps=async()=>{const n=await page.evaluate(()=>window.atvTerrainStatus.simulationSteps);await page.waitForFunction(n=>window.atvTerrainStatus.simulationSteps>=n+4,n);};
 return {page,touch,steps};
}
try {
 for(const viewport of (process.env.ATV_ENHANCED_GESTURE ? [] : [{width:390,height:844},{width:844,height:390}])) {
  const {page,touch,steps}=await setup(viewport);
  await page.waitForFunction(()=>window.atvTerrainStatus.frame.grounded.some(Boolean));
  const primary={id:1,x:viewport.width*.7,y:viewport.height*.65},second={id:2,x:viewport.width*.18,y:viewport.height*.65};
  await touch('touchStart',[primary]);await steps();
  await touch('touchMove',[{...primary,x:primary.x+20,y:primary.y+14}]);await steps();
  const before=await page.evaluate(()=>window.atvTerrainStatus.input);
  await touch('touchStart',[{...primary,x:primary.x+20,y:primary.y+14},second]);
  await page.waitForFunction(()=>window.atvTerrainStatus.input?.touch_drive?.reverse_held&&window.atvTerrainStatus.input.reverse,null,{timeout:15000});
  let reverse=await page.evaluate(()=>window.atvTerrainStatus.input);
  assert.equal(reverse.steer,before.steer);assert.equal(reverse.lean,before.lean);assert.equal(reverse.preload_axis,1);
  await page.waitForFunction(()=>window.atvTerrainStatus.frame.speed < -1,null,{timeout:15000});
  await page.screenshot({path:`${output}/reverse-${viewport.width}.png`});
  // Lift only secondary: forward and primary axes resume immediately.
  await touch('touchEnd',[second]);await steps();
  const resumed=await page.evaluate(()=>window.atvTerrainStatus.input);
  assert.equal(resumed.touch_drive.reverse_held,false);assert.equal(resumed.throttle,1);assert.equal(resumed.reverse,false);assert.equal(resumed.steer,before.steer);assert.equal(resumed.lean,before.lean);assert.equal(resumed.preload_axis,1);
  await touch('touchCancel');await steps();assert.equal(await page.evaluate(()=>window.atvTerrainStatus.input.touch_drive.held),false);
  assert(await page.evaluate(()=>window.orbitRequests.every(v=>v===0)));
  results.push({viewport,groundReverse:true,primaryAxesPreserved:true,secondaryRelease:true,cancel:true});await page.close();
 }
 for(const gesture of (process.env.ATV_ENHANCED_GESTURE ? [process.env.ATV_ENHANCED_GESTURE] : ['up','cat-nac','cordova'])) {
  const {page,touch}=await setup({width:390,height:844});
  const primary={id:1,x:310,y:530},second={id:2,x:80,y:530};
  await touch('touchStart',[primary]);
  await page.waitForFunction(()=>window.atvTerrainStatus.frame.preload_armed,null,{timeout:45000,polling:'raf'});
  await touch('touchEnd');
  await page.waitForFunction(()=>window.rideFrames.some(s=>s.frame.preload_active),null,{timeout:5000,polling:'raf'});
  await page.waitForFunction(()=>!window.atvTerrainStatus.frame.grounded.some(Boolean),null,{timeout:5000,polling:'raf'});
  await touch('touchStart',[primary,second]);
  const index=gesture==='up'?0:gesture==='cat-nac'?3:7;
  const target=await page.locator('.touch-trick-choice').nth(index).boundingBox();
  const swipe={...second,x:target.x+target.width/2,y:target.y+target.height/2};
  await touch('touchMove',[primary,swipe]);
  const preview=await page.evaluate(()=>({input:window.atvTerrainStatus.input,selected:document.querySelector('.touch-trick-choice.selected')?.textContent}));
  assert.equal(preview.input.stunt_modifiers,0,'preview must not start a stunt');
  assert.equal(preview.selected,gesture==='up'?'Nac Nac':gesture==='cat-nac'?'Cat Nac':'Cordova');
  await page.screenshot({path:`${output}/${gesture}-wheel.png`});
  await touch('touchEnd',[swipe]);
  const id=gesture==='up'?0:gesture==='cat-nac'?8:9;
  try { await page.waitForFunction(id=>window.rideFrames.some(s=>s.frame.stunt?.id===id&&s.frame.stunt.main_time>.05),id,{timeout:5000,polling:'raf'}); } catch(e) { writeFileSync(`${output}/failed-${gesture}.json`,JSON.stringify(await page.evaluate(()=>window.rideFrames))); throw e; }
  const stunt=await page.evaluate(id=>window.rideFrames.find(s=>s.frame.stunt?.id===id&&s.frame.stunt.main_time>.05),id);
  assert.equal(stunt.frame.rider_mode,'stunt');assert.equal(stunt.input.touch_drive.held,true);assert.equal(stunt.input.throttle,1);assert.equal(stunt.input.reverse,false);assert.equal(stunt.input.preload_axis,1);assert.equal(stunt.input.steer,0);assert.equal(stunt.input.lean,0);
  await page.screenshot({path:`${output}/${gesture}.png`});
  await page.waitForFunction(()=>window.atvTerrainStatus.input.stunt_modifiers===0&&window.atvTerrainStatus.input.touch_drive.held);
  assert(await page.evaluate(()=>window.orbitRequests.every(v=>v===0)));
  results.push({gesture,stunt:stunt.frame.stunt.name,primaryDriveAndBrace:true,previewWithoutStunt:true,releaseStartsStunt:true,independentRelease:true});
  await touch('touchCancel');await page.close();
 }
 assert.deepEqual(errors,[]);writeFileSync(`${output}/report.json`,JSON.stringify({results,errors},null,2));console.log(JSON.stringify({gate:'enhanced-mobile',results,errors}));
} finally {await browser.close();}
