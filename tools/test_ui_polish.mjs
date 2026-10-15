// Opt-in runtime-content UI gate: boot/course/model loaders, motion and rotation.
import assert from 'node:assert/strict';
import {assertNoErrors, base, button, launch, newPage, nextFrames, waitForGame} from './browser/harness.mjs';
const browser = await launch();
const errors = [];
const sizes = [{width:390,height:844},{width:844,height:390},{width:667,height:375}];
try {
  const page = await newPage(browser, {viewport:sizes[0],hasTouch:true}, errors);
  const shot = async name => { if (process.env.ATV_UI_SCREENSHOTS) await page.screenshot({path:`${process.env.ATV_UI_SCREENSHOTS}-${name}.png`}); };
  const screen = async name => {
    await page.waitForFunction(name => document.querySelector('#frontend').dataset.page === name && document.querySelector('#loading-screen').hidden, name, {timeout:90000});
    assert.equal(await page.locator('#frontend').getAttribute('data-error'),null,await page.locator('#menu-notice').textContent());
    await page.waitForFunction(()=>document.getAnimations().filter(a=>a.effect?.target?.id==='menu-stage').length===0);
  };
  const inViewport = async selector => {
    await page.waitForFunction(()=>Math.abs(document.querySelector('#game').getBoundingClientRect().width-innerWidth)<1 && Math.abs(document.querySelector('#game').getBoundingClientRect().height-innerHeight)<1);
    const box = await page.locator(selector).boundingBox(), size = page.viewportSize();
    assert(box && box.x>=-1 && box.y>=-1 && box.x+box.width<=size.width+1 && box.y+box.height<=size.height+1,`${selector} fits ${JSON.stringify(size)}: ${JSON.stringify(box)}`);
  };
  // Hold only startup WASM so the authored boot screen is observable.
  let releaseBoot;
  const boot = new Promise(resolve=>{releaseBoot=resolve;});
  await page.route('**/atv_web.wasm', async route=>{await boot;await route.continue();});
  await page.goto(`${base}/play.html`);
  for (const size of sizes) {
    await page.setViewportSize(size); await inViewport('.loading-panel');
    assert(await page.locator('#loading-screen').isVisible());
    assert(!(await page.locator('#menu-notice').isVisible()));
    await shot(`boot-${size.width}`);
  }
  releaseBoot(); await screen('title');
  await button(page,'Press START').tap(); await screen('main');
  // Navigate during entry motion; old snapshots must not survive the next route.
  await button(page,'Single Player').tap(); await screen('single');
  await button(page,'Freestyle').tap(); await screen('tracks');
  let releaseCourse;
  const course = new Promise(resolve=>{releaseCourse=resolve;});
  await page.route('**/content/*.bin', async route=>{await course;await route.continue();});
  await button(page,'Ride Caribou Peak').tap(); await screen('loading');
  for (const size of sizes) {
    await page.setViewportSize(size);
    await nextFrames(page, 3); await inViewport('.course-load-meter');
    assert(await page.locator('.course-load-meter').isVisible());
    assert.equal(await page.locator('#status').isVisible(),false);
    await shot(`course-${size.width}`);
  }
  releaseCourse();
  await waitForGame(page, {until: () => document.querySelector('#frontend').hidden && window.atvTerrainStatus.ready});
  for (const size of sizes) {
    await page.setViewportSize(size);
    await button(page,'Pause').tap(); await screen('pause');
    for (const button of await page.locator('#menu-buttons button').all()) {
      const box=await button.boundingBox();assert(box.height>=43,'Finger-sized pause target');
    }
    await shot(`pause-${size.width}`);
    await button(page,'Settings').tap(); await screen('settings');
    await button(page,'Controls').tap(); await screen('controls');
    await inViewport('input[type=range]');
    await button(page,'Back').tap(); await screen('settings'); await button(page,'Back').tap(); await screen('pause');
    await button(page,'Resume').tap();
    await page.waitForFunction(()=>document.querySelectorAll('.menu-transition').length===0,null,{timeout:5000});
  }
  // Context restoration gets a centered loader and returns to driving.
  await page.evaluate(()=>{window.contextRecovery=document.querySelector('#game').getContext('webgl2').getExtension('WEBGL_lose_context');window.contextRecovery.loseContext();});
  await page.locator('#loading-screen').waitFor({state:'visible'});await inViewport('.loading-panel');
  await page.evaluate(()=>window.contextRecovery.restoreContext());
  await page.locator('#loading-screen').waitFor({state:'hidden'});
  await button(page,'Pause').tap(); await screen('pause');
  await page.emulateMedia({reducedMotion:'reduce'});
  await button(page,'Settings').tap(); await screen('settings');
    await button(page,'Controls').tap();await screen('controls');
  assert.equal(await page.locator('.menu-transition').count(),0);
  assert.equal(await page.locator('.loading-meter').count(),1);
  await button(page,'Back').tap();await screen('settings');await button(page,'Back').tap();await screen('pause');
  await button(page,'Quit to Main Menu').tap();await screen('main');
  await button(page,'Single Player').tap();await screen('single');
  await button(page,'Quad and Rider').tap();await screen('garage');
  await page.waitForFunction(()=>document.querySelectorAll('.model-view canvas[data-ready=true]').length===2,null,{timeout:90000});
  assert.equal(await page.locator('.model-view.is-loading').count(),0);
  assertNoErrors(errors);
  // A failing content request must end startup loading and display the error.
  const failure=await browser.newPage({viewport:sizes[0]});
  await failure.route('**/content/manifest.json',route=>route.abort());
  await failure.goto(`${base}/play.html`);
  await failure.waitForFunction(()=>document.querySelector('#frontend').dataset.error==='true',{},{timeout:90000});
  assert.equal(await failure.locator('#loading-screen').isVisible(),false);
  assert(await failure.locator('#menu-notice').isVisible());
  console.log('PASS: startup/course/recovery/model loading, transitions, reduced motion, failure, portrait/landscape rotation and touch targets');
} finally {await browser.close();}
