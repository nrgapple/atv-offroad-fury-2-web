// Weather in the Rust engine (tools/test_weather_lifecycle.mjs and
// tools/test_snow.mjs on play.html): the per-load Math.random() sample against
// each venue's chance, the rain model removed when NAT06 or SX13 stay dry, one
// rain.vag voice muted by pause and kept through Reset and a lost WebGL
// context, and Caribou race Snow whose clock stops while paused. Math.random
// is pinned per page, as the JavaScript test does. Host browser evidence.
// Content group.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const state = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state').textContent));
const audio = page => page.evaluate(() => JSON.parse(document.documentElement.getAttribute('data-engine-audio') ?? 'null'));
const until = (page, predicate, arg, timeout = 30000) => page.waitForFunction(predicate, arg, { timeout });
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }
const seed = sample => {
  localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
  Math.random = () => sample;
};

const browser = await launch({ args: ['--autoplay-policy=no-user-gesture-required'] });
const results = [];
try {
  for (const [venue, sample, expected, mobile] of [
    ['env04', 0.25, true, true],
    ['nat06', 0.25, true, false],
    ['nat06', 0.99, false, false],
    ['sx13', 0.25, true, false],
    ['sx13', 0.99, false, false],
    ['env01', 0.25, false, false]
  ]) {
    const errors = [];
    const page = await newPage(browser, { viewport: mobile ? { width: 390, height: 844 } : { width: 960, height: 540 } }, errors);
    await page.addInitScript(seed, sample);
    await page.goto(url(`/engine.html?venue=${venue}&uiDiagnostics=1&audioDiagnostics=1`));
    await page.waitForSelector('#ui-state', { state: 'attached', timeout: 120000 });
    await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).step > 2, null, 60000);
    let s = await state(page);
    const weather = s.weather;
    assert.ok(weather, `${venue}: weather selection reported`);
    assert.equal(weather.active, expected, `${venue} at ${sample}: ${JSON.stringify(weather)}`);
    assert.equal(weather.playerCount, 1);
    // Shipped rain names placed geometry; none selects ScreenRain.
    assert.equal(weather.screenRain, false);
    assert.equal(Boolean(weather.removedModel), !expected && ['nat06', 'sx13'].includes(venue), JSON.stringify(weather));
    if (weather.removedModel) assert.equal(weather.modelPlaced, false, 'the dry venue drops its rain model');
    if (expected) {
      assert.equal(weather.kind, 'rain');
      assert.equal(weather.modelPlaced, true, 'the rain model stays in the scene');
      await page.mouse.click(100, 100);
      await until(page, () => JSON.parse(document.documentElement.getAttribute('data-engine-audio') ?? 'null')?.events?.rain === 1, null, 30000);
      // Pause mutes it (the effects bus), Resume brings the same voice back.
      await key(page, 'Escape');
      await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).paused === true);
      await until(page, () => JSON.parse(document.documentElement.getAttribute('data-engine-audio')).active === false);
      await key(page, 'Escape');
      await until(page, () => JSON.parse(document.documentElement.getAttribute('data-engine-audio')).active === true);
      // Reset ATV keeps the loop.
      await key(page, 'KeyR');
      await nextFrames(page, 20);
      assert.equal((await audio(page)).events.rain, 1, 'one rain voice through pause and Reset');
      if (venue === 'env04') {
        await page.evaluate(() => {
          window.__context = document.querySelector('#game').getContext('webgl2').getExtension('WEBGL_lose_context');
          window.__context.loseContext();
        });
        await until(page, () => document.querySelector('#status').textContent === 'Restoring graphics…');
        await page.evaluate(() => window.__context.restoreContext());
        await until(page, () => document.querySelector('#status').textContent !== 'Restoring graphics…');
        await nextFrames(page, 20);
        s = await state(page);
        assert.deepEqual(s.weather, weather, 'the selection survives the restored context');
        assert.equal((await audio(page)).events.rain, 1, 'and so does the rain voice');
      }
    } else {
      await nextFrames(page, 20);
      assert.equal((await audio(page))?.events?.rain ?? 0, 0, `${venue}: no rain loop`);
    }
    assertNoErrors(errors);
    results.push({ venue, sample, active: weather.active, removed: weather.removedModel });
    await page.close();
  }
  // Caribou (ENV01) race Snow: the source ring emitters run, and pause holds
  // the Snow clock.
  {
    const errors = [];
    const page = await newPage(browser, { viewport: { width: 960, height: 540 } }, errors);
    await page.addInitScript(() => localStorage.setItem('atv2-keyboard-help-seen-v1', '1'));
    await page.goto(url('/engine.html?venue=env01&race=1&laps=1&cpu=0&uiDiagnostics=1'));
    await page.waitForSelector('#ui-state', { state: 'attached', timeout: 120000 });
    await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).snow?.time > 3, null, 120000);
    let s = await state(page);
    assert.equal(s.weather.active, true);
    assert.equal(s.weather.kind, 'snow');
    assert.ok(s.snow.activeEmitters > 0 && s.snow.activeEmitters < 60, JSON.stringify(s.snow));
    assert.ok(s.snow.emitted > 100, JSON.stringify(s.snow));
    assert.equal(s.snow.draws, 1, JSON.stringify(s.snow));
    await page.mouse.click(480, 270);
    await key(page, 'Escape');
    await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).paused === true);
    // Time-based on purpose: the paused Snow clock stays still over wall-clock time.
    await page.waitForTimeout(300);
    const paused = (await state(page)).snow;
    await page.waitForTimeout(600);
    s = await state(page);
    assert.equal(s.snow.time, paused.time, 'paused Snow holds its clock');
    assert.equal(s.snow.emitted, paused.emitted);
    await key(page, 'Escape');
    await until(page, time => JSON.parse(document.querySelector('#ui-state').textContent).snow.time > time, paused.time);
    assertNoErrors(errors);
    results.push({ venue: 'env01 race', snow: s.snow });
    await page.close();
  }
  console.log(`PASS: Rust weather per-venue chance, rain model removal, one rain voice through pause/Reset/context loss, Snow: ${JSON.stringify(results)}`);
} finally {
  await browser.close();
}
