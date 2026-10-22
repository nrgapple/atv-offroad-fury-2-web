// WebGL context loss on the Rust engine page (game.js webglcontextlost /
// webglcontextrestored): WEBGL_lose_context drops the context mid-ride and
// restores it. The engine shows "Restoring graphics…", holds the ride (no
// steps while lost), then rebuilds its GPU resources in place: the page does
// not reload, the ride keeps its position and step count and drives on, the
// scene, HUD and map draw again. Also over the pause menu, during a race with
// CPU riders, and on the frontend's main menu with its 3D preview.
// Run: node tools/run_browser_tests.mjs --only test_engine_context_loss.mjs (content group).
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const wait = (page, predicate, arg, timeout = 60000) => page.waitForFunction(predicate, arg, { timeout });
const status = page => page.evaluate(() => { const s = document.querySelector('#status'); return { text: s.textContent, fatal: s.classList.contains('fatal') }; });
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }

/** Luminance spread and lit share of the page as drawn (a lost context draws nothing). */
async function picture(page) {
  const png = (await page.screenshot()).toString('base64');
  // Decoded on a blank page: the game page's CSP has no data: images.
  const blank = await page.context().browser().newPage();
  try {
    return await blank.evaluate(async src => {
    const image = new Image(); image.src = `data:image/png;base64,${src}`; await image.decode();
    const canvas = new OffscreenCanvas(image.width, image.height), context = canvas.getContext('2d');
    context.drawImage(image, 0, 0);
    const { data } = context.getImageData(0, 0, image.width, image.height);
    let sum = 0, square = 0, lit = 0, n = 0;
    for (let i = 0; i < data.length; i += 16) {
      const y = 0.299 * data[i] + 0.587 * data[i + 1] + 0.114 * data[i + 2];
      sum += y; square += y * y; n++; if (y > 24) lit++;
    }
    const mean = sum / n;
    return { mean, spread: Math.sqrt(square / n - mean * mean), lit: lit / n };
    }, png);
  } finally {
    await blank.close();
  }
}

/** Loses the context, checks the held state, restores it and waits for the rebuild. */
async function loseAndRestore(page, check) {
  await page.evaluate(() => {
    window.__context = document.querySelector('#game').getContext('webgl2').getExtension('WEBGL_lose_context');
    window.__context.loseContext();
  });
  await wait(page, () => document.querySelector('#status').textContent === 'Restoring graphics…');
  assert.ok((await status(page)).fatal, 'the restoring line shows');
  await check?.();
  await page.evaluate(() => window.__context.restoreContext());
  await wait(page, () => document.querySelector('#status').textContent !== 'Restoring graphics…');
  assert.equal((await status(page)).fatal, false, 'the line hides again');
  await nextFrames(page, 20);
}

const assertDrawn = (shot, label) => {
  assert.ok(shot.spread > 12 && shot.lit > 0.3, `${label}: the scene draws again ${JSON.stringify(shot)}`);
};

