// The original Duel event type (research/RACING.md "Duel event type") in
// /play.html: Main Menu → Multiplayer → Split Screen → Race → Event cycles to
// Duel (`0026d010` offers it only with the multiplayer flag); the CPU riders
// row reads N/A (`0021d9f0` gives a Duel no CPUs) while Laps still steps.
// Start Race waits on Players, which seats two players even when the Split
// Screen page counts three (the manual's two-player Duel). The race path
// driver (`autopilot=1`) rides both players two Supercross laps with no
// Final Lap sign (`001e9a00`: Single Race only) to the Single Race finish
// and combined results (`001f1990` runs for modes 0 and 5) listing the two
// players, with no Profile Points (`001f1e30` credits a lone human only).
// Host Chromium evidence with two stand-in standard gamepads.
// ATV_SHOTS=<dir> saves the setup page and the results.
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { assertNoErrors, launch, newPage, nextFrames, unlockStore, url } from './browser/harness.mjs';

const SHOTS = process.env.ATV_SHOTS || '';
const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const front = async page => (await read(page)).front;
const onFront = (page, name, timeout = 120000) =>
  page.waitForFunction(name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name, name, { timeout });
async function key(page, code) {
  await page.keyboard.down(code);
  await nextFrames(page, 2);
  await page.keyboard.up(code);
  await nextFrames(page, 3);
}
// Down (or Up) to the row for `action`.
async function select(page, action) {
  const f = await front(page);
  const at = f.choices.indexOf(action);
  assert.ok(at >= 0, `${action} in ${f.choices}`);
  for (let i = f.selection; i < at; i++) await key(page, 'ArrowDown');
  for (let i = f.selection; i > at; i--) await key(page, 'ArrowUp');
  assert.equal((await front(page)).choices[(await front(page)).selection], action);
}
async function choose(page, action) {
  await select(page, action);
  await key(page, 'Enter');
}
const label = (f, action) => f.labels[f.choices.indexOf(action)];
async function shot(page, name) {
  if (!SHOTS) return;
  await mkdir(SHOTS, { recursive: true });
  await page.screenshot({ path: `${SHOTS}/${name}.png` });
}

const browser = await launch({ args: ['--autoplay-policy=no-user-gesture-required'] });
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 }, serviceWorkers: 'block' }, errors);
  await unlockStore(page);
  await page.addInitScript(() => {
    if (sessionStorage.getItem('seeded')) return;
    sessionStorage.setItem('seeded', '1');
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.removeItem('atv-player-selection');
    localStorage.removeItem('atv-split-screen');
    localStorage.removeItem('atv-split-player2');
    // The last split session had three players: a Duel still seats two.
    localStorage.setItem('atv-split-players', '3');
    const make = (index, id) => {
      const buttons = Array.from({ length: 17 }, () => ({ pressed: false, touched: false, value: 0 }));
      return { id, index, connected: true, mapping: 'standard', timestamp: 1, axes: [0, 0, 0, 0], buttons };
    };
    const pads = [make(0, 'Stand-in pad (STANDARD GAMEPAD)'), make(1, 'Stand-in pad (STANDARD GAMEPAD)')];
    navigator.getGamepads = () => pads;
  });
  await page.goto(url('/play.html?uiDiagnostics=1&weather=off&autopilot=1'));
  await onFront(page, 'title');
  await page.mouse.click(8, 8);
  await key(page, 'Enter'); await onFront(page, 'main');
  await choose(page, 'multiplayer'); await onFront(page, 'multiplayer');
  await choose(page, 'split'); await onFront(page, 'split');
  await choose(page, 'splitrace'); await onFront(page, 'race');
  let f = await front(page);
  assert.equal(label(f, 'raceevent'), 'Event: Single Race');
  // The type cycler in split screen: Single Race, Practice, Duel.
  await select(page, 'raceevent');
  await key(page, 'ArrowLeft');
  f = await front(page);
  assert.equal(f.race.event, 'duel');
  assert.equal(label(f, 'raceevent'), 'Event: Duel');
  assert.equal(label(f, 'racecpu'), 'CPU riders: N/A');
  assert.equal(label(f, 'racelaps'), 'Laps: 3');
  // Supercross: the shortest laps; two of them.
  await key(page, 'PageUp');
  await select(page, 'racelaps');
  await key(page, 'ArrowLeft');
  f = await front(page);
  assert.ok(f.race.course.startsWith('sx'), f.race.course);
  assert.equal(f.race.laps, 2);
  assert.equal(f.race.event, 'duel');
  await shot(page, 'race-duel-setup');
  await choose(page, 'racestart');
  // Players: two seats, no third join row.
  await onFront(page, 'splitseats');
  f = await front(page);
  assert.equal(label(f, 'splitgo'), 'Start Event · 2 Players');
  assert.ok(!f.choices.some(a => a.endsWith('join')), JSON.stringify(f.choices));
  await choose(page, 'splitgo');
  await page.waitForFunction(() => {
    const s = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}');
    return s.ride?.duel === true && s.split?.player2?.race?.phase === 'racing';
  }, null, { timeout: 240000 });
  let s = await read(page);
  assert.equal(s.split.players, 2);
  assert.equal(s.split.player1.race.field, 2, 'a Duel races the two players alone');
  assert.equal(s.split.player1.race.laps, 2);
  // Both race to the finish; no Final Lap sign (12) on either view.
  const signs = new Set();
  const deadline = Date.now() + 600000;
  for (;;) {
    s = await read(page);
    for (const p of [s.split?.player1, s.split?.player2]) if (p?.race?.sign != null) signs.add(p.race.sign);
    if (s.split?.resultsShown) break;
    if (Date.now() > deadline) throw new Error(`timed out racing: ${JSON.stringify([s.split?.player1?.race, s.split?.player2?.race])}`);
    await page.waitForTimeout(250);
  }
  assert.ok(!signs.has(12), `Final Lap shown in a Duel: ${[...signs]}`);
  assert.equal(s.split.player1.race.phase, 'finished');
  assert.equal(s.split.player2.race.phase, 'finished');
  const rows = s.split.resultsRows;
  assert.equal(rows.length, 2, JSON.stringify(rows));
  assert.ok(rows.some(r => r[0].includes('Player 2')), JSON.stringify(rows));
  assert.deepEqual(rows.map(r => r[0].split('.')[0]), ['1', '2']);
  assert.ok(rows.every(r => r[4] === '-'), `no Profile Points: ${JSON.stringify(rows)}`);
  const standings = s.split.player1.race.standings;
  assert.ok(standings.every(r => r.player || r.name === 'Player 2'), JSON.stringify(standings));
  await nextFrames(page, 20);
  await shot(page, 'race-duel-results');
  assertNoErrors(errors);
  console.log(`PASS: Duel on ${f.race.course}: CPU riders N/A, two seats of three, ${rows.map(r => `${r[0]} ${r[1]}`).join(' | ')}, signs ${[...signs]}, no Profile Points`);
} finally {
  await browser.close();
}
