// Real game/menu geometry with synthetic installed-iOS metrics. No OS install.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
const engines = createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || 'playwright');
const engine = process.env.ATV_IOS_BROWSER || 'chromium';
const browser = await engines[engine].launch({ headless: true });
const base = process.env.ATV_TEST_URL || 'http://127.0.0.1:4338';
const errors = [];
try {
  const page = await browser.newPage({ viewport: { width: 390, height: 844 }, hasTouch: true });
  page.on('pageerror', e => errors.push(e.message));
  await page.addInitScript(() => {
    Object.defineProperty(navigator, 'standalone', { value: true });
    Object.defineProperty(navigator, 'userAgent', { value: 'Mozilla/5.0 (iPhone) AppleWebKit Safari' });
    // WebKit's shorter available height, with physical CSS screen held portrait.
    Object.defineProperty(screen, 'width', { get: () => 390 });
    Object.defineProperty(screen, 'height', { get: () => 844 });
    Object.defineProperty(visualViewport, 'height', { get: () => innerHeight - (innerWidth < innerHeight ? 59 : 21) });
  });
  await page.goto(base + '/play.html?drive=1&environment=env01');
  await page.waitForFunction(() => window.atvTerrainStatus?.simulationSteps > 60, null, { timeout: 120000 });
  async function check(width, height, safe) {
    await page.setViewportSize({ width, height });
    await page.evaluate(safe => { for (const [side,px] of Object.entries(safe)) document.documentElement.style.setProperty(`--app-safe-${side}`, `${px}px`); }, safe);
    await page.waitForFunction(({width,height}) => { const c = document.querySelector('#game'); return c.clientWidth === width && c.clientHeight === height && c.width === Math.round(width*Math.min(devicePixelRatio,1.5)) && c.height === Math.round(height*Math.min(devicePixelRatio,1.5)); }, {width,height});
    await page.locator('#freestyle-minimap').waitFor({ state: 'visible', timeout: 60000 });
    const result = await page.evaluate(() => {
      const box = id => document.getElementById(id).getBoundingClientRect().toJSON();
      return { game: box('game'), toolbar: box('toolbar'), map: box('freestyle-minimap'), scroll: [document.documentElement.scrollWidth, document.documentElement.scrollHeight] };
    });
    assert.equal(result.game.x, 0); assert.equal(result.game.y, 0); assert.equal(result.game.bottom, height); assert.equal(result.game.right, width);
    assert(result.toolbar.y >= safe.top && result.toolbar.x >= safe.left);
    assert(result.map.y >= safe.top && result.map.right <= width - safe.right, JSON.stringify(result));
    assert(result.scroll[0] <= width && result.scroll[1] <= height);
    if (process.env.ATV_IOS_SCREENSHOTS) await page.screenshot({ path: `${process.env.ATV_IOS_SCREENSHOTS}-${engine}-${width}.png` });
    await page.getByRole('button', {name:'Pause', exact:true}).click();
    await page.getByRole('button', {name:'Resume',exact:true}).waitFor();
    await page.waitForFunction(() => document.querySelector('#menu-stage').getAnimations().every(a => a.playState === 'finished'));
    const stage = await page.locator('#menu-stage').boundingBox();
    assert.equal(stage.y,safe.top); assert.equal(stage.x,safe.left); assert.equal(stage.y+stage.height,height-safe.bottom);
    await page.getByRole('button', {name:'Resume',exact:true}).click();
    console.log(JSON.stringify({ engine, width, height, safe, result }));
  }
  await check(390,844,{top:59,right:0,bottom:34,left:0});
  await check(844,390,{top:0,right:59,bottom:21,left:59});
  await check(390,844,{top:59,right:0,bottom:34,left:0});
  assert.deepEqual(errors,[]);
  console.log('PASS: installed-iOS gap compensation, edge-to-edge game backing, safe HUD/pause and live orientation changes. Physical installation remains untested.');
} finally { await browser.close(); }