const browser = await launch();
const errors = [];
try {
  // 1. A single-player ride: held while lost, then on from the same place.
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await page.addInitScript(() => localStorage.setItem('atv2-keyboard-help-seen-v1', '1'));
  await page.goto(url('/engine.html?venue=env01&uiDiagnostics=1'));
  await wait(page, () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').toolbar, null, 120000);
  await nextFrames(page, 30);
  await page.mouse.click(640, 400);
  await page.keyboard.down('KeyW'); await page.waitForTimeout(2500); await page.keyboard.up('KeyW');
  assertDrawn(await picture(page), 'before');
  await page.evaluate(() => { window.__samePage = true; });
  const before = await read(page);
  const start = before.origin;
  assert.ok(before.step > 60, `the ride ran (${before.step} steps)`);
  let held;
  await loseAndRestore(page, async () => {
    held = await read(page);
    await page.waitForTimeout(600);
    const later = await read(page);
    assert.equal(later.step, held.step, 'no steps while the context is lost');
    assert.ok(held.step - before.step < 30, `held where it was lost (${before.step} → ${held.step})`);
  });
  assert.equal(await page.evaluate(() => window.__samePage), true, 'rebuilt in place (no reload)');
  let after = await read(page);
  assert.ok(after.step >= held.step && after.step - held.step < 300, `the step count carries on (${held.step} → ${after.step})`);
  const moved = Math.hypot(after.origin[0] - held.origin[0], after.origin[2] - held.origin[2]);
  assert.ok(moved < 30, `the ride stays where it was (${moved.toFixed(1)} ft)`);
  assertDrawn(await picture(page), 'after');
  assert.ok(after.toolbar?.hits?.length >= 2, 'the toolbar is laid out again');
  await page.keyboard.down('KeyW'); await page.waitForTimeout(1500); await page.keyboard.up('KeyW');
  const driven = await read(page);
  assert.ok(driven.step > after.step + 30, 'the ride drives on');
  assert.notDeepEqual(driven.origin, start);

  // 2. Lost under the pause menu: the menu returns, and Resume rides on.
  await key(page, 'Escape');
  await wait(page, () => JSON.parse(document.querySelector('#ui-state').textContent).toolbar?.menu?.page === 'pause');
  await loseAndRestore(page);
  assert.equal(await page.evaluate(() => window.__samePage), true);
  after = await read(page);
  assert.equal(after.toolbar.menu?.page, 'pause', 'the pause menu is still open');
  assert.equal(after.paused, true);
  assertDrawn(await picture(page), 'pause menu');
  await key(page, 'Escape');
  await wait(page, () => JSON.parse(document.querySelector('#ui-state').textContent).paused === false);
  await page.close();

  // 3. A race with CPU riders: the race clock and roster carry on.
  const race = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await race.addInitScript(() => localStorage.setItem('atv2-keyboard-help-seen-v1', '1'));
  await race.goto(url('/engine.html?venue=env01&race=1&laps=2&cpu=3&uiDiagnostics=1'));
  await wait(race, () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').toolbar, null, 120000);
  await race.mouse.click(640, 400);
  await race.keyboard.down('KeyW');
  await wait(race, () => { const s = JSON.parse(document.querySelector('#ui-state').textContent); return s.race?.phase === 'racing' || s.phase === 'racing'; }, null, 60000).catch(() => {});
  await race.waitForTimeout(1500);
  await race.keyboard.up('KeyW');
  await race.evaluate(() => { window.__samePage = true; });
  const raceBefore = await read(race);
  await loseAndRestore(race);
  const raceAfter = await read(race);
  assert.equal(await race.evaluate(() => window.__samePage), true);
  assert.ok(raceAfter.step >= raceBefore.step, `race steps carry on (${raceBefore.step} → ${raceAfter.step})`);
  assert.deepEqual(raceAfter.race?.phase, raceBefore.race?.phase, 'same race phase');
  assertDrawn(await picture(race), 'race');
  await race.close();

  // 4. The frontend: the main menu and its 3D preview come back.
  const front = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await front.goto(url('/engine.html?uiDiagnostics=1'));
  await wait(front, () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === 'title', null, 120000);
  await front.mouse.click(8, 8);
  await key(front, 'Enter');
  await wait(front, () => JSON.parse(document.querySelector('#ui-state').textContent).front?.models?.[0], null, 120000);
  await front.evaluate(() => { window.__samePage = true; });
  await loseAndRestore(front);
  await wait(front, () => JSON.parse(document.querySelector('#ui-state').textContent).front?.models?.[0], null, 120000);
  assert.equal(await front.evaluate(() => window.__samePage), true);
  assert.equal((await read(front)).front.page, 'main');
  assertDrawn(await picture(front), 'main menu');
  await key(front, 'ArrowDown');
  assert.equal((await read(front)).front.selection, 1, 'the menu takes input again');
  await front.close();

  assertNoErrors(errors);
  console.log('PASS Rust WebGL context loss: rebuilt in place (ride, pause menu, race, main menu)');
} finally {
  await browser.close();
}
