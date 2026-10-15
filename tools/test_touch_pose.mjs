// Opt-in original-content pose check. The native preload force remains gated.
import assert from 'node:assert/strict';
import { assertNoErrors, base, launch, newPage, waitForSteps } from './browser/harness.mjs';
const browser=await launch();
try {
  const errors=[]; const page=await newPage(browser,{viewport:{width:390,height:844},hasTouch:true,isMobile:true},errors);
  await page.addInitScript(() => localStorage.setItem('atv-mobile-one-handed', 'false')); // Explicit optional two-finger mode.
  await page.goto(new URL('/play.html?drive=1',base).href);
  await page.waitForFunction(()=>window.atvTerrainStatus?.simulation&&window.atvTerrainStatus.frame.grounded.every(Boolean),null,{timeout:90000});
  const cdp=await page.context().newCDPSession(page);
  const steps=count=>waitForSteps(page,count);
  await cdp.send('Input.dispatchTouchEvent',{type:'touchStart',touchPoints:[{id:1,x:250,y:240,radiusX:6,radiusY:6,force:1}]});
  await steps(30);
  const held=await page.evaluate(()=>window.atvTerrainStatus);
  assert.equal(held.input.lean,0);assert.equal(held.input.preload_axis,1);assert.equal(held.frame.preload_active,false);
  if(process.env.ATV_TOUCH_POSE_PREFIX) await page.screenshot({path:`${process.env.ATV_TOUCH_POSE_PREFIX}-held.png`});
  await cdp.send('Input.dispatchTouchEvent',{type:'touchEnd',touchPoints:[]});
  await steps(30);
  const released=await page.evaluate(()=>window.atvTerrainStatus);
  assert.notDeepEqual(released.frame.rider_node_matrices,held.frame.rider_node_matrices);
  assert.equal(released.input.lean,0);assert.equal(released.frame.preload_active,false);
  if(process.env.ATV_TOUCH_POSE_PREFIX) await page.screenshot({path:`${process.env.ATV_TOUCH_POSE_PREFIX}-released.png`});
  assertNoErrors(errors);
  console.log('PASS: right hold/release changes original rider matrices with neutral physical lean; no flat-ground preload force.');
} finally {await browser.close();}
