// Split Screen → Freestyle Objectives (research/SPLIT_SCREEN.md "Split
// objectives") in /play.html with two stand-in standard gamepads.
//
// 1. Split Screen → Freestyle Objectives → Prospect Springs → Hit Every
//    Green Target → Players → Start Event: both players ride one target
//    run. `objectiveTest=targets` rolls the players in turn through the
//    next untouched target (target i to player i % 2), so the real trigger
//    test credits each hit to the player whose ATV touched it. The targets
//    are shared, the run ends at the last one, the player with the most
//    targets wins (002fab40) and the results list both players. Nothing is
//    saved to the profile (002fce00).
// 2. Score Stunt Points with `objectiveTest=timeout` (5 s): point levels
//    never end a split run; at time out the top score wins, equal scores
//    share it (002fb960).
// ATV_SPLIT_SHOTS=<dir> saves captures. Host Chromium evidence only.
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { assertNoErrors, launch, newPage, nextFrames, unlockStore, url } from './browser/harness.mjs';

const SHOTS = process.env.ATV_SPLIT_SHOTS || '';
const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const wait = (page, predicate, arg, timeout = 120000) => page.waitForFunction(predicate, arg, { timeout });
const onFront = (page, name) =>
  wait(page, name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name, name);
const objective = page => read(page).then(s => s.ride?.objective);
async function key(page, code) {
  await page.keyboard.down(code);
  await nextFrames(page, 2);
  await page.keyboard.up(code);
  await nextFrames(page, 3);
}
async function choose(page, action) {
  const f = (await read(page)).front;
  const at = f.choices.indexOf(action);
  assert.ok(at >= 0, `${action} in ${f.choices}`);
  for (let i = f.selection; i < at; i++) await key(page, 'ArrowDown');
  for (let i = f.selection; i > at; i--) await key(page, 'ArrowUp');
  await key(page, 'Enter');
}
async function shot(page, name) {
  if (!SHOTS) return;
  await mkdir(SHOTS, { recursive: true });
  await nextFrames(page, 3);
  await page.screenshot({ path: `${SHOTS}/${name}.png` });
}

async function open(browser, query) {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 }, serviceWorkers: 'block' }, errors);
  await unlockStore(page);
  await page.addInitScript(() => {
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.setItem('atv-player-selection', JSON.stringify({ quadId: 1, riderId: 0 }));
    localStorage.setItem('atv-split-players', '2');
    localStorage.removeItem('atv-split-screen');
    const make = index => ({
      id: 'Stand-in pad (STANDARD GAMEPAD)', index, connected: true, mapping: 'standard', timestamp: 1,
      axes: [0, 0, 0, 0], buttons: Array.from({ length: 17 }, () => ({ pressed: false, touched: false, value: 0 })),
    });
    const pads = [make(0), make(1)];
    navigator.getGamepads = () => pads;
  });
  await page.goto(url(`/play.html?uiDiagnostics=1&weather=off&${query}`));
  await onFront(page, 'title');
  await page.mouse.click(8, 8);
  await key(page, 'Enter');
  await onFront(page, 'main');
  await choose(page, 'multiplayer');
  await onFront(page, 'multiplayer');
  await choose(page, 'split');
  await onFront(page, 'split');
  return { page, errors };
}

// Split Screen → Freestyle Objectives → a course → an objective →
// Players → Start Event; waits for the shared run.
async function rideObjective(page, course, action) {
  await choose(page, 'splitobjectives');
  await onFront(page, 'objective-courses');
  await choose(page, `objcourse:${course}`);
  await onFront(page, 'objectives');
  await choose(page, action);
  await onFront(page, 'splitseats');
  await choose(page, 'splitgo');
  await wait(page, () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride?.objective?.split,
    null, 300000);
}

