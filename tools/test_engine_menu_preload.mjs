// Menu course preload (issue #453): while Select Track or the race setup rests
// on a course, the page prepares it in the background, so Start is faster.
//
// Fresh browser contexts, so every run starts with nothing cached:
//   without  `?preload=0`: the menu does nothing, Start loads cold;
//   with     the menu settles, prepares the course, Start finds it ready.
// Also checked: nothing starts before the selection has rested; a selection
// change supersedes the run and the second course is the one prepared; a
// prepared course prepares nothing again; Start stops a running preload;
// data-saver refuses it; the race setup prepares its race course; and the
// page's long tasks while preparing stay under 100 ms.
// Works against production:
//   node tools/run_browser_tests.mjs --only test_engine_menu_preload.mjs --url https://furyremastered.com
// MENU_PRELOAD_SHOT=out.png saves Select Track with a prepared course.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, unlockStore, url } from './browser/harness.mjs';

const ROUNDS = 2;
const state = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const preload = page => page.evaluate(() => JSON.parse(window.atvPreload || '{}'));

async function until(page, check, what, timeout = 120000) {
  const end = Date.now() + timeout;
  for (;;) {
    const report = await preload(page);
    if (check(report)) return report;
    assert.ok(Date.now() < end, `${what}: ${JSON.stringify(report)}`);
    await page.waitForTimeout(25);
  }
}

async function context(browser, errors, init) {
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await unlockStore(page);
  await page.addInitScript(() => {
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    window.atvLongTasks = [];
    new PerformanceObserver(list => {
      for (const entry of list.getEntries()) window.atvLongTasks.push(Math.round(entry.duration));
    }).observe({ entryTypes: ['longtask'] });
  });
  if (init) await page.addInitScript(init);
  return page;
}

async function menu(page, name, extra = '') {
  await page.goto(url(`/play.html?uiDiagnostics=1&weather=off&front=${name}${extra}`));
  await page.waitForFunction(p => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === p, name, { timeout: 120000 });
}

/** Start from the current page; the milliseconds until the ride is live. */
async function start(page) {
  const clicked = Date.now();
  await page.keyboard.press('Enter');
  await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride?.environment, null, { timeout: 180000, polling: 16 });
  const elapsed = Date.now() - clicked;
  const cache = (await state(page)).decodedCache;
  return { elapsed, loadMs: cache.loadMs, hits: cache.hits, misses: cache.misses };
}

const best = runs => runs.reduce((a, b) => (b.loadMs < a.loadMs ? b : a));

