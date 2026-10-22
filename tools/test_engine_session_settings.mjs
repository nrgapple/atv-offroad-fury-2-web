// Switch-over gaps 2, 7, 13 and the Fullscreen row (research/SWITCHOVER.md) in
// the Rust engine's browser build, against the JavaScript behaviour:
// - iOS audio session (audio.js applyAudioSession): 'playback' by default so the
//   Ring/Silent switch does not mute the game; Settings → Extras → Silent switch
//   shows only where navigator.audioSession exists and 'mute' selects 'ambient'.
// - Fullscreen (frontend.js): the Display row only where fullscreenEnabled.
// - Audio → Sound (audio.js loadAudioSettings): a played ride uses the saved
//   mode, Enhanced by default; ?sound= and scripted rides keep their choice.
// - Race intro (game.js introSkip): a click off the toolbar or a new pad press
//   skips the pre-race camera, like any key. Host Chromium evidence.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent ?? 'null'));
const wait = (page, predicate, arg, timeout = 120000) => page.waitForFunction(predicate, arg, { timeout });
const onPage = (page, name) => wait(page, name => JSON.parse(document.querySelector('#ui-state').textContent).toolbar?.menu?.page === name, name);
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }
async function click(page, action) {
  const m = (await read(page)).toolbar.menu, hit = m.hits.find(h => h.action === action);
  assert.ok(hit, `${action} on ${m.page}: ${m.choices}`);
  await page.mouse.click(hit.x + hit.width / 2, hit.y + hit.height / 2); await nextFrames(page, 3);
}
const choices = async page => (await read(page)).toolbar.menu.choices;
const ready = page => wait(page, () => JSON.parse(document.querySelector('#ui-state')?.textContent ?? 'null')?.settings?.soundMode != null);

const browser = await launch();
try {
  // iOS-like page: an audio session, no Fullscreen API, nothing saved.
  {
    const errors = [];
    const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
    await page.addInitScript(() => {
      localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
      const writes = window.audioSessionWrites = [];
      const session = { _type: 'auto', get type() { return this._type; }, set type(v) { this._type = v; writes.push(v); } };
      Object.defineProperty(Navigator.prototype, 'audioSession', { get: () => session, configurable: true });
      Object.defineProperty(Document.prototype, 'fullscreenEnabled', { get: () => false, configurable: true });
    });
    await page.goto(url('/engine.html?venue=env01&uiDiagnostics=1'));
    await ready(page);
    assert.equal(await page.evaluate(() => navigator.audioSession.type), 'playback');
    assert.equal((await read(page)).settings.soundMode, 'Enhanced', 'saved-less ride plays Enhanced');
    await page.mouse.click(640, 400);
    await key(page, 'Escape'); await onPage(page, 'pause');
    await click(page, 'settings'); await onPage(page, 'settings');
    await click(page, 'hud-settings'); await onPage(page, 'hud-settings');
    assert.ok(!(await choices(page)).includes('fullscreen'), 'no Fullscreen row without the API');
    await key(page, 'Escape'); await onPage(page, 'settings');
    await click(page, 'extras'); await onPage(page, 'extras');
    assert.ok((await choices(page)).includes('silentmode'), 'Silent switch row with an audio session');
    await click(page, 'silentmode');
    assert.equal(await page.evaluate(() => navigator.audioSession.type), 'ambient');
    assert.equal(JSON.parse(await page.evaluate(() => localStorage.getItem('atv-audio-v1'))).silent, 'mute');
    await click(page, 'silentmode');
    assert.equal(await page.evaluate(() => navigator.audioSession.type), 'playback');
    // Written only on change: auto → playback → ambient → playback.
    assert.deepEqual(await page.evaluate(() => window.audioSessionWrites), ['playback', 'ambient', 'playback']);
    assertNoErrors(errors);
    console.log('  iOS-like: playback by default, Silent switch row toggles ambient/playback, no Fullscreen row, Enhanced');
    await page.close();
  }
  // Desktop browser: no session row, Fullscreen row, saved Original, ?sound= wins.
  {
    const errors = [];
    const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
    await page.addInitScript(() => {
      localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
      localStorage.setItem('atv-audio-v1', JSON.stringify({ sfx: 1, mode: 'original', silent: 'play' }));
    });
    await page.goto(url('/engine.html?venue=env01&uiDiagnostics=1'));
    await ready(page);
    assert.equal((await read(page)).settings.soundMode, 'Original', 'saved Original');
    await page.mouse.click(640, 400);
    await key(page, 'Escape'); await onPage(page, 'pause');
    await click(page, 'settings'); await onPage(page, 'settings');
    await click(page, 'hud-settings'); await onPage(page, 'hud-settings');
    assert.ok((await choices(page)).includes('fullscreen'));
    await key(page, 'Escape'); await onPage(page, 'settings');
    await click(page, 'extras'); await onPage(page, 'extras');
    assert.ok(!(await choices(page)).includes('silentmode'), 'no Silent switch row without a session');
    await page.goto(url('/engine.html?venue=env01&uiDiagnostics=1&sound=enhanced'));
    await ready(page);
    assert.equal((await read(page)).settings.soundMode, 'Enhanced', '?sound= wins over the saved mode');
    assertNoErrors(errors);
    console.log('  desktop: Fullscreen row, no Silent switch row, saved Original, ?sound=enhanced wins');
    await page.close();
  }
  // Race intro: a click, then (in a second race) a new pad press, skips it.
  for (const how of ['click', 'pad']) {
    const errors = [];
    const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
    await page.addInitScript(how => {
      localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
      if (how !== 'pad') return;
      const buttons = Array.from({ length: 17 }, () => ({ pressed: false, touched: false, value: 0 }));
      const pad = { id: 'Intro skip stand-in (STANDARD GAMEPAD)', index: 0, connected: true, mapping: 'standard', timestamp: 1, axes: [0, 0, 0, 0], buttons };
      window.pressPad = down => { buttons[0] = { pressed: down, touched: down, value: down ? 1 : 0 }; pad.timestamp++; };
      navigator.getGamepads = () => [pad];
    }, how);
    await page.goto(url('/engine.html?venue=env01&race=1&laps=1&cpu=3&weather=off&uiDiagnostics=1&debug=1'));
    await wait(page, () => /race intro/.test(document.querySelector('#status')?.textContent), null, 180000);
    if (how === 'click') await page.mouse.click(640, 400);
    else { await page.evaluate(() => window.pressPad(true)); await nextFrames(page, 4); await page.evaluate(() => window.pressPad(false)); }
    await wait(page, () => !/race intro/.test(document.querySelector('#status')?.textContent), null, 15000);
    assertNoErrors(errors);
    console.log(`  race intro skipped by ${how}`);
    await page.close();
  }
  console.log('PASS: Rust audio session, Fullscreen row, saved sound mode and race intro skip');
} finally {
  await browser.close();
}
