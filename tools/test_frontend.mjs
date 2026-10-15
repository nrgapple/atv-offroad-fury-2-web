// Opt-in hosted original-content integration; source assets never enter CI.
// ATV_TEST_URL=<host/public HTTPS> PLAYWRIGHT_MODULE=<module> node tools/test_frontend.mjs
import assert from 'node:assert/strict';
import { ENVIRONMENTS } from '../web/launch.js';
import { assertNoErrors, base, button, gameState, holdKey, launch, newPage, nextFrames, waitForAnimations, waitForGame, waitForScreen } from './browser/harness.mjs';
const browser = await launch();
const errors = [];
try {
  const page = await newPage(browser, { viewport: { width: 1280, height: 896 }, hasTouch: true }, errors);
  const screen = async name => {
    console.log(`Checking ${name}`);
    await waitForScreen(page, name);
    if (process.env.ATV_UI_SCREENSHOTS) { await waitForAnimations(page); await page.screenshot({ path: `${process.env.ATV_UI_SCREENSHOTS}-${name}.png` }); }
  };
  const pad = async (button, navigation = false) => {
    await page.evaluate(button => {
      window.menuPad = { connected:true, axes:[0,0,0,0], buttons:Array.from({length:17},()=>({pressed:false,value:0})) };
      Object.defineProperty(navigator,'getGamepads',{configurable:true,value:()=>[window.menuPad]});
    }, button);
    // Every shown menu deliberately blocks held controller input until a
    // neutral poll. Observe neutral frames before synthesizing a new press.
    await nextFrames(page, 2);
    await page.evaluate(button => { window.menuPad.buttons[button] = {pressed:true,value:1}; }, button);
    if (navigation) { await page.waitForURL(url => ['/play','/play.html'].includes(url.pathname)); return; }
    await nextFrames(page, 3);
    await page.evaluate(()=>{if(window.menuPad) window.menuPad.buttons=window.menuPad.buttons.map(()=>({pressed:false,value:0}));});
    await nextFrames(page, 3);
  };
  // The menu applies a press after an asynchronous worker layout; wait for the track change.
  const padTrack = async button => {
    const before = await page.locator('#frontend').getAttribute('data-track');
    await pad(button);
    await page.waitForFunction(before => document.querySelector('#frontend').dataset.track !== before, before);
  };
  await page.goto(base);
  await screen('title');
  assert.equal(await page.locator('#disc').isHidden(), true);
  assert.equal((await gameState(page)).simulation, false);
  await pad(13);
  await page.waitForFunction(() => document.querySelector('#frontend').dataset.selection === '1');
  await pad(0); await page.waitForURL('**/devlog.html');
  await page.getByRole('heading',{name:'Development log',exact:true}).waitFor();
  await page.getByRole('link',{name:'Play current build'}).click(); await screen('title');
  await page.keyboard.press('Enter'); await screen('main');
  assert.equal(await button(page,'Multiplayer').count(),1);
  assert.equal(await page.locator('#frontend').getAttribute('data-renderer'),'rust');
  assert.equal(await button(page,'Dev log').count(),1);
  await page.keyboard.press('ArrowDown');
  await page.waitForFunction(() => document.querySelector('#frontend').dataset.selection === '1');
  await page.keyboard.press('ArrowUp');
  await page.waitForFunction(() => document.querySelector('#frontend').dataset.selection === '0');
  await page.keyboard.press('Enter'); await screen('single');
  assert((await button(page,'Race').count()) <= 1);
  await pad(13); await page.waitForFunction(() => document.querySelector('#frontend').dataset.selection === '1'); await pad(12);
  await pad(0); await screen('tracks');
  assert.equal(await page.locator('#frontend').getAttribute('data-track'),'env01');
  const visited = new Set();
  for (let index = 0; index < ENVIRONMENTS.length; index++) {
    visited.add(await page.locator('#frontend').getAttribute('data-track'));
    await padTrack(15);
  }
  assert.deepEqual([...visited].sort(), [...ENVIRONMENTS].sort());
  for (let index = 0; index < ENVIRONMENTS.length && await page.locator('#frontend').getAttribute('data-track') !== 'env02'; index++) await padTrack(15);
  assert.equal(await page.locator('#frontend').getAttribute('data-track'),'env02');
  if (process.env.ATV_UI_SCREENSHOTS) await page.screenshot({path:`${process.env.ATV_UI_SCREENSHOTS}-tioga-selection.png`});
  await pad(0);
  console.log("Starting Tioga", await page.locator("#menu-notice").textContent());
  let state = await waitForGame(page, {timeout:90000, until:()=>document.querySelector('#frontend').hidden});
  assert.match(state.source,/env02/); assert(state.simulation); assert(state.frame.origin.every(Number.isFinite)); assert(state.obstacles.instances>0);
  await holdKey(page, 'KeyW', 72);
  state = await gameState(page); assert(state.frame.speed > 0); assert.equal(state.glError,0);
  if (process.env.ATV_UI_SCREENSHOTS) await page.screenshot({path:`${process.env.ATV_UI_SCREENSHOTS}-tioga-driving.png`});
  await pad(9); await screen('pause');
  // Time-based on purpose: let any in-flight step settle, then prove the paused frame does not change.
  await page.waitForTimeout(100); const paused = await page.evaluate(()=>window.atvTerrainStatus.frame);
  await page.waitForTimeout(150); assert.deepEqual(await page.evaluate(()=>window.atvTerrainStatus.frame),paused,'Menus pause the Rust vehicle');
  await button(page,'Settings').click(); await screen('settings');
  await button(page,'Controls').click(); await screen('controls');
  await page.keyboard.press('Escape'); await screen('settings'); await page.keyboard.press('Escape'); await screen('pause');
  await button(page,'Resume').click();
  await page.waitForFunction(()=>document.querySelector('#frontend').hidden);
  await page.keyboard.press('Escape'); await screen('pause');
  await button(page,'Quit to Main Menu').click(); await screen('main');
  await page.setViewportSize({width:390,height:844});
  await button(page,'Single Player').tap(); await screen('single');
  await button(page,'Freestyle').tap(); await screen('tracks');
  for (let index = 0; index < ENVIRONMENTS.length && await page.locator('#frontend').getAttribute('data-track') !== 'env01'; index++) {
    const before = await page.locator('#frontend').getAttribute('data-track');
    await button(page,'Previous environment').tap();
    await page.waitForFunction(before => document.querySelector('#frontend').dataset.track !== before,before);
  }
  assert.equal(await page.locator('#frontend').getAttribute('data-track'),'env01');
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth),390);
  const stage = await page.locator('#menu-stage').boundingBox();
  assert.equal(stage.width,390); assert.equal(stage.height,844);
  for (const button of await page.locator('#menu-buttons button').all()) assert((await button.boundingBox()).height >= 44);
  if (process.env.ATV_UI_SCREENSHOTS) await page.screenshot({path:`${process.env.ATV_UI_SCREENSHOTS}-mobile.png`});
  await button(page,'Ride Caribou Peak').tap();
  state = await waitForGame(page, {timeout:90000, until:()=>document.querySelector('#frontend').hidden});
  assert.match(state.source,/env01/); assert(state.frame.origin.every(Number.isFinite)); assert.equal(state.glError,0);
  await page.keyboard.press('Escape'); await screen('pause');
  assert.equal(await page.locator('#fullscreen').isHidden(),true);
  assert.equal(await page.locator('#reset').isHidden(),true);
  assert.equal(await page.locator('#help').isVisible(),false);
  assert.equal(await page.evaluate(()=>getComputedStyle(document.body).userSelect),'none');
  const resume = button(page,'Resume');
  await resume.waitFor({state:'visible'});
  const resumeBounds = await resume.boundingBox();
  assert(resumeBounds.height >= 43, 'Portrait pause targets remain finger-sized');
  await button(page,'Settings').tap(); await screen('settings');
  await button(page,'Controls').tap(); await screen('controls');
  if (process.env.ATV_UI_SCREENSHOTS) await page.screenshot({path:`${process.env.ATV_UI_SCREENSHOTS}-controls-mobile.png`});
  await button(page,'Back').tap(); await screen('settings');
  await button(page,'Back').tap(); await screen('pause');
  await button(page,'Resume').tap();
  await page.waitForFunction(()=>document.querySelector('#frontend').hidden);
  assert(await button(page,'Pause').isVisible());
  assert(await button(page,'Camera').isVisible());
  await page.goto(base);
  await screen('title');
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth),390);
  if (process.env.ATV_UI_SCREENSHOTS) await page.screenshot({path:`${process.env.ATV_UI_SCREENSHOTS}-start-mobile.png`});
  await button(page,'Dev log').tap();
  await page.getByRole('heading',{name:'Development log',exact:true}).waitFor();
  await page.getByRole('link',{name:'Play current build'}).tap(); await screen('title');
  assertNoErrors(errors);
  console.log('PASS: original title/menu/track resources, keyboard and controller navigation, only supported menu routes, Tioga drive, paused simulation, mobile touch selection, Caribou reload, no disc prompt or browser errors.');
} finally { await browser.close(); }
