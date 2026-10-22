// Host measurement of each Display → Graphics preset while riding. Phone
// emulation is 390x844 at DPR3 with touch and CDP CPU throttling (x4 by
// default); desktop is 1280x720 at DPR1 unthrottled. These are host Chromium
// numbers, not physical-phone GPU, thermal or battery results.
//
//   ATV_TEST_URL=http://127.0.0.1:4173 PLAYWRIGHT_MODULE=... node tools/benchmark_graphics_presets.mjs
// Options: ATV_BENCH_MAPS (env01,sx01,env04), ATV_BENCH_PRESETS (original,enhanced,high),
// ATV_BENCH_PROFILES (phone,desktop), ATV_BENCH_SECONDS (10), ATV_BENCH_THROTTLE (4),
// ATV_BENCH_OUTPUT (work/verification/graphics-presets).
import { mkdir, writeFile } from 'node:fs/promises';
import { base, jsBase, launch, newPage, waitForGame } from './browser/harness.mjs';
const maps = (process.env.ATV_BENCH_MAPS || 'env01,sx01,env04').split(',');
const presets = (process.env.ATV_BENCH_PRESETS || 'original,enhanced,high').split(',');
const profiles = (process.env.ATV_BENCH_PROFILES || 'phone,desktop').split(',');
const seconds = Number(process.env.ATV_BENCH_SECONDS || 10), throttle = Number(process.env.ATV_BENCH_THROTTLE || 4);
const out = process.env.ATV_BENCH_OUTPUT || 'work/verification/graphics-presets';
const PROFILES = {
  phone: { context: { viewport: { width: 390, height: 844 }, deviceScaleFactor: 3, hasTouch: true, isMobile: true }, throttle },
  desktop: { context: { viewport: { width: 1280, height: 720 }, deviceScaleFactor: 1 }, throttle: 1 }
};
const percentile = (values, p) => { const s = [...values].sort((a, b) => a - b); return s.length ? s[Math.min(s.length - 1, Math.floor(p * s.length))] : null; };
const mean = values => values.length ? values.reduce((a, b) => a + b, 0) / values.length : null;
const round = v => v === null ? null : Math.round(v * 100) / 100;
await mkdir(out, { recursive: true });
const browser = await launch();
const rows = [];
try {
  for (const profile of profiles) for (const map of maps) for (const preset of presets) {
    const errors = [];
    const page = await newPage(browser, PROFILES[profile].context, errors);
    await page.addInitScript(preset => {
      localStorage.setItem('atv-graphics', preset);
      localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
      const bench = window.__bench = { recording: false, intervals: [], callbacks: [], draws: [], ratios: [], frameDraws: 0, last: 0 };
      for (const name of ['drawElements', 'drawElementsInstanced', 'drawArrays', 'drawArraysInstanced']) {
        const original = WebGL2RenderingContext.prototype[name];
        WebGL2RenderingContext.prototype[name] = function (...args) { bench.frameDraws++; return original.apply(this, args); };
      }
      const raf = window.requestAnimationFrame.bind(window);
      window.requestAnimationFrame = callback => raf(time => {
        const started = performance.now(); callback(time); const spent = performance.now() - started;
        if (bench.recording) bench.callbacks.push(spent);
      });
      const tick = time => {
        if (bench.recording) {
          if (bench.last) bench.intervals.push(time - bench.last);
          bench.draws.push(bench.frameDraws); bench.ratios.push(window.atvTerrainStatus?.renderRatio ?? null);
        }
        bench.last = time; bench.frameDraws = 0; raf(tick);
      };
      raf(tick);
    }, preset);
    const cdp = await page.context().newCDPSession(page);
    await cdp.send('Performance.enable');
    await page.goto(`${jsBase}/play.html?drive=1&environment=${map}`);
    await waitForGame(page, { until: () => window.atvTerrainStatus.simulation });
    if (PROFILES[profile].throttle > 1) await cdp.send('Emulation.setCPUThrottlingRate', { rate: PROFILES[profile].throttle });
    await page.locator('#game').focus();
    await page.keyboard.down('KeyW');
    await page.waitForTimeout(2000); // Deliberately wall-clock: warm-up before sampling.
    const task = async () => Object.fromEntries((await cdp.send('Performance.getMetrics')).metrics.map(m => [m.name, m.value]));
    const before = await task();
    await page.evaluate(() => { const b = window.__bench; Object.assign(b, { recording: true, intervals: [], callbacks: [], draws: [], ratios: [] }); });
    await page.waitForTimeout(seconds * 1000); // Deliberately wall-clock: fixed sampling window.
    const sample = await page.evaluate(() => { const b = window.__bench; b.recording = false; const s = window.atvTerrainStatus; return { ...b, steps: s.simulationSteps, graphics: s.graphics, shadows: s.contactShadows, renderRatio: s.renderRatio, resolution: [document.querySelector('#game').width, document.querySelector('#game').height] }; });
    const after = await task();
    await page.keyboard.up('KeyW');
    if (PROFILES[profile].throttle > 1) await cdp.send('Emulation.setCPUThrottlingRate', { rate: 1 });
    const row = {
      profile, map, preset: sample.graphics?.preset ?? preset, frames: sample.intervals.length,
      fps: round(sample.intervals.length / seconds), frameMs: { mean: round(mean(sample.intervals)), p50: round(percentile(sample.intervals, .5)), p95: round(percentile(sample.intervals, .95)) },
      rafCallbackMs: { mean: round(mean(sample.callbacks)), p95: round(percentile(sample.callbacks, .95)) },
      mainThreadBusy: round((after.TaskDuration - before.TaskDuration) / seconds),
      drawCalls: { mean: round(mean(sample.draws)), max: Math.max(...sample.draws) },
      renderRatio: { end: sample.renderRatio, min: Math.min(...sample.ratios.filter(Number.isFinite)) },
      resolution: sample.resolution, shadows: sample.shadows, anisotropy: sample.graphics?.anisotropyApplied ?? 0, errors
    };
    rows.push(row);
    console.log(JSON.stringify(row));
    await page.close();
  }
} finally {
  await browser.close();
  await writeFile(`${out}/benchmark.json`, JSON.stringify({ scope: 'Host Chromium; phone = 390x844 DPR3 touch emulation with CDP CPU throttle; not physical-device GPU/thermal/battery evidence', seconds, throttle, rows }, null, 2));
}
