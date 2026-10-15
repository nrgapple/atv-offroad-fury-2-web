import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';
import { assertNoErrors, base, launch, newPage, nextFrames, waitForAnimations, waitForGame } from './browser/harness.mjs';
const out = process.env.ATV_MINIMAP_OUTPUT || 'work/verification/minimap-orientation';
await mkdir(out, {recursive:true});
const browser = await launch();
const errors=[], page = await newPage(browser,{viewport:{width:390,height:844},hasTouch:true},errors);
try {
  await page.goto(`${base}/play.html?drive=1&environment=env02`);
  const state=()=>page.evaluate(()=>window.atvTerrainStatus);
  await waitForGame(page,{until:()=>window.atvTerrainStatus.minimap.visible});
  const map=page.locator('#freestyle-minimap'), pixels=()=>map.locator('canvas').evaluate(c=>Array.from(c.getContext('2d').getImageData(0,0,c.width,c.height).data));
  const before=await state(), north=await pixels();
  assert.equal(before.minimap.orientation,'north-up'); assert.equal(before.minimap.mapRotation,0);
  await map.click(); await page.waitForFunction(()=>window.atvTerrainStatus.minimap.orientation==='heading-up' && window.atvTerrainStatus.minimap.markerRotation===0);
  const heading=await state(), rotated=await pixels();
  assert(Math.abs(heading.minimap.mapRotation+heading.minimap.heading)<1e-10);
  assert.equal(heading.minimap.builds,before.minimap.builds);
  assert(rotated.filter((v,i)=>v!==north[i]).length>128,'Actual course pixels rotate, not just the marker');
  assert.equal(heading.input.throttle,0,'Mouse activation does not drive');
  await page.screenshot({path:`${out}/heading-up.png`});
  await map.focus(); await page.keyboard.press('Enter'); await page.waitForFunction(()=>window.atvTerrainStatus.minimap.orientation==='north-up');
  assert.equal((await state()).input.throttle,0);
  const cdp=await page.context().newCDPSession(page), box=await map.boundingBox(), x=box.x+box.width/2,y=box.y+box.height/2;
  const point=(id,x,y)=>({id,x,y,radiusX:5,radiusY:5,force:1});
  // Drive capture stays on the game surface while a second finger taps GPS.
  await cdp.send('Input.dispatchTouchEvent',{type:'touchStart',touchPoints:[point(1,150,600)]});
  await page.waitForFunction(()=>window.atvTerrainStatus.input.touch_drive?.held);
  const generation=(await state()).input.touch_drive.generation;
  await cdp.send('Input.dispatchTouchEvent',{type:'touchStart',touchPoints:[point(1,150,600),point(2,x,y)]});
  await cdp.send('Input.dispatchTouchEvent',{type:'touchEnd',touchPoints:[point(2,x,y)]});
  await page.waitForFunction(()=>window.atvTerrainStatus.minimap.orientation==='heading-up');
  assert.equal((await state()).input.touch_drive.generation,generation); assert((await state()).input.touch_drive.held);
  // Time-based on purpose: prove no delayed compatibility click toggles the map back.
  await page.waitForTimeout(150); assert.equal((await state()).minimap.orientation,'heading-up','No compatibility-click double toggle');
  await cdp.send('Input.dispatchTouchEvent',{type:'touchEnd',touchPoints:[]});
  await page.waitForFunction(()=>!window.atvTerrainStatus.input.touch_drive.held);
  // Dragging away and cancellation never activate the GPS button.
  await cdp.send('Input.dispatchTouchEvent',{type:'touchStart',touchPoints:[point(3,x,y)]});
  await cdp.send('Input.dispatchTouchEvent',{type:'touchMove',touchPoints:[point(3,x-70,y+60)]});
  await cdp.send('Input.dispatchTouchEvent',{type:'touchEnd',touchPoints:[]});
  assert.equal((await state()).minimap.orientation,'heading-up');
  await page.reload(); await waitForGame(page,{until:()=>window.atvTerrainStatus.minimap.visible});
  assert.equal((await state()).minimap.orientation,'heading-up');
  await page.getByRole('button',{name:'Pause',exact:true}).click(); await page.getByRole('button',{name:'HUD settings',exact:true}).click(); await page.getByRole('button',{name:'Minimap configuration',exact:true}).click();
  await page.getByRole('button',{name:'Orientation: Heading up',exact:true}).click();
  await page.getByRole('button',{name:'Orientation: North up',exact:true}).waitFor();
  await page.setViewportSize({width:844,height:390}); await nextFrames(page); await waitForAnimations(page);
  for(const r of await page.locator('#menu-buttons button, #menu-buttons input').evaluateAll(es=>es.map(e=>{const r=e.getBoundingClientRect();return {x:r.x,y:r.y,w:r.width,h:r.height};}))) assert(r.x>=0&&r.y>=0&&r.x+r.w<=844&&r.y+r.h<=390&&r.h>=43.99);
  assertNoErrors(errors);
  await writeFile(`${out}/results.json`,JSON.stringify({before:before.minimap,heading:heading.minimap,errors},null,2));
  console.log('PASS: actual north/heading-up pixels, keyboard/mouse, saved preference, second-finger tap during driving, no duplicate click or drag activation, and nested landscape settings.');
} finally {await browser.close();}
