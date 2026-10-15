// Real-content menu/settings/HUD gate. Never includes source bytes in reports.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
const {chromium}=createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE||'playwright');
const browser=await chromium.launch({headless:true});
const base=process.env.ATV_TEST_URL||'http://127.0.0.1:4191';
const errors=[];
try {
 const page=await browser.newPage({viewport:{width:1280,height:720},deviceScaleFactor:2,hasTouch:true});
 page.on('pageerror',e=>errors.push(e.message));
 const screen=async name=>{await page.waitForFunction(name=>document.querySelector('#frontend').dataset.page===name&&!document.querySelector('#frontend').hidden&&!document.querySelector('#loading-screen').hidden===false,name,{timeout:90000});assert.equal(await page.locator('#frontend').getAttribute('data-error'),null);await page.waitForFunction(()=>!document.getAnimations().some(a=>a.effect?.target?.id==='menu-stage')&&document.querySelectorAll('.menu-transition').length===0);};
 const click=async name=>page.getByRole('button',{name,exact:true}).click();
 const shot=async name=>{if(process.env.ATV_HD_SCREENSHOTS)await page.screenshot({path:`${process.env.ATV_HD_SCREENSHOTS}-${name}.png`});};
 await page.goto(`${base}/play.html`);await screen('title');await shot('title');await click('Press START');await screen('main');await page.waitForFunction(()=>document.querySelector('.model-view canvas')?.dataset.ready==='true');await shot('main');
 await page.setViewportSize({width:320,height:568});await page.waitForTimeout(200);await shot('main-320');
 await page.setViewportSize({width:1280,height:720});await page.waitForFunction(()=>document.querySelector('.model-view canvas')?.dataset.ready==='true');
 await page.keyboard.press('ArrowDown');await page.keyboard.press('ArrowDown');await page.keyboard.press('Enter');await screen('settings');
 assert.deepEqual(await page.locator('.pause-panel button').allTextContents().then(a=>a.map(t=>t.trim())),['Controls','Display','Audio','Extras','Back']);
 for(const size of [{width:320,height:568},{width:390,height:844},{width:844,height:390},{width:1280,height:720}]){
  await page.setViewportSize(size);await page.waitForTimeout(100);await shot(`settings-${size.width}`);
  for(const name of ['Controls','Display','Audio','Extras']){
   await click(name);await screen({Controls:'controls',Display:'hud-settings',Audio:'music',Extras:'extras'}[name]);
   const panel=await page.locator('.pause-panel').boundingBox();assert(panel.x>=0&&panel.y>=0&&panel.x+panel.width<=size.width+1&&panel.y+panel.height<=size.height+1);
   for(const b of await page.locator('.pause-panel button').all()){const r=await b.boundingBox();assert(r.height>=44);}
   await shot(`${name.toLowerCase()}-${size.width}`);await click('Back');await screen('settings');
  }
 }
 await click('Extras');await screen('extras');await click('Crashes: Original');await click('Crashes: Crazy');await click('Crash camera: Tight');await click('Chase camera: Original');
 assert.equal(await page.evaluate(()=>localStorage.getItem('atv-crash-mode')),'gorn');assert.equal(await page.evaluate(()=>localStorage.getItem('atv-crash-camera')),'original');assert.equal(await page.evaluate(()=>localStorage.getItem('atv-chase-camera')),'assisted');
 await click('Back');await click('Controls');await screen('controls');await click('One-handed mobile: On');
 await page.getByRole('slider',{name:'Mobile steering sensitivity'}).fill('115');
 await click('Back');await click('Display');await screen('hud-settings');await click('Minimap configuration');await screen('minimap-config');
 await click('Shape: Circular');await page.getByRole('slider',{name:'Minimap size',exact:true}).fill('192');
 await page.reload();await screen('title');await click('Press START');await click('Settings');await click('Extras');await screen('extras');
 await page.getByRole('button',{name:'Crashes: Gorn (blood & limbs)',exact:true}).waitFor();await page.getByRole('button',{name:'Crash camera: Original',exact:true}).waitFor();await page.getByRole('button',{name:'Chase camera: Assisted',exact:true}).waitFor();
 await click('Crash camera: Original');await click('Back');await click('Controls');await screen('controls');assert.equal(await page.getByRole('slider').inputValue(),'115');await click('One-handed mobile: Off');
 await click('Back');await click('Display');await click('Minimap configuration');await screen('minimap-config');await page.getByRole('button',{name:'Shape: Square',exact:true}).waitFor();assert.equal(await page.getByRole('slider',{name:'Minimap size',exact:true}).inputValue(),'192');
 await page.goto(`${base}/play.html?mode=single&map=env01`);await page.waitForFunction(()=>window.atvTerrainStatus?.simulation&&document.querySelector('#frontend').hidden,null,{timeout:90000});
 await page.keyboard.down('KeyW');await page.waitForTimeout(1000);await page.keyboard.up('KeyW');await shot('hud');
 assert.equal(await page.locator('#toolbar .atv-control').first().evaluate(e=>getComputedStyle(e).backgroundColor),'rgba(0, 0, 0, 0)');
 await page.keyboard.press('Escape');await screen('pause');await shot('pause');await click('Settings');await screen('settings');await click('Display');await click('Minimap configuration');await click('Freestyle minimap: On');
 await click('Back');await click('Back');await click('Back');await screen('pause');await click('Resume');await page.waitForFunction(()=>!document.querySelector('#native-hud').hidden);await shot('hud-standalone');
 assert.equal(await page.evaluate(()=>window.atvTerrainStatus.glError),0);assert.deepEqual(errors,[]);
 console.log('PASS: HD menu, four sections from main/pause, four viewport layouts, 44px targets, saved controls/map/crash/camera, transparent HUD buttons, real driving and standalone HUD');
}finally{await browser.close();}
