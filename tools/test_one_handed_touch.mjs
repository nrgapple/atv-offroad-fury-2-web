// Real browser menu/CDP touch events and original Caribou ramp; no state/force injection.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
const { chromium } = createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || 'playwright');
const browser = await chromium.launch({ headless: true });
try {
  const page = await browser.newPage({ viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true });
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
  await page.waitForFunction(()=>window.atvTerrainStatus.input?.touch_drive.held && window.atvTerrainStatus.input.touch_drive.axis===0 && window.atvTerrainStatus.input.preload_axis===1);
  await touch('touchMove',[{...origin,y:630}]);
  await page.waitForFunction(()=>window.atvTerrainStatus.input?.touch_drive.axis===-1 && window.atvTerrainStatus.input.lean===1);
  await touch('touchMove',[{...origin,y:490}]);
  await page.waitForFunction(()=>window.atvTerrainStatus.input?.touch_drive.axis===1 && window.atvTerrainStatus.input.lean===-1);
  await page.waitForFunction(()=>window.atvTerrainStatus.frame.preload_armed,null,{timeout:45000,polling:'raf'});
  await touch('touchEnd');
  await page.waitForFunction(()=>!window.atvTerrainStatus.frame.grounded.some(Boolean),null,{timeout:5000,polling:'raf'});
  await touch('touchStart',[origin]);
  await touch('touchMove',[{...origin,x:130,y:630}]);
  await page.waitForFunction(()=>{const i=window.atvTerrainStatus.input;return i?.touch_drive.axis===0 && i.steer===-1 && i.lean===1 && i.stunt_modifiers===0 && i.preload_axis===undefined;},null,{timeout:5000,polling:'raf'});
  assert.deepEqual(errors,[]);
  await touch('touchCancel');
  console.log('PASS: saved one-handed toggle survives reload; left-side single thumb braces, drives, brakes/reverses through native gate and controls airborne steer/lean with no tricks, throttle or brace. Physical phone hardware unverified.');
} finally {await browser.close();}
