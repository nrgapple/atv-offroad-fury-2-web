// Ordinary source-course driving; no injected vehicle states or forces.
import assert from 'node:assert/strict';
import { assertNoErrors, base, holdKey, launch, newPage, waitForGame, waitForSteps } from './browser/harness.mjs';
const browser = await launch();
try {
 for (const viewport of [{width:390,height:844},{width:1280,height:720}]) {
  const errors=[], page = await newPage(browser,{viewport},errors);
  await page.goto(`${base}/play.html?drive=1&environment=env01`);
  await waitForGame(page,{until:()=>window.atvTerrainStatus.simulationSteps>60});
  await page.evaluate(()=>{
   window.hillTrace=[];
   let last=performance.now();
   function record(now) {
    const d=window.atvTerrainStatus, p=d?.cameraPresentation;
    if(p && d.frame) window.hillTrace.push({time:now,dt:Math.min(.05,(now-last)/1000),step:d.simulationSteps,orbit:p.requestedHillOrbit,pullback:p.uphillDistanceScale,preset:d.followCamera.preset,reset:d.frame.reset_count,phase:d.frame.wreck.phase,position:d.camera.slice(0,3),pitch:d.camera[4],orientation:d.frame.orientation});
    last=now; if(window.hillTrace.length<4000)requestAnimationFrame(record);
   }
   requestAnimationFrame(record);
  });
  await page.keyboard.down('KeyW');
  await page.waitForFunction(()=>window.atvTerrainStatus.cameraPresentation.requestedHillOrbit>.20,null,{timeout:45000});
  const hill=await page.evaluate(()=>window.atvTerrainStatus);
  const native=Math.atan2(hill.followCamera.target[1]-hill.followCamera.position[1],Math.hypot(hill.followCamera.target[0]-hill.followCamera.position[0],hill.followCamera.target[2]-hill.followCamera.position[2]));
  assert(Math.abs(hill.camera[4]-native-hill.cameraPresentation.pitchLift-hill.cameraPresentation.uphillLift)<1e-6);
  assert(hill.cameraPresentation.uphillDistanceScale>1.09,'climb pulls back');
  if(process.env.ATV_HILL_SCREENSHOT)await page.screenshot({path:process.env.ATV_HILL_SCREENSHOT.replace('.png',`-${viewport.width}.png`)});
  await holdKey(page,'KeyE',150);
  await waitForSteps(page,90);
  const trace=await page.evaluate(()=>window.hillTrace);
  let checked=0,maxStep=0;
  for(let i=1;i<trace.length;i++) {
   const a=trace[i-1],b=trace[i];
   assert([...b.position,b.pitch,b.orbit,b.pullback].every(Number.isFinite));
   if(a.preset===0 && b.preset===0 && a.phase==='Riding' && b.phase==='Riding' && a.reset===b.reset) {
    const jump=Math.abs(b.orbit-a.orbit); maxStep=Math.max(maxStep,jump);
    // Any terrain grade change is bounded by the exponential response. Allow
    // one RAF scheduling interval and float measurement rounding in observer.
    assert(jump<=.52*(1-Math.exp(-Math.min(.05,b.dt+.003)/.45))+.001,`orbit snapped ${jump}`);
    checked++;
   }
  }
  assert(checked>100);
  await page.keyboard.up('KeyW');
  await page.getByRole('button',{name:'Camera',exact:true}).click();
  await page.waitForFunction(()=>window.atvTerrainStatus.followCamera.preset!==0&&window.atvTerrainStatus.cameraPresentation.requestedHillOrbit===0&&window.atvTerrainStatus.cameraPresentation.uphillDistanceScale===1);
  assertNoErrors(errors);
  console.log(JSON.stringify({viewport,orbit:hill.cameraPresentation.requestedHillOrbit,pullback:hill.cameraPresentation.uphillDistanceScale,position:hill.frame.origin,grounded:hill.frame.grounded,nativePitch:native,renderedPitch:hill.camera[4],samples:trace.length,checked,maxOrbitStep:maxStep}));
  await page.close();
 }
 console.log('PASS: desktop/portrait Caribou climb pulls back and orbits around the rider; lean-back trace remains bounded; alternate views restore baseline.');
} finally {await browser.close();}
