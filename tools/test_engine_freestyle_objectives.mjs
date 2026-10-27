// Freestyle objectives and the Amateur Freestyle championship in the browser
// (research/CHAMPIONSHIPS.md "Freestyle objectives"). A profile on the final
// round of Amateur Freestyle (env01) with every objective of the first five
// courses done (30 of 36) opens Championship → Amateur Freestyle → Select
// Objective. `objectiveTest=timeout` leaves the Combos run five seconds: the
// HUD panel counts down and the run fails ("Sorry"); Continue returns to
// Select Objective (Pause → Restart first restarts the run's clock). A second load with `objectiveTest=score` credits the last
// point level: Score Stunt Points completes, its three level bits count
// (33 of 36), and Finish Championship awards silver (30+), its 500 Profile
// Points (00322650) and opens Pro Freestyle.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, optionalScreenshot, url } from './browser/harness.mjs';

const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const onPage = (page, name, timeout = 60000) => page.waitForFunction(name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name, name, { timeout });
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }
const profile = page => page.evaluate(() => JSON.parse(localStorage.getItem('fury-profile') || '{}'));
const labels = front => Object.fromEntries(front.choices.map((c, i) => [c, front.labels[i]]));

async function choose(page, action) {
  const front = (await read(page)).front;
  const at = front.choices.indexOf(action);
  assert.ok(at >= 0, `${action} in ${front.choices}`);
  for (let i = front.selection; i < at; i++) await key(page, 'ArrowDown');
  for (let i = front.selection; i > at; i--) await key(page, 'ArrowUp');
  await key(page, 'Enter');
}

async function toObjectives(page, query) {
  await page.goto(url(`/play.html?uiDiagnostics=1&weather=off&${query}`));
  await onPage(page, 'title');
  await page.mouse.click(8, 8);
  await key(page, 'Enter'); await onPage(page, 'main');
  await key(page, 'Enter'); await onPage(page, 'single');
  await choose(page, 'championship'); await onPage(page, 'championship');
}

async function rideObjective(page, action) {
  await choose(page, action);
  await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride?.objective?.timeLeft != null, null, { timeout: 300000 });
}

async function waitResults(page) {
  await page.waitForFunction(() => {
    const ride = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride;
    return ride?.objective?.recorded && ride?.results?.length === 2;
  }, null, { timeout: 120000 });
}

const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  const done = Object.fromEntries(['env05', 'env02', 'env08', 'env06', 'env04'].map(c => [`unspstunt-normal:${c}`, 63]));
  const start = {
    points: 40, races: 5, wins: 5,
    championships: { progress: { 'unspstunt-normal': { round: 5, points: [0, 0, 0, 0, 0] } }, opened: [], objectives: done },
  };
  await page.addInitScript(start => {
    if (sessionStorage.getItem('freestyle-test')) return;
    sessionStorage.setItem('freestyle-test', '1');
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.setItem('fury-profile', JSON.stringify(start));
  }, start);

  await toObjectives(page, 'objectiveTest=timeout');
  let front = (await read(page)).front;
  assert.equal(labels(front)['free:unspstunt'], 'Amateur Freestyle · Round 6 of 6 · 30/36');
  assert.equal(labels(front)['free:spstunt'], 'Pro Freestyle · Locked');
  await optionalScreenshot(page, 'ATV_FREESTYLE_SCREENSHOT', 'championship');
  await choose(page, 'free:unspstunt'); await onPage(page, 'objectives');
  front = (await read(page)).front;
  assert.deepEqual(front.labels.slice(0, 5), [
    'Hit Every Green Target · Unavailable',
    'Hit Every Red Target · Unavailable',
    'Bust 10 Different Combos',
    'Score Stunt Points · 0 of 3 levels',
    'Finish Championship',
  ]);
  await optionalScreenshot(page, 'ATV_FREESTYLE_SCREENSHOT', 'select');

  // The Combos run times out: Sorry, nothing earned.
  await rideObjective(page, 'obj:2');
  let state = (await read(page)).ride;
  assert.equal(state.environment, 'env01');
  assert.equal(state.objective.progress, 'Combos 0 of 10');
  // Pause → Restart starts the run over: the clock is back at its start.
  await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride?.objective?.timeLeft < 4.5, null, { timeout: 30000 });
  await key(page, 'Escape');
  await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').toolbar?.menu?.page === 'pause', null, { timeout: 30000 });
  const menu = (await read(page)).toolbar.menu;
  assert.equal(menu.choices[1], 'reset');
  const before = (await read(page)).ride.objective.timeLeft;
  assert.ok(before > 0 && before < 4.5, `paused at ${before}`);
  await key(page, 'ArrowDown');
  await key(page, 'Enter');
  await page.waitForFunction(before => {
    const s = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}');
    return !s.paused && s.ride?.objective?.timeLeft > before + 0.2 && !s.ride.objective.recorded;
  }, before, { timeout: 30000 });
  state = (await read(page)).ride;
  assert.equal(state.objective.score, 0);
  await nextFrames(page, 20);
  await optionalScreenshot(page, 'ATV_FREESTYLE_SCREENSHOT', 'hud');
  await waitResults(page);
  state = (await read(page)).ride;
  assert.equal(state.objective.timeLeft, 0);
  await nextFrames(page, 5);
  await optionalScreenshot(page, 'ATV_FREESTYLE_SCREENSHOT', 'failed');
  assert.equal((await profile(page)).championships.objectives['unspstunt-normal:env01'] ?? 0, 0);
  await key(page, 'ArrowDown');
  await key(page, 'Enter');
  await onPage(page, 'objectives', 120000);

  // Score Stunt Points completes: three level bits.
  await toObjectives(page, 'objectiveTest=score');
  await choose(page, 'free:unspstunt'); await onPage(page, 'objectives');
  await rideObjective(page, 'obj:3');
  await waitResults(page);
  await nextFrames(page, 5);
  await optionalScreenshot(page, 'ATV_FREESTYLE_SCREENSHOT', 'completed');
  let saved = await profile(page);
  assert.equal(saved.championships.objectives['unspstunt-normal:env01'], 0b111000);
  await key(page, 'ArrowDown');
  await key(page, 'Enter');
  await onPage(page, 'objectives', 120000);
  front = (await read(page)).front;
  assert.equal(labels(front)['obj:3'], 'Score Stunt Points · 3 of 3 levels');
  await choose(page, 'objfinish');
  await onPage(page, 'championship');
  front = (await read(page)).front;
  assert.match(front.notice ?? '', /silver trophy! \(\+500 Profile Points\) Pro Freestyle opened\./);
  assert.match(labels(front)['free:unspstunt'], /Round 1 of 6 · 33\/36 · Silver/);
  assert.match(labels(front)['free:spstunt'], /^Pro Freestyle · Round 1 of 5/);
  saved = await profile(page);
  assert.equal(saved.points, 540);
  assert.ok(saved.championships.opened.includes('spstunt-normal'));
  await optionalScreenshot(page, 'ATV_FREESTYLE_SCREENSHOT', 'trophy');
  assertNoErrors(errors);
  console.log('Freestyle objectives: timeout and completion runs, championship trophy and unlock');
} finally {
  await browser.close();
}
