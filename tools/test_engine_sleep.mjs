// Inactive single-player render sleep in the Rust engine, as web/game.js
// render() does: no frame is scheduled while paused, unfocused or hidden, and
// lifecycle/input events wake the ride. Counts animation-frame callbacks,
// simulation steps and Chrome main-thread task time over two-second windows
// for active, paused, blurred and hidden states (host Chromium, not a phone).
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, url } from './browser/harness.mjs';

const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 960, height: 540 } }, errors);
  await page.addInitScript(() => {
    const counter = window.frameCount = { frames: 0 };
    const raf = window.requestAnimationFrame.bind(window);
    window.requestAnimationFrame = callback => raf(time => { counter.frames++; callback(time); });
  });
  const session = await page.context().newCDPSession(page);
  await session.send('Performance.enable');
  await page.goto(url('/engine.html?venue=env01&uiDiagnostics=1'));
  await page.waitForFunction(() => /\d+ fps/.test(document.querySelector('#status').textContent), null, { timeout: 120000 });
  await page.locator('canvas').click();
  const sample = async () => {
    const { metrics } = await session.send('Performance.getMetrics');
    const ui = JSON.parse(await page.locator('#ui-state').textContent());
    return { task: metrics.find(m => m.name === 'TaskDuration').value * 1000, frames: await page.evaluate(() => window.frameCount.frames), step: ui.step };
  };
  const window2s = async () => {
    const a = await sample();
    await page.waitForTimeout(2000);
    const b = await sample();
    return { frames: b.frames - a.frames, steps: b.step - a.step, taskMs: Math.round(b.task - a.task) };
  };
  const visibility = value => page.evaluate(value => {
    Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => value });
    document.dispatchEvent(new Event('visibilitychange'));
  }, value);
  // Pause is read on the next simulation batch: hold the key briefly.
  const press = async key => { await page.keyboard.down(key); await page.waitForTimeout(100); await page.keyboard.up(key); };
  const report = {};
  report.active = await window2s();
  await press('Escape');
  await page.waitForTimeout(200);
  report.paused = await window2s();
  await press('Escape');
  await page.waitForTimeout(200);
  report.resumed = await window2s();
  // game.js pauses on the window's blur and focus (not the canvas's).
  await page.evaluate(() => window.dispatchEvent(new Event('blur')));
  await page.waitForTimeout(200);
  report.blurred = await window2s();
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await visibility('hidden');
  await page.waitForTimeout(200);
  report.hidden = await window2s();
  await visibility('visible');
  await page.waitForTimeout(200);
  report.visible = await window2s();
  console.log(JSON.stringify(report));
  assert.ok(report.active.frames >= 60 && report.active.steps >= 100, 'active ride renders and steps');
  for (const state of ['paused', 'blurred', 'hidden']) {
    assert.ok(report[state].frames <= 2, `${state}: no frames scheduled (${report[state].frames})`);
    assert.equal(report[state].steps, 0, `${state}: simulation stopped`);
  }
  for (const state of ['resumed', 'visible']) assert.ok(report[state].frames >= 60 && report[state].steps >= 100, `${state}: wakes`);
  assertNoErrors(errors);
  console.log('PASS: Rust ride sleeps while paused, blurred or hidden and wakes on resume');
} finally {
  await browser.close();
}