const browser = await launch();
try {
  const errors = [];

  // Start without a preload, cold, then with one, alternating.
  const without = [], withPreload = [];
  let report, course;
  for (let round = 0; round < ROUNDS; round++) {
    let page = await context(browser, errors);
    await menu(page, 'tracks', '&preload=0');
    await page.waitForTimeout(1500); // Deliberately wall-clock: the menu art settles as in the run below.
    assert.equal((await preload(page)).started ?? 0, 0, '?preload=0 keeps it idle');
    without.push(await start(page));
    await page.context().close();

    page = await context(browser, errors);
    await menu(page, 'tracks');
    report = await until(page, r => r.state === 'done' || r.state === 'skipped' || r.state === 'failed', 'preload ends');
    assert.equal(report.state, 'done', `preload finished: ${JSON.stringify(report)}`);
    assert.equal(report.completed, 1, `one run prepared the course: ${JSON.stringify(report)}`);
    assert.ok(report.settleMs >= 700, `it waited for the selection to rest: ${report.settleMs} ms`);
    course = report.course;
    await page.waitForFunction(() => {
      // Live counters: an idle menu draws no frames, so #ui-state's copy goes stale.
      const c = JSON.parse(window.atvDecodedCache || 'null');
      return !c || (c.pending === 0);
    }, null, { timeout: 60000, polling: 100 });
    if (round === 0 && process.env.MENU_PRELOAD_SHOT) await page.screenshot({ path: process.env.MENU_PRELOAD_SHOT });
    const run = await start(page);
    assert.ok(run.hits > 0, `Start hit the cache: ${JSON.stringify(run)}`);
    withPreload.push(run);
    await page.context().close();
  }
  const [slow, fast] = [best(without), best(withPreload)];
  assert.ok(fast.loadMs < slow.loadMs * 0.9, `Start after a preload (${fast.loadMs} ms) is not measurably faster than without one (${slow.loadMs} ms)`);

  // A selection change supersedes the run; only the new course is prepared.
  const change = await context(browser, errors);
  await menu(change, 'tracks');
  const first = await until(change, r => r.state === 'running' || r.state === 'done', 'the first course starts');
  await change.keyboard.press('ArrowRight');
  const second = await until(change, r => r.course && r.course !== first.course, 'the selection changed');
  assert.ok(second.superseded >= 1 || first.state === 'done', `the change superseded the run: ${JSON.stringify(second)}`);
  const settled = await until(change, r => r.state === 'done' && r.course === second.course, 'the second course is prepared');
  assert.ok(settled.prepared.includes(second.course), `second course prepared: ${JSON.stringify(settled)}`);
  assert.equal(settled.superseded >= 1, first.state !== 'done', `superseded count: ${JSON.stringify(settled)} after ${JSON.stringify(first)}`);

  // A run costs the page no long frame: measured on the next course, once the
  // page's own start-up work (which has long tasks of its own) is over.
  await change.waitForTimeout(4000); // Deliberately wall-clock: start-up and menu art settle.
  await change.evaluate(() => { window.atvLongTasks.length = 0; });
  await change.keyboard.press('ArrowRight');
  const third = await until(change, r => r.state === 'running' && r.course !== second.course, 'the next course starts');
  await until(change, r => r.state === 'done' && r.course === third.course, 'the next course is prepared');
  const longTasks = await change.evaluate(() => window.atvLongTasks.slice());
  assert.ok(Math.max(0, ...longTasks) < 100, `long tasks while preparing a course: ${longTasks}`);
  await change.keyboard.press('ArrowLeft');
  await until(change, r => r.state === 'done' && r.course === second.course, 'back on the prepared course');
  const settledAgain = await preload(change);

  // Rapid changes never start a run: the selection has not rested.
  const before = settledAgain.started;
  for (let i = 0; i < 4; i++) {
    await change.keyboard.press('ArrowRight');
    await change.waitForTimeout(150); // Deliberately wall-clock: faster than the debounce.
  }
  const moving = await preload(change);
  assert.equal(moving.started, before, `no run while the selection moves: ${JSON.stringify(moving)}`);
  assert.equal(moving.state, 'waiting', `waiting for it to rest: ${JSON.stringify(moving)}`);

  // Back on a prepared course: nothing to do, no second preparation.
  // (It may stop at the cache budget: this page's own entries are never evicted.)
  const landed = await until(change, r => r.state === 'done' || (r.state === 'skipped' && r.reason === 'cache-budget'), 'the last course settles');
  for (let i = 0; i < 4; i++) await change.keyboard.press('ArrowLeft');
  // (Four steps right, four steps back: the course the run below came from.)
  const again = await until(change, r => r.course === second.course && r.state === 'done' && r.started > landed.started, 'back on the prepared course');
  assert.equal(again.completed, landed.completed, `nothing prepared again: ${JSON.stringify(again)}`);
  assert.ok(again.cached > landed.cached, `found prepared: ${JSON.stringify(again)}`);
  await change.context().close();

  // Start during a run stops it.
  const racing = await context(browser, errors);
  await menu(racing, 'tracks');
  await until(racing, r => r.state === 'running', 'a run starts');
  await racing.keyboard.press('Enter');
  const stopped = await until(racing, r => r.stopped >= 1 || r.state === 'idle', 'Start stops the run');
  assert.ok(stopped.state === 'idle', `stopped: ${JSON.stringify(stopped)}`);
  await racing.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride?.environment, null, { timeout: 180000 });
  await racing.context().close();

  // The race setup prepares its race course.
  const race = await context(browser, errors);
  await menu(race, 'race');
  const raced = await until(race, r => r.state === 'done' || r.state === 'failed', 'the race course is prepared');
  assert.equal(raced.state, 'done', `race course prepared: ${JSON.stringify(raced)}`);
  await race.context().close();

  // Data saver: no preload.
  const saver = await context(browser, errors, () => {
    Object.defineProperty(navigator, 'connection', { value: { saveData: true, effectiveType: '4g' }, configurable: true });
  });
  await menu(saver, 'tracks');
  const refused = await until(saver, r => r.state === 'skipped' || r.state === 'done', 'data saver is honored');
  assert.deepEqual([refused.state, refused.reason, refused.completed], ['skipped', 'save-data', 0], JSON.stringify(refused));
  await saver.context().close();

  assertNoErrors(errors);
  console.log(`PASS: menu preload ${course}: Start without ${without.map(r => r.loadMs).join('/')} ms load (${without.map(r => r.elapsed).join('/')} ms wall); after a preload ${withPreload.map(r => r.loadMs).join('/')} ms load (${withPreload.map(r => r.elapsed).join('/')} ms wall); preload ${report.runMs} ms, longest page task ${Math.max(0, ...longTasks)} ms (${longTasks.length} over 50 ms); superseded and refused as expected. Host measurements.`);
} finally {
  await browser.close();
}
