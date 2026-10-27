// Freestyle target runs and the standalone Objectives mode in the browser
// (research/CHAMPIONSHIPS.md "Freestyle objectives"). The courses' OB
// archives carry the `.obj`, the green/red target TOYs and the cherries.
//
// 1. Championship → Amateur Freestyle (round 1, env05) → Hit Every Green
//    Target: the ride loads the 13 targets and their cherry models, places
//    the ATV at the objective's DefaultStartPosition, and
//    `objectiveTest=targets` sets the ATV down 8 ft short of the next
//    untouched target every 1.5 s and rolls it through at 20 ft/s, so the
//    real trigger sphere test takes each hit (500 points and 10 s each,
//    "Target Collected", "All Targets Collected"). The run completes with
//    the seconds-left bonus (2fd980) and saves bit 0.
//    With 23 objectives done before, the 24th awards bronze at once
//    (003230f0): 250 Profile Points and Pro Freestyle opened.
// 2. Single Player → Freestyle Objectives → Caribou Peak → Hit Every Red
//    Target: the same rules outside a championship; the bit (1) is saved to
//    the course's mask and Continue returns to Select Objective.
// ATV_FREESTYLE_SCREENSHOT=<dir> saves captures.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, optionalScreenshot, url } from './browser/harness.mjs';

const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const onPage = (page, name, timeout = 60000) =>
  page.waitForFunction(
    name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name,
    name,
    { timeout }
  );
async function key(page, code) {
  await page.keyboard.down(code);
  await nextFrames(page, 2);
  await page.keyboard.up(code);
  await nextFrames(page, 3);
}
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

async function toSingle(page, query) {
  await page.goto(url(`/play.html?uiDiagnostics=1&weather=off&${query}`));
  await onPage(page, 'title');
  await page.mouse.click(8, 8);
  await key(page, 'Enter');
  await onPage(page, 'main');
  await key(page, 'Enter');
  await onPage(page, 'single');
}

// Rides a target run to its results; returns the final objective state.
async function rideTargets(page, action, count, shots) {
  await choose(page, action);
  await page.waitForFunction(
    () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride?.objective?.timeLeft != null,
    null,
    { timeout: 300000 }
  );
  let state = (await read(page)).ride;
  assert.equal(state.objective.targets.length, count, JSON.stringify(state.objective.progress));
  assert.equal(state.objective.cherries, count);
  assert.ok(state.objective.start, 'the objective start position');
  assert.ok(
    state.objective.targets.every(t => t.radius === 5 && /^cherry[gr]$/.test(t.model)),
    JSON.stringify(state.objective.targets[0])
  );
  await nextFrames(page, 30);
  await optionalScreenshot(page, 'ATV_FREESTYLE_SCREENSHOT', `${shots}-start`);
  // The first hit: points, time and the sign.
  await page.waitForFunction(
    () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride?.objective?.hits?.some(h => h),
    null,
    { timeout: 60000 }
  );
  state = (await read(page)).ride.objective;
  assert.match(state.progress, /^Targets [12] of \d+$/);
  assert.ok(state.score >= 500, `score ${state.score}`);
  await optionalScreenshot(page, 'ATV_FREESTYLE_SCREENSHOT', `${shots}-hit`);
  await page.waitForFunction(
    () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride?.objective?.recorded,
    null,
    { timeout: 240000 }
  );
  state = (await read(page)).ride.objective;
  assert.equal(state.outcome, 'Completed');
  assert.ok(
    state.hits.every(h => h),
    'every target hit'
  );
  // The last sign, unless a Hall of Fame record message followed it.
  assert.match(state.message, /^(All Targets Collected|New .*!)$/);
  return state;
}

// Past the Hall of Fame name entry (one button) to Restart / Continue,
// then Continue.
async function continueResults(page) {
  await page.waitForFunction(
    () => (JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride?.results?.length ?? 0) > 0,
    null,
    { timeout: 60000 }
  );
  if ((await read(page)).ride.results.length === 1) {
    await key(page, 'Enter');
    await page.waitForFunction(
      () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride?.results?.length === 2,
      null,
      { timeout: 60000 }
    );
  }
  await key(page, 'ArrowDown');
  await key(page, 'Enter');
}

