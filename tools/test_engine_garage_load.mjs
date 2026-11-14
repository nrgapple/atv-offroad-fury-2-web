// Garage vehicle load (#469, research/PERFORMANCE.md "Garage vehicle load"): at
// x4 CPU on a phone viewport, a settled step inflates its archive and decodes its
// textures as decode-pool jobs, builds the bike and its rider in separate frames,
// and no long task of the load comes near the half second it used to hold the
// page's thread.
// Run: node tools/run_browser_tests.mjs --only test_engine_garage_load.mjs
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, unlockStore, url } from './browser/harness.mjs';

const QUAD_NEXT = 2406;
const PST = 4;
const EXPAND = 5;
// Host x4 emulation; the motorcycle's load held the thread for about 500 ms
// before. Generous, for a shared host: the jobs are the structural check.
const LONG_TASK_BOUND = 400;

const front = page => page.evaluate(() => JSON.parse(document.getElementById('ui-state').textContent).front);

function instrument(quadId) {
  localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
  localStorage.setItem('atv-player-selection', JSON.stringify({ quadId, riderId: 0 }));
  const log = (window.__load = { jobs: [], tasks: [], links: [], audio: [] });
  const link = WebGL2RenderingContext.prototype.linkProgram;
  WebGL2RenderingContext.prototype.linkProgram = function (...args) {
    const start = performance.now();
    const result = link.apply(this, args);
    log.links.push([start, performance.now() - start]);
    return result;
  };
  const Audio = window.AudioContext;
  window.AudioContext = class extends Audio {
    constructor(...args) {
      const start = performance.now();
      super(...args);
      log.audio.push([start, performance.now() - start]);
    }
  };
  const post = Worker.prototype.postMessage;
  Worker.prototype.postMessage = function (data, ...rest) {
    if (data?.kind) log.jobs.push([performance.now(), data.kind]);
    return post.call(this, data, ...rest);
  };
  new PerformanceObserver(list => {
    for (const task of list.getEntries()) log.tasks.push([task.startTime, task.duration]);
  }).observe({ entryTypes: ['longtask'] });
}

const errors = [];
const browser = await launch({ engine: 'chromium' });
const report = {};
try {
  // A step within one archive would reuse it: each of these reads a new one.
  for (const { name, start, builds } of Array.from({ length: 3 }, () => [{ name: 'atv', start: 1, builds: 1 }, { name: 'motorcycle', start: 28, builds: 2 }]).flat()) {
    const page = await newPage(browser, { viewport: { width: 390, height: 844 }, deviceScaleFactor: 3, hasTouch: true, isMobile: true }, errors);
    await unlockStore(page);
    await page.addInitScript(instrument, start);
    await page.goto(url('/play.html?uiDiagnostics=1&front=garage'));
    await page.waitForFunction(
      total => {
        const el = document.getElementById('ui-state');
        const f = el && JSON.parse(el.textContent).front;
        return f?.page === 'garage' && !f.motion && !f.exit && f.previewBuilds?.[0] >= total;
      },
      builds,
      { timeout: 120000 }
    );
    // Deliberately wall-clock: startup's background jobs finish before the step.
    await new Promise(resolve => setTimeout(resolve, 3000));
    const cdp = await page.context().newCDPSession(page);
    await cdp.send('Emulation.setCPUThrottlingRate', { rate: 4 });
    const before = await front(page);
    const hit = before.hits.find(h => h.id === QUAD_NEXT);
    assert.ok(hit, `${name}: the next-vehicle arrow`);
    const from = await page.evaluate(() => performance.now());
    await page.touchscreen.tap(hit.x + hit.width / 2, hit.y + hit.height / 2);
    await page.waitForFunction(
      total => JSON.parse(document.getElementById('ui-state').textContent).front.previewBuilds[0] >= total,
      before.previewBuilds[0] + builds,
      { timeout: 60000 }
    );
    const log = await page.evaluate(from => ({
      jobs: window.__load.jobs.filter(j => j[0] >= from),
      links: window.__load.links.filter(t => t[0] >= from),
      audio: window.__load.audio.filter(t => t[0] >= from),
      tasks: window.__load.tasks
    }), from);
    assert.ok(log.jobs.some(j => j[1] === EXPAND), `${name}: the archive inflates as a job`);
    assert.ok(log.jobs.filter(j => j[1] === PST).length >= 3, `${name}: the textures decode as jobs`);
    assert.equal(log.links.length, 0, `${name}: settled steps reuse the preview pipeline`);
    // Long tasks of the load itself, from its archive job: a fresh browser's first
    // tap holds a separate 500 ms task before any load (research/PERFORMANCE.md).
    const first = log.jobs.find(j => j[1] === EXPAND)[0];
    const worst = Math.max(0, ...log.tasks.filter(t => t[0] >= first).map(t => t[1]));
    assert.ok(worst < LONG_TASK_BOUND, `${name}: slowest long task ${worst.toFixed(0)} ms (bound ${LONG_TASK_BOUND})`);
    (report[name] ??= []).push({ jobs: log.jobs.length, worstLongTaskMs: Math.round(worst),
      worstFromTapMs: Math.max(0, ...log.tasks.filter(t => t[0] >= from).map(t => t[1])),
      links: log.links, audio: log.audio });
    await page.context().close();
  }
} finally {
  await browser.close();
}
assertNoErrors(errors);
console.log(JSON.stringify(report));
console.log('PASS a garage step inflates and decodes as jobs and keeps the page thread free');
