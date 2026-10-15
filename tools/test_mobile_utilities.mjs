// Actual touch regression: toolbar fingers remain independent of riding capture.
// Requires original runtime content; never runs in private-free CI.
import assert from 'node:assert/strict';
import {mkdirSync} from 'node:fs';
import {assertNoErrors,base,launch,newPage,waitForSteps} from './browser/harness.mjs';
const browser=await launch();
const output=process.env.ATV_MOBILE_UTILITY_SCREENSHOTS;
if(output)mkdirSync(output,{recursive:true});
const errors=[], results=[];
try {
 for(const [name,viewport] of [['portrait',{width:390,height:844}],['landscape',{width:844,height:390}]]) {
  const page=await newPage(browser,{viewport,hasTouch:true,isMobile:true},errors);
  await page.addInitScript(()=>localStorage.setItem('atv-mobile-one-handed','true'));
  await page.goto(new URL('/play.html?drive=1',base).href);
  await page.waitForFunction(()=>window.atvTerrainStatus?.simulation,null,{timeout:90000});
  const read=()=>page.evaluate(()=>window.atvTerrainStatus);
  const steps=(count=4)=>waitForSteps(page,count,{timeout:30000});
  const cdp=await page.context().newCDPSession(page);
  const touch=(type,points=[])=>cdp.send('Input.dispatchTouchEvent',{type,touchPoints:points.map(p=>({...p,radiusX:6,radiusY:6,force:1}))});
  const point=async(selector,id=2)=>{const box=await page.locator(selector).boundingBox();return {id,x:box.x+box.width/2,y:box.y+box.height/2};};
  const buttons=await page.locator('#toolbar button:not(#freestyle-minimap)').evaluateAll(nodes=>nodes.map(button=>{
   const rect=button.getBoundingClientRect();return {name:button.getAttribute('aria-label'),rect:rect.toJSON(),hit:document.elementFromPoint(rect.x+rect.width/2,rect.y+rect.height/2)?.closest('button')===button};
  }));
  assert.deepEqual(buttons.map(button=>button.name),['Pause','Camera']);
  for(const {rect,hit} of buttons){assert(rect.width>=44&&rect.height>=44);assert(rect.x>=0&&rect.y>=0&&rect.right<=viewport.width&&rect.bottom<=viewport.height);assert(hit,'A HUD or touch overlay must not cover utilities');}
  assert.equal(await page.locator('#touch-controls').getAttribute('data-one-handed'),'true');
  await page.evaluate(()=>{window.utilityClicks={menu:0,camera:0};document.querySelector('#toolbar').addEventListener('click',event=>{const button=event.target.closest('button');if(button)window.utilityClicks[button.id]++;});});
  const camera=await point('#camera'), pause=await point('#menu');
  // Idle primary finger: no riding input while pressing a utility, one action on release.
  let preset=(await read()).followCamera.preset;
  await touch('touchStart',[camera]);await steps();
  assert.equal((await read()).input.throttle,0);assert.equal((await read()).input.touch_drive.held,false);
  await touch('touchEnd',[camera]);await steps();
  assert.notEqual((await read()).followCamera.preset,preset);
  assert.equal(await page.evaluate(()=>window.utilityClicks.camera),1,'Primary touch must not activate twice');
  // Moving away or cancelling must not activate the button.
  preset=(await read()).followCamera.preset;
  await touch('touchStart',[camera]);await touch('touchMove',[{...camera,y:camera.y+90}]);await touch('touchMove',[camera]);await touch('touchEnd',[camera]);await steps();
  await touch('touchStart',[camera]);await touch('touchCancel');await steps();
  assert.equal((await read()).followCamera.preset,preset);
  assert.equal((await read()).input.touch_drive.held,false,'Dragging from toolbar into terrain must not start throttle');
  assert.equal(await page.evaluate(()=>window.utilityClicks.camera),1);
  // The second finger has no compatibility click on the old implementation.
  const drive={id:1,x:viewport.width*.58,y:viewport.height*.72};
  await touch('touchStart',[drive]);await steps();
  assert.equal((await read()).input.touch_drive.held,true);
  preset=(await read()).followCamera.preset;
  await touch('touchStart',[drive,camera]);await touch('touchEnd',[camera]);await steps();
  assert.notEqual((await read()).followCamera.preset,preset);
  assert.equal((await read()).input.touch_drive.held,true,'Camera finger must not release the riding finger');
  assert.equal(await page.evaluate(()=>window.utilityClicks.camera),2,'Second finger camera tap activates once');
  if(output)await page.screenshot({path:`${output}/mobile-utilities-held-${name}.png`});
  await touch('touchStart',[drive,pause]);await touch('touchEnd',[pause]);
  await page.waitForFunction(()=>document.querySelector('#frontend').dataset.page==='pause'&&!document.querySelector('#frontend').hidden);
  assert.equal(await page.evaluate(()=>window.utilityClicks.menu),1);
  // Time-based on purpose: let any in-flight step land, then prove the paused frame stays frozen.
  await page.waitForTimeout(100);const paused=(await read()).frame;
  await page.waitForTimeout(200);assert.deepEqual((await read()).frame,paused,'Pause touch freezes the simulation');
  assert(await page.locator('#touch-controls').isHidden());
  assert.equal((await read()).input.throttle,0);
  await touch('touchCancel');
  if(output)await page.screenshot({path:`${output}/mobile-utilities-pause-${name}.png`});
  await page.getByRole('button',{name:'Resume',exact:true}).tap();await steps();
  assert.equal((await read()).input.touch_drive.held,false,'Resume cannot reuse the former captured riding finger');
  assert.equal((await read()).input.throttle,0);
  // Native mouse and keyboard button activation keep their normal click path.
  preset=(await read()).followCamera.preset;
  await page.locator('#camera').click();await steps();assert.notEqual((await read()).followCamera.preset,preset);
  preset=(await read()).followCamera.preset;
  await page.locator('#camera').focus();await page.keyboard.press('Enter');await steps();assert.notEqual((await read()).followCamera.preset,preset);
  await page.locator('#menu').tap();await page.waitForFunction(()=>document.querySelector('#frontend').dataset.page==='pause'&&!document.querySelector('#frontend').hidden);
  await page.getByRole('button',{name:'Resume',exact:true}).tap();await steps();
  assert.equal(await page.evaluate(()=>window.utilityClicks.menu),2,'Idle primary pause also activates once');
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth),viewport.width);
  results.push({viewport:name,buttons:buttons.map(({name,rect})=>({name,width:rect.width,height:rect.height})),primaryAndHeldTouch:true,cancelAndDrag:true,pauseFreezes:true});
  await page.close();
 }
 assertNoErrors(errors);
 console.log(JSON.stringify({gate:'mobile-utilities',results,errors}));
} finally { await browser.close(); }
