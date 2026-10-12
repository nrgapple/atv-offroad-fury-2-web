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
  const origin={id:1,x:120,y:560};
  const screenshot = async name => {
    if (process.env.ATV_ONE_HAND_FEEDBACK_SCREENSHOT_PREFIX) await page.screenshot({ path: `${process.env.ATV_ONE_HAND_FEEDBACK_SCREENSHOT_PREFIX}-${name}.png` });
  };
  const readout = page.locator('.touch-one-hand-readout');
  const expectFeedback = async (steering, leaning) => {
    assert.equal(await readout.locator('.touch-steer-status').textContent(), steering);
    assert.equal(await readout.locator('.touch-lean-status').textContent(), leaning);
    assert.equal(await readout.locator('.touch-drive-status').textContent(), 'THROTTLE + BRACE');
    assert(await readout.isVisible());
    const box = await readout.boundingBox();
    assert(box.x >= 0 && box.y >= 0 && box.x + box.width <= 390 && box.y + box.height <= 844, 'Feedback stays within the portrait viewport');
  };
  await touch('touchStart',[origin]);
  await page.waitForFunction(()=>{const i=window.atvTerrainStatus.input;return i?.throttle===1 && i.brake===0 && !i.reverse && i.preload_axis===1 && i.lean===0;});
  await expectFeedback('STRAIGHT', 'NEUTRAL');
  await screenshot('held');
  for (const [dx,dy,steer,lean,steering,leaning,name] of [
    [0,70,0,1,'STRAIGHT','BACK 100%','back'], [0,-70,0,-1,'STRAIGHT','FORWARD 100%','forward'],
    [70,0,-1,0,'RIGHT 100%','NEUTRAL','right'], [-70,0,1,0,'LEFT 100%','NEUTRAL','left'],
    [35,-35,-.5,-.5,'RIGHT 50%','FORWARD 50%','right-forward'],
    [-70,-70,1,-1,'LEFT 100%','FORWARD 100%','left-forward'],
    [70,70,-1,1,'RIGHT 100%','BACK 100%','right-back'], [-70,70,1,1,'LEFT 100%','BACK 100%','left-back'],
  ]) {
    await touch('touchMove',[{...origin,x:origin.x+dx,y:origin.y+dy}]);
    await page.waitForFunction(([x,y])=>{const i=window.atvTerrainStatus.input;return i?.throttle===1 && i.brake===0 && !i.reverse && i.steer===x && i.lean===y && i.stunt_modifiers===0;},[steer,lean]);
    await expectFeedback(steering, leaning);
    const vector = await page.locator('.touch-rider-marker').evaluate(marker => ({ x: parseFloat(marker.style.getPropertyValue('--steer')), y: parseFloat(marker.style.getPropertyValue('--lean')) }));
    assert.equal(vector.x, dx / 70 * 34); assert.equal(vector.y, dy / 70 * 34);
    await screenshot(name);
  }
  await touch('touchEnd');
  await page.waitForFunction(()=>{const i=window.atvTerrainStatus.input;return i?.throttle===0 && i.brake===0 && !i.reverse && !i.touch_drive.held;});
  assert.equal(await readout.locator('.touch-drive-status').textContent(), 'COASTING');
  assert.equal(await readout.locator('.touch-steer-status').textContent(), 'STRAIGHT');
  assert.equal(await readout.locator('.touch-lean-status').textContent(), 'NEUTRAL');
  await screenshot('coast');
  // Fresh original start for the ramp, retaining the saved mode.
  await page.reload();
  await page.waitForFunction(()=>window.atvTerrainStatus?.simulation,null,{timeout:90000});
  await touch('touchStart',[origin]);
  await page.waitForFunction(()=>window.atvTerrainStatus.frame.preload_armed,null,{timeout:45000,polling:'raf'});
  await touch('touchEnd');
  await page.waitForFunction(()=>!window.atvTerrainStatus.frame.grounded.some(Boolean),null,{timeout:5000,polling:'raf'});
  await touch('touchStart',[origin]);
  await touch('touchMove',[{...origin,x:190,y:630}]);
  await page.waitForFunction(()=>{const i=window.atvTerrainStatus.input;return i?.throttle===1 && i.brake===0 && !i.reverse && i.steer===-1 && i.lean===1 && i.stunt_modifiers===0 && i.preload_axis===1;},null,{timeout:5000,polling:'raf'});
  await expectFeedback('RIGHT 100%', 'BACK 100%');
  assert.equal(await readout.locator('.touch-phase').textContent(), 'AIR · SAME CONTROLS');
  await screenshot('air');
  // Angled-air capture is a separate input scenario: a long full pullback can
  // legitimately wreck. It must not delay neutralization in the clean landing gate.
  await touch('touchMove',[origin]);
  await touch('touchEnd');
  await page.reload();
  await page.waitForFunction(()=>window.atvTerrainStatus?.simulation,null,{timeout:90000});
  await touch('touchStart',[origin]);
  await page.waitForFunction(()=>window.atvTerrainStatus.frame.preload_armed,null,{timeout:45000,polling:'raf'});
  await touch('touchEnd');
  await page.waitForFunction(()=>!window.atvTerrainStatus.frame.grounded.some(Boolean),null,{timeout:5000,polling:'raf'});
  await touch('touchStart',[origin]);
  const heldAt = await page.evaluate(()=>window.landingFrames.at(-1)?.step ?? 0);
  await expectFeedback('STRAIGHT', 'NEUTRAL');
  assert.equal(await readout.locator('.touch-phase').textContent(), 'AIR · SAME CONTROLS');
  await page.waitForFunction(start=>window.landingFrames.some((s,i,frames)=>s.step>start && i>0 && !frames[i-1].frame.grounded.some(Boolean) && s.frame.grounded.some(Boolean)),heldAt,{timeout:10000,polling:'raf'});
  await page.waitForFunction(start=>window.landingFrames.some(s=>s.step>start && s.frame.grounded.some(Boolean) && s.input?.throttle===1 && s.input.lean===0),heldAt,{timeout:5000,polling:'raf'});
  const frames=await page.evaluate(()=>window.landingFrames);
  const landing=frames.findIndex((s,i)=>s.step>heldAt && i>0 && !frames[i-1].frame.grounded.some(Boolean) && s.frame.grounded.some(Boolean));
  assert(landing>=0);
  const next=frames[landing+1];assert(next,'Next fixed step after touchdown recorded');
  assert.equal(next.frame.wreck.phase,'Riding');
  assert.equal(next.input.throttle,1,'Held contact must retain throttle across landing');
  assert.equal(next.input.brake,0);assert.equal(next.input.reverse,false);assert.equal(next.input.stunt_modifiers,0);
  await expectFeedback('STRAIGHT', 'NEUTRAL');
  await page.waitForFunction(()=>document.querySelector('.touch-phase')?.textContent==='GROUND');
  await screenshot('landing');
  if(process.env.ATV_ONE_HAND_LANDING_SCREENSHOT)await page.screenshot({path:process.env.ATV_ONE_HAND_LANDING_SCREENSHOT});
  await touch('touchEnd');
  await page.waitForFunction(()=>{const i=window.atvTerrainStatus.input;return i?.throttle===0 && i.brake===0 && !i.reverse && !i.touch_drive.held;});
  assert.deepEqual(errors,[]);
  console.log('PASS: saved one-handed setting, touch-to-throttle/brace, cardinal/diagonal steering and lean readouts with proportional direction vectors with no brake/reverse/tricks, release coasts, and continuous held throttle/brace through air and landing. Physical phone hardware unverified.');
} finally {await browser.close();}
