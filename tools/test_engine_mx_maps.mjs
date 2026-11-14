// Explicit private-content MX acceptance pass. Defaults to all 56 imported
// venues; ATV_MX_MAPS=mx_fs01,mx_nat01 selects a diagnostic subset.
// Serve the staged content package with tools/serve_web.py --content PATH and
// pass that host URL to tools/run_browser_tests.mjs --url. Host mobile viewport
// results do not establish physical-phone performance.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { assertNoErrors, launch, newPage, optionalScreenshot, url, writeJson } from './browser/harness.mjs';

const catalog = JSON.parse(await readFile(new URL('../research/mx-map-catalog.json', import.meta.url), 'utf8'));
const selected = process.env.ATV_MX_MAPS?.split(',');
const courses = selected ? catalog.filter(c => selected.includes(c.id)) : catalog;
assert.ok(courses.length && (!selected || courses.length === selected.length), 'known, nonempty MX selection');
const browser = await launch();
const report = [];
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 720, height: 405 }, hasTouch: true }, errors);
  const session = await page.context().newCDPSession(page);
  await session.send('Performance.enable');
  await page.addInitScript(() => {
    const samples = window.mxFrameSamples = [];
    const raf = window.requestAnimationFrame.bind(window);
    let previous;
    window.requestAnimationFrame = callback => raf(time => {
      const started = performance.now();
      callback(time);
      const elapsed = performance.now() - started;
      const state = document.querySelector('#ui-state');
      if (state && JSON.parse(state.textContent || '{}').step > 0 && samples.length < 10000) {
        if (previous !== undefined && time > previous) samples.push([time - previous, elapsed]);
        previous = time;
      }
    });
    localStorage.setItem('furyScript', JSON.stringify([
      { frames: 120, input: {} },
      { frames: 240, input: { throttle: 0.4 } },
    ]));
  });
  for (const [index, course] of courses.entries()) {
    const started = Date.now();
    const quad = index % 2 ? 29 : 1;
    await page.goto(url(`/play.html?venue=${course.id}&quad=${quad}&graphics=original&weather=off&uiDiagnostics=1&traceAll=1&script=storage`));
    await page.waitForFunction(() => document.querySelector('#trace') || /error|fail/i.test(document.querySelector('#status')?.textContent), null, { timeout: 240000 });
    const status = await page.locator('#status').textContent();
    assert.ok(await page.locator('#trace').count(), `${course.id}: ${status}`);
    const trace = JSON.parse(await page.locator('#trace').textContent());
    assert.equal(trace.length, 360, course.id);
    assert.ok(trace.every(f => f.origin.every(Number.isFinite) && Number.isFinite(f.speed)), `${course.id}: finite simulation`);
    assert.ok(trace.some(f => f.grounded.some(Boolean)), `${course.id}: grounded contact`);
    const state = await page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
    assert.ok(state.scenery?.triangles > 0, `${course.id}: scenery renders`);
    const samples = await page.evaluate(() => window.mxFrameSamples);
    const percentile = (column, fraction) => {
      const sorted = samples.map(row => row[column]).sort((a, b) => a - b);
      return sorted.length ? sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * fraction))] : null;
    };
    const { metrics } = await session.send('Performance.getMetrics');
    const observation = { id: course.id, quad, host_load_and_360_steps_ms: Date.now() - started,
      scenery_draws: state.scenery.draws, scenery_triangles: state.scenery.triangles,
      host_frame_samples: samples.length, host_frame_interval_p50_ms: percentile(0, 0.5),
      host_frame_interval_p95_ms: percentile(0, 0.95), host_raf_callback_p95_ms: percentile(1, 0.95),
      // JS heap excludes GPU allocations and is not total process/Wasm memory.
      host_js_heap_bytes: metrics.find(m => m.name === 'JSHeapUsedSize')?.value ?? null,
      max_speed: Math.max(...trace.map(f => f.speed)), final: trace.at(-1).origin };
    report.push(observation);
    console.log(JSON.stringify(observation));
    await optionalScreenshot(page, 'ATV_MX_SCREENSHOT', course.id);
    assertNoErrors(errors);
  }
  if (process.env.ATV_MX_REPORT) await writeJson(process.env.ATV_MX_REPORT, report);
  console.log(`PASS: ${courses.length} MX maps loaded, rendered and retained vehicle contact at a host mobile viewport`);
} finally {
  await browser.close();
}
