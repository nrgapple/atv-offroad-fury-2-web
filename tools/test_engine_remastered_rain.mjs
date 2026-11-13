// Remastered rain (authored; research/WEATHER.md): on a venue whose Weather
// selected rain, the Remastered preset alone builds the rain pass (streaks,
// splash rings, lens beads). Original, Enhanced and High never do, at load
// or after a preset change mid-ride, and a Remastered load that stayed dry
// (NAT06; Freeport's chance is 1, it always rains) has none.
// The rain runs on the ride's clock: it holds while paused, takes no lens
// beads under a menu, in photo mode or in a replay, and falls again while a
// replay plays. Host browser evidence. Content group.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent ?? 'null'));
const wait = (page, predicate, arg, timeout = 60000) => page.waitForFunction(predicate, arg, { timeout });
const onPage = (page, name) => wait(page, name => JSON.parse(document.querySelector('#ui-state').textContent).toolbar?.menu?.page === name, name);
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }
async function click(page, action) {
  const m = (await read(page)).toolbar.menu, hit = m.hits.find(h => h.action === action);
  assert.ok(hit, `${action} on ${m.page}: ${m.choices}`);
  await page.mouse.click(hit.x + hit.width / 2, hit.y + hit.height / 2); await nextFrames(page, 3);
}
async function ride(browser, errors, query, venue = 'env04') {
  const page = await newPage(browser, { viewport: { width: 960, height: 540 } }, errors);
  await page.addInitScript(() => localStorage.setItem('atv2-keyboard-help-seen-v1', '1'));
  await page.goto(url(`/play.html?venue=${venue}&uiDiagnostics=1&${query}`));
  await wait(page, () => JSON.parse(document.querySelector('#ui-state')?.textContent ?? 'null')?.step > 30, null, 300000);
  return page;
}

const browser = await launch();
const report = {};
try {
  const errors = [];
  // The venue rains in every preset; only Remastered draws the authored rain.
  for (const graphics of ['original', 'enhanced', 'high']) {
    const page = await ride(browser, errors, `weather=on&graphics=${graphics}`);
    const s = await read(page);
    assert.deepEqual([s.weather.active, s.weather.kind], [true, 'rain'], `${graphics}: ${JSON.stringify(s.weather)}`);
    assert.equal(s.settings.remaster, false);
    assert.equal(s.rain, null, `${graphics} builds no rain pass`);
    report[graphics] = s.rain;
    await page.close();
  }
  {
    const page = await ride(browser, errors, 'weather=off&graphics=remastered', 'nat06');
    const s = await read(page);
    assert.deepEqual([s.settings.remaster, s.weather.active], [true, false], JSON.stringify(s.weather));
    assert.equal(s.rain, null, 'a dry Remastered load has no rain');
    await page.close();
  }
  const page = await ride(browser, errors, 'weather=on&graphics=remastered');
  await wait(page, () => { const r = JSON.parse(document.querySelector('#ui-state').textContent).rain; return r && r.clock > 1 && r.beads > 0 && r.splashes > 0; });
  let s = await read(page);
  assert.equal(s.rain.streaks, 3200);
  report.remastered = s.rain;
  if (process.env.ATV_RAIN_SCREENSHOT) await page.screenshot({ path: process.env.ATV_RAIN_SCREENSHOT });
  // Ride a few seconds so there is a stretch to replay; the rain keeps falling.
  await page.mouse.click(480, 300);
  await page.keyboard.down('KeyW');
  await wait(page, step => JSON.parse(document.querySelector('#ui-state').textContent).step > step + 300, s.step);
  await page.keyboard.up('KeyW');
  assert.ok((await read(page)).rain.clock > s.rain.clock + 4, 'the rain clock follows the ride');
  // Paused: the clock holds and the menu has no beads over it.
  await key(page, 'Escape'); await onPage(page, 'pause');
  await nextFrames(page, 3);
  const paused = (await read(page)).rain;
  assert.equal(paused.beads, 0, 'no lens beads under the pause menu');
  // Time-based on purpose: the paused rain stays still over wall-clock time.
  await page.waitForTimeout(500);
  assert.equal((await read(page)).rain.clock, paused.clock, 'paused rain holds its clock');
  // Photo mode: still no beads, and the rain stays held.
  await click(page, 'photo');
  await wait(page, () => { const s = JSON.parse(document.querySelector('#ui-state').textContent); return s.photo && !s.toolbar.menu; });
  await nextFrames(page, 3);
  s = await read(page);
  assert.deepEqual([s.rain.beads, s.rain.clock], [0, paused.clock], 'photo mode: clean lens, held rain');
  await key(page, 'Escape'); await onPage(page, 'pause');
  // A replay plays over the held ride: its rain falls, on no lens.
  await click(page, 'replay');
  await wait(page, () => { const s = JSON.parse(document.querySelector('#ui-state').textContent); return s.replay && !s.toolbar.menu; });
  await wait(page, clock => { const s = JSON.parse(document.querySelector('#ui-state').textContent); return s.replay.playing && s.rain.clock > clock + 0.5; }, paused.clock);
  assert.equal((await read(page)).rain.beads, 0, 'no lens beads in a replay');
  await key(page, 'Escape'); await onPage(page, 'pause');
  // Leaving Remastered mid-ride drops the pass; coming back builds it again.
  await click(page, 'settings'); await onPage(page, 'settings');
  await click(page, 'hud-settings'); await onPage(page, 'hud-settings');
  for (const preset of ['original', 'enhanced', 'high']) {
    await click(page, 'graphics');
    assert.equal(await page.evaluate(() => localStorage.getItem('atv-graphics')), preset);
    await nextFrames(page, 3);
    assert.equal((await read(page)).rain, null, `${preset} mid-ride has no rain pass`);
  }
  await click(page, 'graphics');
  await wait(page, () => { const s = JSON.parse(document.querySelector('#ui-state').textContent); return s.settings.remaster && s.rain; });
  await key(page, 'Escape'); await key(page, 'Escape');
  await click(page, 'resume');
  await wait(page, () => { const r = JSON.parse(document.querySelector('#ui-state').textContent).rain; return r && r.beads > 0; });
  assertNoErrors(errors);
  console.log(`PASS: Remastered rain only under Remastered, on the ride's clock, no beads in menus, photo mode or replays: ${JSON.stringify(report)}`);
} finally {
  await browser.close();
}
