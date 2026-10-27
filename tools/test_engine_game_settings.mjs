// Settings → Game Settings (the original Settings_Game page): Measurement
// switches the speedometer to Kph, Ecosystem Collision asks before it turns
// off and then stops the quad's vegetation queries, and Sound Configuration
// reaches the mixer. Host browser evidence.
import assert from 'node:assert/strict';
import fs from 'node:fs';
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
const labels = async page => { const m = (await read(page)).toolbar.menu; return m.choices.map(c => m.labels[c]); };
const shots = process.env.ATV_SCREENSHOTS;

const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await page.addInitScript(() => localStorage.setItem('atv2-keyboard-help-seen-v1', '1'));
  await page.goto(url('/play.html?venue=env01&uiDiagnostics=1'));
  await wait(page, () => JSON.parse(document.querySelector('#ui-state')?.textContent ?? 'null')?.settings?.sfx != null);
  let s = await settings(page);
  // The original defaults: Standard (U.S. system), collision on, Stereo.
  assert.equal(s.speedUnits, 'Mph'); assert.equal(s.vegetationQuery, true);
  assert.equal(s.ecosystemCollisionOff, false); assert.equal(s.soundConfiguration, 'Stereo');

  await page.mouse.click(640, 400);
  await key(page, 'Escape'); await onPage(page, 'pause');
  await click(page, 'settings'); await onPage(page, 'settings');
  await click(page, 'game-settings'); await onPage(page, 'game-settings');
  assert.deepEqual(await labels(page), ['Measurement: Standard (U.S. system)', 'Ecosystem Collision: On', 'Sound Configuration: Stereo', 'Back']);
  await click(page, 'measurement');
  assert.equal(await page.evaluate(() => localStorage.getItem('atv-measurement')), 'metric');
  for (let i = 0; i < 3; i++) await click(page, 'sound-configuration'); // stereo → swapped → dolby → mono
  assert.equal(await page.evaluate(() => localStorage.getItem('atv-sound-configuration')), 'mono');
  // Turning Ecosystem Collision off shows the original Warning; No keeps it.
  await click(page, 'ecosystem'); await onPage(page, 'ecosystem-warning');
  if (shots) await page.screenshot({ path: `${shots}/game-settings-warning.png` });
  await click(page, 'ecosystem-no'); await onPage(page, 'game-settings');
  assert.equal(await page.evaluate(() => localStorage.getItem('atv-ecosystem-collision')), null);
  await click(page, 'ecosystem'); await onPage(page, 'ecosystem-warning');
  await click(page, 'ecosystem-yes'); await onPage(page, 'game-settings');
  assert.equal(await page.evaluate(() => localStorage.getItem('atv-ecosystem-collision')), 'off');
  assert.deepEqual(await labels(page), ['Measurement: Metric', 'Ecosystem Collision: Off', 'Sound Configuration: Mono', 'Back']);
  if (shots) await page.screenshot({ path: `${shots}/game-settings-page.png` });
  // Back out and ride: the speedometer reads Kph, the quad skips vegetation
  // and the mixer folds to mono.
  await key(page, 'Escape'); await onPage(page, 'settings');
  await key(page, 'Escape'); await onPage(page, 'pause');
  await click(page, 'resume');
  await wait(page, () => JSON.parse(document.querySelector('#ui-state').textContent).settings.speedUnits === 'Kph');
  s = await settings(page);
  assert.equal(s.vegetationQuery, false); assert.equal(s.ecosystemCollisionOff, true);
  assert.equal(s.soundConfiguration, 'Mono');
  await page.keyboard.down('ArrowUp'); await nextFrames(page, 90); await page.keyboard.up('ArrowUp');
  if (shots) await page.screenshot({ path: `${shots}/game-settings-kph.png` });
  // Reloading keeps every choice.
  await page.reload();
  await wait(page, () => JSON.parse(document.querySelector('#ui-state')?.textContent ?? 'null')?.settings?.speedUnits === 'Kph');
  s = await settings(page);
  // The mixer starts with the first gesture (below).
  assert.equal(s.vegetationQuery, false);
  // Turning it back on does not ask.
  await page.mouse.click(640, 400);
  await key(page, 'Escape'); await onPage(page, 'pause');
  await click(page, 'settings'); await click(page, 'game-settings'); await onPage(page, 'game-settings');
  await click(page, 'ecosystem'); await onPage(page, 'game-settings');
  assert.equal(await page.evaluate(() => localStorage.getItem('atv-ecosystem-collision')), 'on');
  await key(page, 'Escape'); await key(page, 'Escape'); await click(page, 'resume');
  await wait(page, () => JSON.parse(document.querySelector('#ui-state').textContent).settings.vegetationQuery === true);
  assert.equal((await settings(page)).soundConfiguration, 'Mono');
  assertNoErrors(errors);
  if (shots) assert.ok(fs.existsSync(`${shots}/game-settings-page.png`));
  console.log('game settings: Measurement, Ecosystem Collision warning and Sound Configuration verified');
} finally {
  await browser.close();
}
