// Host phone-emulation measurement of the menus and of ride loading.
// 390x844 at DPR 3 with touch; Chromium with CDP CPU throttling, WebKit
// unthrottled (Playwright has no WebKit CPU throttle). These are host browser
// numbers on the host GPU, not physical-phone results.
//
//   ATV_TEST_URL=http://127.0.0.1:4173 PLAYWRIGHT_MODULE=... node tools/benchmark_phone.mjs
//   ATV_JS_URL=http://127.0.0.1:4178 ... node tools/benchmark_phone.mjs --target js
//
// Options (flags): --target rust|js, --engine chromium|webkit, --throttle N,
// --scenarios boot,pages,load, --pages title,main,..., --maps env01,...,
// --label NAME (report file name), --out DIR (work/verification/phone-perf).
// Each scenario opens a fresh browser context (cold HTTP cache).
//
// Instrumentation (an init script, no game hooks): requestAnimationFrame is
// wrapped to time each callback (the engine draws from rAF); WebGL draw calls
// are counted per frame; #status text changes are timestamped (the Rust
// loader reports each load stage there). Chromium adds CDP TaskDuration and
// long tasks.
import { mkdir, writeFile } from 'node:fs/promises';
import { base, jsBase, launch, newPage } from './browser/harness.mjs';

const args = process.argv.slice(2);
const flag = (name, fallback) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 ? args[i + 1] : fallback;
};
const target = flag('target', 'rust');
const engine = flag('engine', 'chromium');
const throttle = Number(flag('throttle', engine === 'chromium' ? 4 : 1));
const scenarios = flag('scenarios', 'boot,pages,load').split(',');
const pages = flag('pages', target === 'js' ? 'title,main' : 'title,main,single,race,garage,profile,tracks').split(',');
const maps = flag('maps', 'env01').split(',');
const out = flag('out', 'work/verification/phone-perf');
const label = flag('label', `${target}-${engine}-x${throttle}`);
const site = target === 'js' ? jsBase : base;
const PHONE = { viewport: { width: 390, height: 844 }, deviceScaleFactor: 3, hasTouch: true, isMobile: engine !== 'firefox' && engine !== 'webkit' ? true : undefined };
if (engine === 'webkit') delete PHONE.isMobile;

const pct = (values, p) => {
  const s = [...values].sort((a, b) => a - b);
  return s.length ? s[Math.min(s.length - 1, Math.floor(p * s.length))] : null;
};
const round = v => (v === null || v === undefined ? null : Math.round(v * 10) / 10);
const sum = values => values.reduce((a, b) => a + b, 0);
/** Frame-interval and callback summary of a window of samples. */
function summarize(frames, seconds) {
  const intervals = frames.map(f => f.interval).filter(v => v > 0);
  const callbacks = frames.map(f => f.callback);
  const drawn = frames.filter(f => f.draws > 0).length;
  return {
    frames: frames.length,
    drawnFrames: drawn,
    fps: round(frames.length / seconds),
    intervalMs: { p50: round(pct(intervals, 0.5)), p95: round(pct(intervals, 0.95)), max: round(Math.max(0, ...intervals)) },
    long33: intervals.filter(v => v > 33.4).length,
    long50: intervals.filter(v => v > 50).length,
    long100: intervals.filter(v => v > 100).length,
    callbackMs: { p50: round(pct(callbacks, 0.5)), p95: round(pct(callbacks, 0.95)), max: round(Math.max(0, ...callbacks)), perSecond: round(sum(callbacks) / seconds) },
    drawsPerDrawnFrame: drawn ? round(sum(frames.map(f => f.draws)) / drawn) : 0
  };
}

function instrument() {
  const bench = (window.__bench = { frames: [], status: [], longTasks: [], uploads: 0, uploadBytes: 0, programs: 0, firstDraw: null, frameDraws: 0, frameUploads: 0, last: 0 });
  const proto = WebGL2RenderingContext.prototype;
  for (const name of ['drawElements', 'drawElementsInstanced', 'drawArrays', 'drawArraysInstanced', 'drawRangeElements']) {
    const original = proto[name];
    proto[name] = function (...a) {
      bench.frameDraws++;
      if (bench.firstDraw === null) bench.firstDraw = performance.now();
      return original.apply(this, a);
    };
  }
  for (const name of ['texImage2D', 'texSubImage2D', 'texImage3D', 'texSubImage3D', 'compressedTexImage2D', 'bufferData', 'bufferSubData']) {
    const original = proto[name];
    proto[name] = function (...a) {
      bench.uploads++;
      bench.frameUploads++;
      const data = a.find(x => x && x.byteLength !== undefined);
      if (data) bench.uploadBytes += data.byteLength;
      return original.apply(this, a);
    };
  }
  const link = proto.linkProgram;
  proto.linkProgram = function (...a) {
    bench.programs++;
    return link.apply(this, a);
  };
  const raf = window.requestAnimationFrame.bind(window);
  let pending = 0;
  window.requestAnimationFrame = callback =>
    raf(time => {
      const started = performance.now();
      try {
        callback(time);
      } finally {
        pending += performance.now() - started;
      }
    });
  const tick = time => {
    bench.frames.push({ t: time, interval: bench.last ? time - bench.last : 0, callback: pending, draws: bench.frameDraws, uploads: bench.frameUploads });
    bench.last = time;
    bench.frameDraws = 0;
    bench.frameUploads = 0;
    pending = 0;
    raf(tick);
  };
  raf(tick);
  try {
    new PerformanceObserver(list => {
      for (const e of list.getEntries()) bench.longTasks.push([Math.round(e.startTime), Math.round(e.duration)]);
    }).observe({ type: 'longtask', buffered: true });
  } catch {}
  const watch = () => {
    const status = document.getElementById('status');
    if (!status) return setTimeout(watch, 20);
    let last = '';
    new MutationObserver(() => {
      const text = status.textContent;
      if (text !== last) bench.status.push([Math.round(performance.now()), (last = text)]);
    }).observe(status, { childList: true, characterData: true, subtree: true });
  };
  document.readyState === 'loading' ? addEventListener('DOMContentLoaded', watch) : watch();
}