const browser = await launch({ args: ['--autoplay-policy=no-user-gesture-required'] });
try {
  {
    const { page, errors } = await open(browser, 'objectiveTest=targets');
    const before = await page.evaluate(() => localStorage.getItem('fury-profile'));
    await rideObjective(page, 'env05', 'obj:0');
    let o = await objective(page);
    assert.equal((await read(page)).split?.players, 2);
    assert.equal(o.split.players, 2);
    assert.equal(o.split.taken.length, 13, JSON.stringify(o.split));
    assert.equal(o.run ?? null, null);
    await nextFrames(page, 30);
    await shot(page, 'split-targets-start');
    // Hits go to the player that touched them, both players score.
    await wait(page, () => {
      const o = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride?.objective?.split;
      return o && o.taken.includes(0) && o.taken.includes(1);
    }, null, 120000);
    o = await objective(page);
    assert.ok(o.split.scores.every(s => s >= 500), JSON.stringify(o.split.scores));
    const hud = (await read(page)).toolbar?.hud ?? (await read(page)).hud ?? {};
    // The target compass, when drawn, lists the untouched targets.
    const left = o.split.taken.filter(t => t === null).length;
    if (hud.compass) assert.ok(Math.abs(hud.compass.length - left) <= 1, `compass ${hud.compass.length} vs ${left}`);
    console.log(`  compass ${hud.compass ? hud.compass.length : 'not drawn'} for ${left} untouched targets`);
    await shot(page, 'split-targets-hit');
    await wait(page, () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride?.objective?.recorded,
      null, 300000);
    o = await objective(page);
    assert.ok(o.split.taken.every(t => t === 0 || t === 1), JSON.stringify(o.split.taken));
    const counts = [0, 1].map(p => o.split.taken.filter(t => t === p).length);
    const most = Math.max(...counts);
    assert.deepEqual(o.split.winners, [0, 1].filter(p => counts[p] === most), JSON.stringify({ counts, o: o.split }));
    assert.deepEqual(o.split.outcomes, ['Completed', 'Completed']);
    await wait(page, () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride?.objective?.summary,
      null, 60000);
    o = await objective(page);
    assert.match(o.summary, /^(Congratulations, Player [12]!|Tie: Player 1 and Player 2!) Hit Every Green Target/);
    assert.match(o.summary, /1st Player [12] \d+ targets?, [\d,]+ pts · 2nd Player [12]/);
    assert.match(o.summary, /not saved/);
    assert.equal(await page.evaluate(() => localStorage.getItem('fury-profile')), before, 'split saves nothing');
    await shot(page, 'split-targets-results');
    console.log(`  target run: ${counts.join(' / ')} targets, winners ${o.split.winners}`);
    // Restart Objective (the first results button) rides it again, both
    // players, every target back.
    const results = (await read(page)).ride.results;
    assert.equal(results.length, 2, JSON.stringify(results));
    await key(page, 'Enter');
    await wait(page, () => {
      const s = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}');
      const o = s.ride?.objective;
      return o?.split && !o.recorded && o.split.taken.filter(t => t === null).length >= 12 && s.split?.players === 2;
    }, null, 300000);
    console.log('  Restart Objective: a fresh shared run for both players');
    assertNoErrors(errors);
    await page.close();
  }
  {
    const { page, errors } = await open(browser, 'objectiveTest=timeout');
    await rideObjective(page, 'env05', 'obj:3');
    let o = await objective(page);
    assert.ok(o.split.timeLeft <= 5, JSON.stringify(o.split));
    await wait(page, () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride?.objective?.recorded,
      null, 120000);
    o = await objective(page);
    const top = Math.max(...o.split.scores);
    assert.deepEqual(o.split.winners, [0, 1].filter(p => o.split.scores[p] === top), JSON.stringify(o.split));
    assert.deepEqual(o.split.outcomes, ['Failed', 'Failed']);
    await wait(page, () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride?.objective?.summary,
      null, 60000);
    await shot(page, 'split-stunt-timeout');
    console.log(`  stunt time out: scores ${o.split.scores}, winners ${o.split.winners}`);
    assertNoErrors(errors);
    await page.close();
  }
  console.log('split objectives: shared targets credited per player, winners at the end and at time out');
} finally {
  await browser.close();
}
