// Opt-in actual UI capture, persisted remapping, music menu and DPR checks.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
const {chromium} = createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || 'playwright');
const base = process.env.ATV_TEST_URL || 'http://127.0.0.1:4173';
const browser = await chromium.launch({headless:true});
const errors=[];
try {
  const page = await browser.newPage({viewport:{width:390,height:844},deviceScaleFactor:3,hasTouch:true});
  page.on('pageerror',e=>errors.push(e.message));
  const screen = async name => { await page.waitForFunction(name=>document.querySelector('#frontend').dataset.page===name && !document.querySelector('#frontend').hidden || document.querySelector('#frontend').dataset.error,name,{timeout:90000}); assert.equal(await page.locator('#frontend').getAttribute('data-error'),null,await page.locator('#menu-notice').textContent()); };
  const click = name => page.getByRole('button',{name,exact:true}).click();
  const state = () => page.evaluate(()=>window.atvTerrainStatus);
  const screenshot = async name => {if(process.env.ATV_SETTINGS_SCREENSHOTS)await page.screenshot({path:`${process.env.ATV_SETTINGS_SCREENSHOTS}-${name}.png`});};
  await page.goto(base+'/play.html?drive=1');
  await page.waitForFunction(()=>window.atvTerrainStatus?.ready && window.atvTerrainStatus?.audio.ready,null,{timeout:90000});
  await click('Pause'); await screen('pause');
  await click('Controls'); await screen('controls');
  await click('Keyboard bindings'); await screen('bindings');
  await page.getByRole('button',{name:/^Throttle:/}).click();
  await page.getByRole('button',{name:'Cancel',exact:true}).waitFor();
  await page.keyboard.press('KeyT');
  await page.getByRole('button',{name:'Throttle: T',exact:true}).waitFor();
  await screenshot('keyboard');
  assert.equal(await page.evaluate(()=>document.querySelector('#menu-canvas').width),1170,'Menu uses 3x device resolution');
  await click('Back');await screen('controls');await click('Back');await screen('pause');await click('Resume');
  await page.keyboard.down('KeyW');await page.waitForTimeout(200);assert.equal((await state()).input.throttle,0);await page.keyboard.up('KeyW');
  await page.keyboard.down('KeyT');await page.waitForFunction(()=>window.atvTerrainStatus.input?.throttle===1);await page.keyboard.up('KeyT');
  await page.waitForFunction(()=>window.atvTerrainStatus.input?.throttle===0);
  // Keyboard capture persists across an actual page reload.
  await page.reload();await page.waitForFunction(()=>window.atvTerrainStatus?.ready && window.atvTerrainStatus.simulation,null,{timeout:90000});await page.locator('#game').focus();
  await page.keyboard.down('KeyT');await page.waitForFunction(()=>window.atvTerrainStatus.input?.throttle===1);await page.keyboard.up('KeyT');
  await click('Pause');await screen('pause');await click('Controls');await screen('controls');await click('Controller bindings');await screen('bindings');
  await page.evaluate(()=>{window.settingsPad={connected:true,index:0,id:'settings-test',axes:[0,0,0,0],buttons:Array.from({length:17},()=>({pressed:false,value:0}))};Object.defineProperty(navigator,'getGamepads',{configurable:true,value:()=>[window.settingsPad]});});
  await page.getByRole('button',{name:/^Throttle:/}).click();await page.getByRole('button',{name:'Cancel',exact:true}).waitFor();await page.waitForTimeout(100);
  await page.evaluate(()=>window.settingsPad.buttons[5]={pressed:true,value:1});await page.waitForTimeout(100);await page.evaluate(()=>window.settingsPad.buttons[5]={pressed:false,value:0});
  await page.getByRole('button',{name:'Throttle: Button 5',exact:true}).waitFor();
  // Signed stick-axis remapping is also captured by actual controller movement.
  await page.getByRole('button',{name:/^Steer left:/}).click();await page.getByRole('button',{name:'Cancel',exact:true}).waitFor();await page.waitForTimeout(100);
  await page.evaluate(()=>window.settingsPad.axes[2]=-.8);await page.waitForTimeout(100);await page.evaluate(()=>window.settingsPad.axes[2]=0);
  await page.getByRole('button',{name:/^Steer left: Axis 2/}).waitFor();await screenshot('controller');
  for(const button of await page.locator('#menu-buttons button').all())assert((await button.boundingBox()).height>=44);
  await click('Back');await click('Back');await screen('pause');await click('Resume');
  await page.evaluate(()=>window.settingsPad.buttons[5]={pressed:true,value:.75});await page.waitForFunction(()=>window.atvTerrainStatus.input?.throttle===.75);
  await page.evaluate(()=>window.settingsPad=null);await page.waitForFunction(()=>window.atvTerrainStatus.input?.throttle===0);
  await click('Pause');await screen('pause');await click('Controls');await screen('controls');await click('Controller bindings');await screen('bindings');
  await click('Clear');await page.getByRole('button',{name:'Throttle: Unbound',exact:true}).waitFor();
  await click('Defaults');await page.getByRole('button',{name:/^Throttle: Button 0 \/ Button 7$/}).waitFor();
  await click('Back');await screen('controls');await click('Back');await screen('pause');await click('Music');await screen('music');
  const selected=new Set();
  for(let p=0;p<6;p++){
    const rows=await page.getByRole('button').allTextContents();
    for(const id of await page.locator('[data-action^="song:"]').evaluateAll(buttons=>buttons.map(button=>button.dataset.action)))selected.add(id);
    if(p===0){await page.getByRole('button',{name:rows.find(row=>row.includes(':')) ,exact:true}).click();await page.waitForFunction(()=>window.atvTerrainStatus.audio.music.playingIndex===0 && window.atvTerrainStatus.audio.music.voices>0);await screenshot('music');}
    if(p<5){await click('Next');await page.waitForFunction(p=>document.querySelector('#frontend').dataset.musicPage===String(p),p+1);}
  }
  assert.equal(selected.size,21,'All original songs have selector entries');
  await page.waitForFunction(()=>window.atvTerrainStatus.audio.musicPreview && window.atvTerrainStatus.audio.context==='running' && window.atvTerrainStatus.audio.music.voices>0);
  const frozen=(await state()).frame;await page.waitForTimeout(200);assert.deepEqual((await state()).frame,frozen,'Music preview keeps physics paused');
  await click('Music off');await page.waitForFunction(()=>!window.atvTerrainStatus.audio.music.enabled && window.atvTerrainStatus.audio.music.voices===0);
  await click('Music on');await page.waitForFunction(()=>window.atvTerrainStatus.audio.music.voices>0);
  await click('Volume');assert.equal((await state()).audio.music.volume,.25*Math.round((await state()).audio.music.volume*4));
  await page.evaluate(()=>window.dispatchEvent(new Event('blur')));await page.waitForFunction(()=>window.atvTerrainStatus.audio.context==='suspended' && !window.atvTerrainStatus.audio.musicPreview);
  await page.evaluate(()=>window.dispatchEvent(new Event('focus')));await page.waitForFunction(()=>window.atvTerrainStatus.audio.musicPreview);
  await click('Back');await screen('pause');await page.waitForFunction(()=>!window.atvTerrainStatus.audio.musicPreview && window.atvTerrainStatus.audio.context==='suspended');
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth),390);assert.deepEqual(errors,[]);
  console.log('PASS: keyboard/button/signed-axis capture, actual drive, reload persistence, 21-song menu, silent paused preview/focus/off/on, 3x menu rendering and mobile bounds.');
} finally {await browser.close();}