const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  // 23 of 36 Amateur objectives done: the green run's bit is the 24th.
  const done = {
    'unspstunt-normal:env02': 63,
    'unspstunt-normal:env08': 63,
    'unspstunt-normal:env06': 63,
    'unspstunt-normal:env04': 31
  };
  const start = { points: 0, races: 0, wins: 0, championships: { progress: {}, opened: [], objectives: done } };
  await page.addInitScript(start => {
    if (sessionStorage.getItem('targets-test')) return;
    sessionStorage.setItem('targets-test', '1');
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.setItem('fury-profile', JSON.stringify(start));
  }, start);

  // 1. A championship target run (Amateur round 1: Prospect Springs).
  await toSingle(page, 'objectiveTest=targets');
  await choose(page, 'championship');
  await onPage(page, 'championship');
  await choose(page, 'free:unspstunt');
  await onPage(page, 'objectives');
  let front = (await read(page)).front;
  assert.equal(labels(front)['obj:0'], 'Hit Every Green Target');
  let state = await rideTargets(page, 'obj:0', 13, 'green');
  // 13 x 500 plus the whole seconds left x 100.
  assert.equal(state.score % 100, 0);
  assert.ok(state.score >= 13 * 500 + 100 * Math.floor(state.timeLeft), `score ${state.score}`);
  await nextFrames(page, 10);
  await optionalScreenshot(page, 'ATV_FREESTYLE_SCREENSHOT', 'green-results');
  let saved = await profile(page);
  assert.equal(saved.championships.objectives['unspstunt-normal:env05'], 0b1);
  // 003230f0: reaching 24 awards bronze at once, its 250 Profile Points,
  // and opens Pro Freestyle.
  assert.equal(saved.championships.progress['unspstunt-normal'].trophy, 'Bronze', JSON.stringify(saved.championships));
  assert.ok(saved.championships.opened.includes('spstunt-normal'));
  const objectivePoints = Math.trunc(state.score / 1000);
  assert.equal(saved.points, 250 + objectivePoints, `points ${saved.points}`);
  // Continue → Select Objective, the target run done.
  await continueResults(page);
  await onPage(page, 'objectives', 120000);
  front = (await read(page)).front;
  assert.equal(labels(front)['obj:0'], 'Hit Every Green Target · Done');

  // 2. Standalone: Single Player → Freestyle Objectives → Caribou Peak.
  await toSingle(page, 'objectiveTest=targets');
  front = (await read(page)).front;
  assert.ok(front.choices.includes('objective-courses'), JSON.stringify(front.choices));
  await choose(page, 'objective-courses');
  await onPage(page, 'objective-courses');
  front = (await read(page)).front;
  assert.equal(front.choices.length, 12, JSON.stringify(front.choices));
  assert.match(labels(front)['objcourse:env05'], / · 1\/6$/);
  await optionalScreenshot(page, 'ATV_FREESTYLE_SCREENSHOT', 'standalone-courses');
  await choose(page, 'objcourse:env01');
  await onPage(page, 'objectives');
  front = (await read(page)).front;
  assert.ok(!front.choices.includes('objfinish'), 'no round row outside a championship');
  state = await rideTargets(page, 'obj:1', 8, 'red');
  assert.ok(state.score >= 8 * 1000, `score ${state.score}`);
  saved = await profile(page);
  assert.equal(saved.championships.objectives['unspstunt-normal:env01'], 0b10);
  // Outside a championship no trophy comes (25 objectives, no silver).
  assert.equal(saved.championships.progress['unspstunt-normal'].trophy, 'Bronze');
  await continueResults(page);
  await onPage(page, 'objectives', 120000);
  front = (await read(page)).front;
  assert.equal(labels(front)['obj:1'], 'Hit Every Red Target · Done');
  await key(page, 'Escape');
  await onPage(page, 'objective-courses');
  assertNoErrors(errors);
  console.log('Freestyle targets: championship green run, standalone red run, completion bits saved');
} finally {
  await browser.close();
}
