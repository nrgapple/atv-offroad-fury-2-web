// Real browser menu/CDP touch events and original Caribou ramp; no state/force injection.
import assert from 'node:assert/strict';
import { captureSimulation } from './capture_simulation.mjs';
import { createRequire } from 'node:module';
const { chromium } = createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || 'playwright');
const browser = await chromium.launch({ headless: true });
try {
  const page = await browser.newPage({ viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true });
  await captureSimulation(page, 'landingFrames');
  const errors=[];page.on('pageerror',e=>errors.push(e.message));
  const url=new URL('/play.html?drive=1&environment=env01',process.env.ATV_TEST_URL || 'http://127.0.0.1:4173');
  await page.goto(url.href);
  await page.waitForFunction(()=>window.atvTerrainStatus?.simulation,null,{timeout:90000});
  await page.locator('#menu').click();
  await page.locator('[data-action="onehanded"]').click();
  await page.waitForFunction(()=>document.querySelector('[data-action="onehanded"]')?.textContent==='One-handed mobile: On');
  if(process.env.ATV_ONE_HAND_SCREENSHOT)await page.screenshot({path:process.env.ATV_ONE_HAND_SCREENSHOT});
  await page.reload();
  await page.waitForFunction(()=>window.atvTerrainStatus?.simulation,null,{timeout:90000});
  await page.locator('#menu').click();
  await page.waitForFunction(()=>document.querySelector('[data-action="onehanded"]')?.textContent==='One-handed mobile: On');
  await page.locator('[data-action="resume"]').click();
  const cdp=await page.context().newCDPSession(page);
  const touch=(type,points=[])=>cdp.send('Input.dispatchTouchEvent',{type,touchPoints:points.map(p=>({radiusX:6,radiusY:6,force:1,...p}))});
  const origin={id:1,x:60,y:560};
  await touch('touchStart',[origin]);
  await page.waitForFunction(()=>{const i=window.atvTerrainStatus.input;return i?.throttle===1 && i.brake===0 && !i.reverse && i.preload_axis===1 && i.lean===0;});
  for (const [dx,dy,steer,lean] of [[0,70,0,1],[0,-70,0,-1],[70,0,-1,0],[-70,0,1,0]]) {
    await touch('touchMove',[{...origin,x:origin.x+dx,y:origin.y+dy}]);
    await page.waitForFunction(([x,y])=>{const i=window.atvTerrainStatus.input;return i?.throttle===1 && i.brake===0 && !i.reverse && i.steer===x && i.lean===y && i.stunt_modifiers===0;},[steer,lean]);
  }
  await touch('touchEnd');
  await page.waitForFunction(()=>{const i=window.atvTerrainStatus.input;return i?.throttle===0 && i.brake===0 && !i.reverse && !i.touch_drive.held;});
  // Fresh original start for the ramp, retaining the saved mode.
  await page.reload();
  await page.waitForFunction(()=>window.atvTerrainStatus?.simulation,null,{timeout:90000});
  await touch('touchStart',[origin]);
  await page.waitForFunction(()=>window.atvTerrainStatus.frame.preload_armed,null,{timeout:45000,polling:'raf'});
  await touch('touchEnd');
  await page.waitForFunction(()=>!window.atvTerrainStatus.frame.grounded.some(Boolean),null,{timeout:5000,polling:'raf'});
  await touch('touchStart',[origin]);
  await touch('touchMove',[{...origin,x:130,y:630}]);
  await page.waitForFunction(()=>{const i=window.atvTerrainStatus.input;return i?.throttle===1 && i.brake===0 && !i.reverse && i.steer===-1 && i.lean===1 && i.stunt_modifiers===0 && i.preload_axis===1;},null,{timeout:5000,polling:'raf'});
  const heldAt = await page.evaluate(()=>window.landingFrames.at(-1)?.step ?? 0);
  await touch('touchMove',[origin]);
  await page.waitForFunction(start=>window.landingFrames.some((s,i,frames)=>s.step>start && i>0 && !frames[i-1].frame.grounded.some(Boolean) && s.frame.grounded.some(Boolean)),heldAt,{timeout:10000,polling:'raf'});
  await page.waitForFunction(start=>window.landingFrames.some(s=>s.step>start && s.frame.grounded.some(Boolean) && s.input?.throttle===1 && s.input.lean===0),heldAt,{timeout:5000,polling:'raf'});
  const frames=await page.evaluate(()=>window.landingFrames);
  const landing=frames.findIndex((s,i)=>s.step>heldAt && i>0 && !frames[i-1].frame.grounded.some(Boolean) && s.frame.grounded.some(Boolean));
  assert(landing>=0);
  const next=frames[landing+1];assert(next,'Next fixed step after touchdown recorded');
  assert.equal(next.frame.wreck.phase,'Riding');
  assert.equal(next.input.throttle,1,'Held contact must retain throttle across landing');
  assert.equal(next.input.brake,0);assert.equal(next.input.reverse,false);assert.equal(next.input.stunt_modifiers,0);
  if(process.env.ATV_ONE_HAND_LANDING_SCREENSHOT)await page.screenshot({path:process.env.ATV_ONE_HAND_LANDING_SCREENSHOT});
  await touch('touchEnd');
  await page.waitForFunction(()=>{const i=window.atvTerrainStatus.input;return i?.throttle===0 && i.brake===0 && !i.reverse && !i.touch_drive.held;});
  assert.deepEqual(errors,[]);
  console.log('PASS: saved one-handed setting, touch-to-throttle/brace, four steering/lean directions with no brake/reverse/tricks, release coasts, and continuous held throttle/brace through air and landing. Physical phone hardware unverified.');
} finally {await browser.close();}
