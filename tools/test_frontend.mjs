// Opt-in hosted original-content integration; source assets never enter CI.
// ATV_TEST_URL=<host/public HTTPS> PLAYWRIGHT_MODULE=<module> node tools/test_frontend.mjs
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
const { chromium } = createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || 'playwright');
const base = process.env.ATV_TEST_URL || 'http://127.0.0.1:4173';
const browser = await chromium.launch({ headless: true });
const errors = [];
try {
  const page = await browser.newPage({ viewport: { width: 1280, height: 896 }, hasTouch: true });
  page.on('pageerror', e => errors.push(e.message));
  const screen = async name => {
    console.log(`Checking ${name}`);
    await page.waitForFunction(name => document.querySelector('#frontend').dataset.page === name || document.querySelector('#frontend').dataset.error, name, { timeout: 90000 });
    assert.equal(await page.locator('#frontend').getAttribute('data-error'), null, await page.locator('#menu-notice').textContent());
    if (process.env.ATV_UI_SCREENSHOTS) { await page.waitForTimeout(350); await page.screenshot({ path: `${process.env.ATV_UI_SCREENSHOTS}-${name}.png` }); }
  };
  const pad = async (button, navigation = false) => {
    await page.evaluate(button => {
      window.menuPad = { connected:true, axes:[0,0,0,0], buttons:Array.from({length:17},(_,i)=>({pressed:i===button,value:i===button?1:0})) };
      Object.defineProperty(navigator,'getGamepads',{configurable:true,value:()=>[window.menuPad]});
    }, button);
    if (navigation) { await page.waitForURL(url => ['/play','/play.html'].includes(url.pathname)); return; }
    await page.waitForTimeout(120);
    await page.evaluate(()=>{if(window.menuPad) window.menuPad.buttons=window.menuPad.buttons.map(()=>({pressed:false,value:0}));});
    await page.waitForTimeout(100);
  };
  await page.goto(base);
  await screen('title');
  assert.equal(await page.locator('#disc').isHidden(), true);
  assert.equal((await page.evaluate(()=>window.atvTerrainStatus)).simulation, false);
  await pad(13);
  await page.waitForFunction(() => document.querySelector('#frontend').dataset.selection === '1');
  await pad(0); await page.waitForURL('**/devlog.html');
  await page.getByRole('heading',{name:'Development log',exact:true}).waitFor();
  await page.getByRole('link',{name:'Play current build'}).click(); await screen('title');
  await page.keyboard.press('Enter'); await screen('main');
  assert.equal(await page.getByRole('button',{name:'Multiplayer',exact:true}).count(),0);
  assert.equal(await page.locator('#frontend').getAttribute('data-renderer'),'rust');
  assert.equal(await page.getByRole('button',{name:'Dev log',exact:true}).count(),1);
  await page.keyboard.press('ArrowDown');
  await page.waitForFunction(() => document.querySelector('#frontend').dataset.selection === '1');
  await page.keyboard.press('ArrowUp');
  await page.waitForFunction(() => document.querySelector('#frontend').dataset.selection === '0');
  await page.keyboard.press('Enter'); await screen('single');
  assert.equal(await page.getByRole('button',{name:'Race',exact:true}).count(),0);
  await pad(13); assert.equal(await page.locator('#frontend').getAttribute('data-selection'),'0');
  await pad(0); await screen('tracks');
  assert.equal(await page.locator('#frontend').getAttribute('data-track'),'env01');
  const visited = new Set();
  for (let index = 0; index < 8; index++) {
    visited.add(await page.locator('#frontend').getAttribute('data-track'));
    await pad(15);
  }
  assert.deepEqual([...visited].sort(), ['env01','env02','env03','env04','env05','env06','env07','env08']);
  for (let index = 0; index < 8 && await page.locator('#frontend').getAttribute('data-track') !== 'env02'; index++) await pad(15);
  assert.equal(await page.locator('#frontend').getAttribute('data-track'),'env02');
  if (process.env.ATV_UI_SCREENSHOTS) await page.screenshot({path:`${process.env.ATV_UI_SCREENSHOTS}-tioga-selection.png`});
  await pad(0);
  console.log("Starting Tioga", await page.locator("#menu-notice").textContent());
  await page.waitForFunction(()=>document.querySelector('#frontend').hidden && window.atvTerrainStatus.ready, null, {timeout:90000});
  let state = await page.evaluate(()=>window.atvTerrainStatus);
  assert.match(state.source,/env02/); assert(state.simulation); assert(state.frame.origin.every(Number.isFinite)); assert(state.obstacles.instances>0);
  await page.keyboard.down('KeyW'); await page.waitForTimeout(1200); await page.keyboard.up('KeyW');
  state = await page.evaluate(()=>window.atvTerrainStatus); assert(state.frame.speed > 0); assert.equal(state.glError,0);
  if (process.env.ATV_UI_SCREENSHOTS) await page.screenshot({path:`${process.env.ATV_UI_SCREENSHOTS}-tioga-driving.png`});
  await pad(9); await screen('pause');
  await page.waitForTimeout(100); const paused = await page.evaluate(()=>window.atvTerrainStatus.frame);
  await page.waitForTimeout(150); assert.deepEqual(await page.evaluate(()=>window.atvTerrainStatus.frame),paused,'Menus pause the Rust vehicle');
  await page.getByRole('button',{name:'Controls',exact:true}).click(); await screen('controls');
  await page.keyboard.press('Escape'); await screen('pause');
  await page.getByRole('button',{name:'Resume',exact:true}).click();
  await page.waitForFunction(()=>document.querySelector('#frontend').hidden);
  await page.keyboard.press('Escape'); await screen('pause');
  await page.getByRole('button',{name:'Quit to Main Menu',exact:true}).click(); await screen('main');
  await page.setViewportSize({width:390,height:844});
  await page.getByRole('button',{name:'Single Player',exact:true}).tap(); await screen('single');
  await page.getByRole('button',{name:'Freestyle',exact:true}).tap(); await screen('tracks');
  for (let index = 0; index < 8 && await page.locator('#frontend').getAttribute('data-track') !== 'env01'; index++) {
    const before = await page.locator('#frontend').getAttribute('data-track');
    await page.getByRole('button',{name:'Previous environment',exact:true}).tap();
    await page.waitForFunction(before => document.querySelector('#frontend').dataset.track !== before,before);
  }
  assert.equal(await page.locator('#frontend').getAttribute('data-track'),'env01');
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth),390);
  const stage = await page.locator('#menu-stage').boundingBox();
  assert.equal(stage.width,390); assert.equal(stage.height,844);
  for (const button of await page.locator('#menu-buttons button').all()) assert((await button.boundingBox()).height >= 44);
  if (process.env.ATV_UI_SCREENSHOTS) await page.screenshot({path:`${process.env.ATV_UI_SCREENSHOTS}-mobile.png`});
  await page.getByRole('button',{name:'Ride Caribou Peak',exact:true}).tap();
  await page.waitForFunction(()=>document.querySelector('#frontend').hidden&&window.atvTerrainStatus.ready,null,{timeout:90000});
  state = await page.evaluate(()=>window.atvTerrainStatus);
  assert.match(state.source,/env01/); assert(state.frame.origin.every(Number.isFinite)); assert.equal(state.glError,0);
  await page.keyboard.press('Escape'); await screen('pause');
  assert.equal(await page.locator('#fullscreen').isHidden(),true);
  assert.equal(await page.locator('#reset').isHidden(),true);
  assert.equal(await page.locator('#help').isVisible(),false);
  assert.equal(await page.evaluate(()=>getComputedStyle(document.body).userSelect),'none');
  const resumeBounds = await page.getByRole('button',{name:'Resume',exact:true}).boundingBox();
  assert(resumeBounds.height >= 43, 'Portrait pause targets remain finger-sized');
  await page.getByRole('button',{name:'Controls',exact:true}).tap(); await screen('controls');
  if (process.env.ATV_UI_SCREENSHOTS) await page.screenshot({path:`${process.env.ATV_UI_SCREENSHOTS}-controls-mobile.png`});
  await page.getByRole('button',{name:'Back',exact:true}).tap(); await screen('pause');
  await page.getByRole('button',{name:'Resume',exact:true}).tap();
  await page.waitForFunction(()=>document.querySelector('#frontend').hidden);
  assert(await page.getByRole('button',{name:'Pause',exact:true}).isVisible());
  assert(await page.getByRole('button',{name:'Camera',exact:true}).isVisible());
  await page.goto(base);
  await screen('title');
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth),390);
  if (process.env.ATV_UI_SCREENSHOTS) await page.screenshot({path:`${process.env.ATV_UI_SCREENSHOTS}-start-mobile.png`});
  await page.getByRole('button',{name:'Dev log',exact:true}).tap();
  await page.getByRole('heading',{name:'Development log',exact:true}).waitFor();
  await page.getByRole('link',{name:'Play current build'}).tap(); await screen('title');
  assert.deepEqual(errors,[]);
  console.log('PASS: original title/menu/track resources, keyboard and controller navigation, only supported menu routes, Tioga drive, paused simulation, mobile touch selection, Caribou reload, no disc prompt or browser errors.');
} finally { await browser.close(); }
