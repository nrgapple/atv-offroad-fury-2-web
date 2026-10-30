// The original Lap Attack event type (research/RACING.md "Lap Attack event
// type"): Single Player → Race → Event steps left from Single Race to Lap
// Attack (`0026d010`); Laps and CPU riders read "N/A" (`002f5e60`,
// `0021d9f0`). The ride has no CPUs and no finish (race over `001f1990` runs
// only for Single Race and Duel). Each completed lap is timed; the fastest
// becomes the best lap and the ghost, recorded from the player's fixed-step
// motion, replays it from the next lap's start (`001f6c60`, `001f6640`).
// The race path driver (`autopilot=1`) rides two Supercross laps.
// ATV_SHOTS=<dir> saves the setup page and a lap with the ghost playing.
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
    if (Date.now() > deadline) throw new Error(`timed out: ${what} ${JSON.stringify({ lap: s.ride?.raceLap ?? null, ghost: s.ride?.ghost ?? null })}`);
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
  // Single player: Single Race → Practice → Lap Attack, wrapping left.
  await pick(page, 'raceevent');
  await key(page, 'ArrowLeft');
  f = await front(page);
  assert.equal(f.race.event, 'lapattack');
  assert.equal(label(f, 'raceevent'), 'Event: Lap Attack');
  assert.equal(label(f, 'racelaps'), 'Laps: N/A');
  assert.equal(label(f, 'racecpu'), 'CPU riders: N/A');
  // Supercross: the shortest laps.
  await key(page, 'PageUp');
  f = await front(page);
  assert.ok(f.race.course.startsWith('sx'), f.race.course);
  assert.equal(f.race.event, 'lapattack');
  if (shots) await page.screenshot({ path: `${shots}/race-lap-attack-setup.png` });
  await pick(page, 'racestart'); await key(page, 'Enter');
  let s = await until(page, s => s.ride?.lapAttack === true && s.ride?.raceLap?.phase === 'racing', 240000, 'lap attack racing');
  assert.equal(s.ride.raceLap.laps, 0, 'Lap Attack has no lap count');
  assert.equal(s.ride.raceLap.field, 1, 'Lap Attack races no CPUs');
  assert.equal(s.ride.ghost, null, 'no ghost before a lap');
  // Supercross's first lap from the grid is not judged for the best lap
  // (Track flag 8, `00248b20`), so lap 2 sets the best lap and the ghost,
  // which replays it from lap 3's start.
  s = await until(page, s => (s.ride?.raceLap?.laps_done ?? 0) >= 1, 300000, 'first lap');
  s = await until(page, s => (s.ride?.raceLap?.laps_done ?? 0) >= 2, 300000, 'second lap');
  s = await until(page, s => s.ride?.ghost?.playing === true, 30000, 'ghost playing');
  const ghost = s.ride.ghost;
  assert.ok(ghost.samples > 60, `ghost samples ${ghost.samples}`);
  assert.ok(Math.abs(ghost.best - s.ride.bestLap) < 1e-3, `ghost best ${ghost.best} vs best lap ${s.ride.bestLap}`);
  assert.ok(Math.abs(ghost.opacity - 0.4) < 1e-6, 'GhostOpacity 0.4');
  assert.equal(ghost.origin.length, 3);
  assert.equal(s.ride.raceLap.phase, 'racing', 'Lap Attack does not finish');
  assert.equal(s.ride.raceLap.lap, s.ride.raceLap.laps_done + 1);
  await nextFrames(page, 60);
  s = await read(page);
  assert.equal(s.ride.ghost.playing, true);
  assert.notDeepEqual(s.ride.ghost.origin, ghost.origin, 'the ghost moves along its lap');
  if (shots) await page.screenshot({ path: `${shots}/race-lap-attack-ghost.png` });
  // Pause → Quit to Main Menu ends the run.
  await key(page, 'Escape');
  await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').toolbar?.menu?.page === 'pause', null, { timeout: 30000 });
  const menu = (await read(page)).toolbar.menu;
  const quit = menu.choices.indexOf('main');
  assert.ok(quit > 0, `Quit to Main Menu in ${menu.choices}`);
  for (let i = 0; i < quit; i++) await key(page, 'ArrowDown');
  await key(page, 'Enter');
  await onPage(page, 'main', 60000);
  assertNoErrors(errors);
  console.log(`PASS: Lap Attack on ${f.race.course}: no CPUs, ${s.ride.raceLap.laps_done} laps, best ${s.ride.bestLap.toFixed(2)}s, ghost ${s.ride.ghost.samples} samples at opacity ${s.ride.ghost.opacity.toFixed(2)}`);
} finally {
  await browser.close();
}
