// The original Practice event type (research/RACING.md "Practice"): Single
// Player → Race → Event cycles Single Race, Practice and Lap Attack (`0026d010`); Practice
// shows Laps "N/A" (`002f5e60`) and its ride laps on with the CPU field and
// no finish (race over `001f1990` runs only for Single Race and Duel): a
// completed lap leaves the race running with the lap counter past it. Pause
// keeps Restart Race and Quit to Main Menu leaves the run. The race path
// driver (`autopilot=1`) rides the player around a Supercross lap.
// ATV_SHOTS=<dir> saves the setup page.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, unlockStore, url } from './browser/harness.mjs';

const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const front = async page => (await read(page)).front;
const onPage = (page, name, timeout = 60000) =>
  page.waitForFunction(name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name, name, { timeout });
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }
async function pick(page, action) {
  for (let i = 0; i < 12 && (await front(page)).choices[(await front(page)).selection] !== action; i++) await key(page, 'ArrowDown');
  const f = await front(page);
  assert.equal(f.choices[f.selection], action);
}
const label = (f, action) => f.labels[f.choices.indexOf(action)];
async function until(page, predicate, timeout, what) {
  const deadline = Date.now() + timeout;
  for (;;) {
    const s = await read(page);
    if (predicate(s)) return s;
    if (Date.now() > deadline) throw new Error(`timed out: ${what} ${JSON.stringify(s.ride?.raceLap ?? null)}`);
    await page.waitForTimeout(250);
  }
}
const shots = process.env.ATV_SHOTS;

const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await unlockStore(page);
  await page.addInitScript(() => {
    if (sessionStorage.getItem('seeded')) return;
    sessionStorage.setItem('seeded', '1');
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.removeItem('atv-player-selection');
  });
  await page.goto(url('/play.html?uiDiagnostics=1&autopilot=1&weather=off'));
  await onPage(page, 'title');
  await page.mouse.click(8, 8);
  await key(page, 'Enter'); await onPage(page, 'main');
  await key(page, 'Enter'); await onPage(page, 'single');
  await pick(page, 'race'); await key(page, 'Enter'); await onPage(page, 'race');
  let f = await front(page);
  assert.equal(label(f, 'raceevent'), 'Event: Single Race');
  // The type cycler: Single Race → Practice → Lap Attack (Duel is split screen only).
  await pick(page, 'raceevent');
  await key(page, 'ArrowRight');
  f = await front(page);
  assert.equal(f.race.event, 'practice');
  assert.equal(label(f, 'raceevent'), 'Event: Practice');
  assert.equal(label(f, 'racelaps'), 'Laps: N/A');
  await key(page, 'ArrowRight');
  assert.equal((await front(page)).race.event, 'lapattack');
  await key(page, 'ArrowLeft');
  // Supercross: the shortest laps.
  await key(page, 'PageUp');
  f = await front(page);
  assert.ok(f.race.course.startsWith('sx'), f.race.course);
  assert.equal(f.race.event, 'practice');
  if (shots) await page.screenshot({ path: `${shots}/race-practice-setup.png` });
  await pick(page, 'racestart'); await key(page, 'Enter');
  let s = await until(page, s => s.ride?.practice === true && s.ride?.raceLap?.phase === 'racing', 240000, 'practice racing');
  assert.equal(s.ride.raceLap.laps, 0, 'Practice has no lap count');
  assert.ok(s.ride.raceLap.field > 1, `CPU field: ${s.ride.raceLap.field}`);
  // A lap completes and the run goes on: no finish, no results.
  s = await until(page, s => (s.ride?.raceLap?.laps_done ?? 0) >= 1, 240000, 'first practice lap');
  await nextFrames(page, 30);
  s = await read(page);
  assert.equal(s.ride.raceLap.phase, 'racing', 'Practice does not finish');
  assert.equal(s.ride.raceLap.lap, s.ride.raceLap.laps_done + 1);
  // Pause → Quit to Main Menu ends the run.
  await key(page, 'Escape');
  await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').toolbar?.menu?.page === 'pause', null, { timeout: 30000 });
  const menu = (await read(page)).toolbar.menu;
  assert.equal(menu.choices[1], 'reset');
  const quit = menu.choices.indexOf('main');
  assert.ok(quit > 0, `Quit to Main Menu in ${menu.choices}`);
  for (let i = 0; i < quit; i++) await key(page, 'ArrowDown');
  await key(page, 'Enter');
  await onPage(page, 'main', 60000);
  assertNoErrors(errors);
  console.log(`PASS: Practice on ${f.race.course}: Laps N/A, ${s.ride.raceLap.laps_done} lap(s) at ${s.ride.raceLap.elapsed.toFixed(1)}s with ${s.ride.raceLap.field - 1} CPUs and no finish, Pause → Quit to Main Menu`);
} finally {
  await browser.close();
}