const browser = await launch({ engine });
const report = { target, engine, throttle, site, label, viewport: '390x844@3', when: new Date().toISOString(), scenarios: {} };

async function open(path) {
  const errors = [];
  const page = await newPage(browser, PHONE, errors);
  await page.addInitScript(() => {
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
  });
  await page.addInitScript(instrument);
  let cdp = null;
  if (engine === 'chromium') {
    cdp = await page.context().newCDPSession(page);
    await cdp.send('Performance.enable');
    if (throttle > 1) await cdp.send('Emulation.setCPUThrottlingRate', { rate: throttle });
  }
  await page.goto(`${site}${path}`);
  return { page, cdp, errors };
}
const busy = async cdp => (cdp ? Object.fromEntries((await cdp.send('Performance.getMetrics')).metrics.map(m => [m.name, m.value])).TaskDuration : null);
const mark = page => page.evaluate(() => performance.now());
const framesSince = (page, from, to = Infinity) => page.evaluate(([from, to]) => window.__bench.frames.filter(f => f.t >= from && f.t < to), [from, to]);
/** Samples a window of `ms` (deliberately wall-clock) while `during` runs. */
async function sample(page, cdp, ms, during) {
  const b0 = await busy(cdp);
  const from = await mark(page);
  const tasks0 = await page.evaluate(() => window.__bench.longTasks.length);
  const run = during ? during() : null;
  await page.waitForTimeout(ms); // Deliberately wall-clock: fixed sampling window.
  if (run) await run;
  const to = await mark(page);
  const b1 = await busy(cdp);
  const frames = await framesSince(page, from, to);
  const longTasks = await page.evaluate(n => window.__bench.longTasks.slice(n), tasks0);
  return { ...summarize(frames, (to - from) / 1000), busy: b0 === null ? null : round((b1 - b0) / ((to - from) / 1000) * 100) / 100, longTasks: longTasks.length, longTaskMaxMs: Math.max(0, ...longTasks.map(t => t[1])) };
}

