// Saved ride settings in the Rust engine: the choices game.js applies while
// riding (Speed rush, crashes, flips, train riding, audio volumes) take effect
// at load and when changed in the pause menu; a ride started from the engine's
// frontend uses the saved Display → Graphics preset, and a preset changed
// mid-ride applies its live parts (interpolation, smoothed sky, Fresnel,
// contact shadows, dust and tire-track pools). Host browser evidence.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent ?? 'null'));
const settings = async page => (await read(page))?.settings;
const wait = (page, predicate, arg) => page.waitForFunction(predicate, arg, { timeout: 120000 });
const onPage = (page, name) => wait(page, name => JSON.parse(document.querySelector('#ui-state').textContent).toolbar?.menu?.page === name, name);
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }
async function click(page, action) {
  const m = (await read(page)).toolbar.menu, hit = m.hits.find(h => h.action === action);
  assert.ok(hit, `${action} on ${m.page}: ${m.choices}`);
  await page.mouse.click(hit.x + hit.width / 2, hit.y + hit.height / 2); await nextFrames(page, 3);
}

const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await page.addInitScript(() => {
    if (sessionStorage.getItem('seeded')) return;
    sessionStorage.setItem('seeded', '1');
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.setItem('atv-speed-rush', 'high');
    localStorage.setItem('atv-crash-mode', 'gorn');
    localStorage.setItem('atv-flips', 'unleashed');
    localStorage.setItem('atv-train-riding', 'realistic');
    localStorage.setItem('atv-audio-v1', JSON.stringify({ sfx: 0.5, mode: 'original', silent: 'play' }));
    localStorage.setItem('atv-soundtrack-v1', JSON.stringify({ v: 3, enabled: true, volume: 0.3 }));
  });
  await page.goto(url('/play.html?venue=env01&uiDiagnostics=1'));
  await wait(page, () => JSON.parse(document.querySelector('#ui-state')?.textContent ?? 'null')?.settings?.sfx != null);
  let s = await settings(page);
  assert.equal(s.speedRushPeak, 1); assert.equal(s.crashMode, 'gorn');
  assert.equal(s.flips, true); assert.equal(s.realisticTrain, true);
  assert.equal(s.sfx, 0.5); assert.ok(Math.abs(s.music - 0.3) < 1e-6);
  // A direct ?venue= ride keeps its URL/Original graphics until changed.
  assert.equal(s.interpolation, false); assert.equal(s.contactShadows, false);
  // Pause → Settings → Display: Speed rush and the Graphics preset.
  await page.mouse.click(640, 400);
  await key(page, 'Escape'); await onPage(page, 'pause');
  await click(page, 'settings'); await onPage(page, 'settings');
  await click(page, 'hud-settings'); await onPage(page, 'hud-settings');
  await click(page, 'speed-rush'); // high → off
  assert.equal((await settings(page)).speedRushPeak, 0);
  await click(page, 'graphics'); // auto → original
  await click(page, 'graphics'); // original → enhanced
  assert.equal(await page.evaluate(() => localStorage.getItem('atv-graphics')), 'enhanced');
  s = await settings(page);
  assert.deepEqual([s.interpolation, s.skySmooth, s.fresnel, s.contactShadows], [true, true, true, true]);
  // Anisotropy and the shadow map follow the preset mid-ride (applyAnisotropy,
  // the shadow map resized or dropped), as the browser does.
  assert.deepEqual([s.anisotropy, s.shadowMap], [4, 0]);
  await click(page, 'graphics'); // enhanced → high
  s = await settings(page);
  assert.deepEqual([s.anisotropy, s.shadowMap], [16, 2048]);
  for (let i = 0; i < 3; i++) await click(page, 'graphics'); // high → auto → original → enhanced
  s = await settings(page);
  assert.deepEqual([s.anisotropy, s.shadowMap], [4, 0]);
  await key(page, 'Escape'); await key(page, 'Escape');
  await click(page, 'resume');
  await wait(page, () => JSON.parse(document.querySelector('#ui-state').textContent).settings.dust === 24);
  // Extras → Crashes back to Original drops the crash layer at once.
  await key(page, 'Escape'); await onPage(page, 'pause');
  await click(page, 'settings'); await click(page, 'extras'); await onPage(page, 'extras');
  await click(page, 'funnycrashes'); // gorn → original
  assert.equal(await page.evaluate(() => localStorage.getItem('atv-crash-mode')), 'original');
  assert.equal((await settings(page)).crashMode, null);
  assertNoErrors(errors);
  // A ride from the engine's frontend (a ride link) loads the saved preset.
  const link = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await link.addInitScript(() => { localStorage.setItem('atv-graphics', 'high'); localStorage.setItem('atv2-keyboard-help-seen-v1', '1'); });
  await link.goto(url('/play.html?map=env01&play=1&uiDiagnostics=1'));
  await wait(link, () => JSON.parse(document.querySelector('#ui-state')?.textContent ?? 'null')?.settings != null);
  s = await settings(link);
  assert.deepEqual([s.interpolation, s.anisotropy, s.dust, s.tireTracks, s.contactShadows], [true, 16, 96, 3600, true]);
  assertNoErrors(errors);
  console.log('PASS: Rust ride applies saved Speed rush, crashes, flips, train, audio volumes and graphics');
} finally {
  await browser.close();
}