try {
  if (scenarios.includes('boot')) {
    // Start-up to the title and its first seconds (the "beginning").
    const { page, cdp, errors } = await open('/play.html');
    // The Rust title is drawn by WebGL; the JavaScript game's title is DOM.
    await page.waitForFunction(() => window.__bench.firstDraw !== null || document.querySelector('#menu-buttons button'), null, { timeout: 120000 });
    const firstDraw = await page.evaluate(() => window.__bench.firstDraw ?? performance.now());
    await page.waitForTimeout(10000); // Deliberately wall-clock: the first ten seconds after the first frame.
    const frames = await framesSince(page, firstDraw, firstDraw + 10000);
    const bench = await page.evaluate(() => ({ longTasks: window.__bench.longTasks, uploads: window.__bench.uploads, uploadBytes: window.__bench.uploadBytes, programs: window.__bench.programs }));
    const firstSeconds = summarize(frames, 10);
    const idle = await sample(page, cdp, 3000);
    report.scenarios.boot = {
      firstDrawMs: round(firstDraw),
      first10s: { ...firstSeconds, longTasks: bench.longTasks.filter(t => t[0] >= firstDraw).length, longTaskMs: sum(bench.longTasks.filter(t => t[0] >= firstDraw).map(t => t[1])), longTaskMax: Math.max(0, ...bench.longTasks.map(t => t[1])) },
      worstFrames: frames.filter(f => f.interval > 50).map(f => [Math.round(f.t - firstDraw), Math.round(f.interval), Math.round(f.callback)]).slice(0, 40),
      idleAfter: idle,
      uploads: bench.uploads,
      uploadMB: round(bench.uploadBytes / 1e6),
      programs: bench.programs,
      errors
    };
    console.log('boot', JSON.stringify(report.scenarios.boot));
    await page.context().close();
  }
  if (scenarios.includes('pages')) {
    report.scenarios.pages = {};
    for (const name of pages) {
      const path = target === 'js' ? '/play.html' : name === 'title' ? '/play.html' : `/play.html?front=${name}`;
      const { page, cdp, errors } = await open(path);
      await page.waitForFunction(() => window.__bench.firstDraw !== null || document.querySelector('#menu-buttons button'), null, { timeout: 120000 });
      await page.waitForTimeout(8000); // Deliberately wall-clock: page artwork and previews settle.
      await page.locator('#game').focus().catch(() => {});
      if (target === 'js' && name === 'main') {
        await page.keyboard.press('Enter');
        await page.waitForTimeout(3000); // Deliberately wall-clock: page settles.
      }
      const row = { idle: await sample(page, cdp, 3000) };
      // Selection moves (keyboard focus): a row change and its redraw.
      row.select = await sample(page, cdp, 2000, async () => {
        for (let i = 0; i < 6; i++) {
          await page.keyboard.press(i < 3 ? 'ArrowDown' : 'ArrowUp');
          await page.waitForTimeout(250); // Deliberately wall-clock: paced key presses.
        }
      });
      // Scrolling a list (wheel over the page centre).
      if (['race', 'tracks', 'profile'].includes(name)) {
        row.scroll = await sample(page, cdp, 2000, async () => {
          await page.mouse.move(195, 600);
          for (let i = 0; i < 12; i++) {
            await page.mouse.wheel(0, i < 6 ? 120 : -120);
            await page.waitForTimeout(100); // Deliberately wall-clock: paced wheel steps.
          }
        });
      }
      // A page transition: Back, then forward again with Enter.
      if (name !== 'title') {
        row.transition = await sample(page, cdp, 2500, async () => {
          await page.keyboard.press('Escape');
          await page.waitForTimeout(1200); // Deliberately wall-clock: the slide runs.
          await page.keyboard.press('Enter');
        });
      }
      row.errors = errors;
      report.scenarios.pages[name] = row;
      console.log('page', name, JSON.stringify(row));
      await page.context().close();
    }
  }
  if (scenarios.includes('load')) {
    report.scenarios.load = {};
    for (const map of maps) {
      const { page, cdp, errors } = await open(`/play.html?drive=1&environment=${map}`);
      // The first ride frame: a frame drawing the 3D scene (40+ draw calls).
      await page.waitForFunction(() => window.__bench.frames.some(f => f.draws >= 40 && f.t > 0), null, { timeout: 180000 });
      const t = await page.evaluate(() => {
        const b = window.__bench;
        const ride = b.frames.find(f => f.draws >= 40);
        return { firstDraw: b.firstDraw, ride: ride.t, status: b.status, programs: b.programs, uploads: b.uploads, uploadBytes: b.uploadBytes, longTasks: b.longTasks, nav: performance.getEntriesByType('navigation')[0]?.domContentLoadedEventEnd ?? null };
      });
      const resources = await page.evaluate(() => {
        const entries = performance.getEntriesByType('resource');
        const content = entries.filter(e => e.name.includes('/content/'));
        return { count: entries.length, content: content.length, contentKB: Math.round(content.reduce((a, e) => a + (e.encodedBodySize || e.transferSize || 0), 0) / 1024), lastContentEnd: Math.round(Math.max(0, ...content.map(e => e.responseEnd))) };
      });
      const loadFrames = await framesSince(page, t.firstDraw ?? 0, t.ride);
      // Riding: hold the throttle for five seconds after the first frame.
      await page.locator('#game').focus().catch(() => {});
      await page.keyboard.down('ArrowUp');
      const riding = await sample(page, cdp, 5000);
      await page.keyboard.up('ArrowUp');
      const loadLong = t.longTasks.filter(x => x[0] >= (t.firstDraw ?? 0) && x[0] < t.ride);
      report.scenarios.load[map] = {
        firstDrawMs: round(t.firstDraw),
        rideFrameMs: round(t.ride),
        loadMs: round(t.ride - (t.firstDraw ?? 0)),
        stages: t.status.map(([at, text]) => [at, text.slice(0, 60)]),
        loaderFrames: loadFrames.length,
        loaderLongestGapMs: round(Math.max(0, ...loadFrames.map(f => f.interval))),
        loadLongTasks: loadLong.length,
        loadLongTaskMs: sum(loadLong.map(x => x[1])),
        loadLongTaskMax: Math.max(0, ...loadLong.map(x => x[1])),
        programs: t.programs,
        uploads: t.uploads,
        uploadMB: round(t.uploadBytes / 1e6),
        resources,
        firstFiveSeconds: riding,
        errors
      };
      console.log('load', map, JSON.stringify(report.scenarios.load[map]));
      await page.context().close();
    }
  }
} finally {
  await browser.close();
}
await mkdir(out, { recursive: true });
await writeFile(`${out}/${label}.json`, JSON.stringify(report, null, 1));
console.log(`wrote ${out}/${label}.json`);
