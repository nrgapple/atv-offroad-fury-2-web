// The diagnostics overlay in the Rust engine (web/diagnostics.js): Off by
// default; F3 cycles Performance (frame statistics and the former #status
// strip line) and Debug (vehicle state), saved under atv.diagnostics.v1;
// ?debug=1 / ?debug=2 open Performance / Debug. The #status element stays
// hidden (its text keeps updating) unless a fatal error shows it. The
// overlay is drawn by the Rust UI. Host browser evidence.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || 'null'));
const until = (page, predicate, arg) => page.waitForFunction(predicate, arg, { timeout: 60000 });
const mode = page => read(page).then(s => s?.diagnostics);
const statusShown = page => page.evaluate(() => getComputedStyle(document.getElementById('status')).display !== 'none');
async function f3(page) { await page.keyboard.down('F3'); await nextFrames(page, 2); await page.keyboard.up('F3'); await nextFrames(page, 20); }

const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await page.addInitScript(() => localStorage.setItem('atv2-keyboard-help-seen-v1', '1'));
  await page.goto(url('/engine.html?venue=env01&uiDiagnostics=1'));
  await page.waitForSelector('#ui-state', { state: 'attached', timeout: 120000 });
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).diagnostics);
  assert.deepEqual(await mode(page), { mode: 'off', lines: [] });
  await until(page, () => /fps/.test(document.getElementById('status').textContent));
  assert.equal(await statusShown(page), false, '#status stays hidden');
  await page.mouse.click(640, 400);
  await f3(page);
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).diagnostics.lines.some(l => /fps/.test(l)));
  let d = await mode(page);
  assert.equal(d.mode, 'performance');
  assert.match(d.lines[0], /^PERFORMANCE · /);
  assert.ok(d.lines.some(l => l.startsWith('FPS ')), d.lines.join('\n'));
  assert.equal(await page.evaluate(() => localStorage.getItem('atv.diagnostics.v1')), 'performance');
  assert.equal(await statusShown(page), false);
  if (process.env.ATV_SHOT) await page.screenshot({ path: `${process.env.ATV_SHOT}-performance.png` });
  await f3(page);
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).diagnostics.lines.some(l => l.startsWith('Position ft')));
  d = await mode(page);
  assert.equal(d.mode, 'debug');
  if (process.env.ATV_SHOT) await page.screenshot({ path: `${process.env.ATV_SHOT}-debug.png` });
  await f3(page);
  assert.deepEqual(await mode(page), { mode: 'off', lines: [] });
  assert.equal(await page.evaluate(() => localStorage.getItem('atv.diagnostics.v1')), 'off');
  await page.close();
  // URL modes, on a ride and on the frontend.
  for (const [query, expected] of [['venue=env01&debug=1', 'performance'], ['venue=env01&debug=2', 'debug'], ['debug=1', 'performance']]) {
    const p = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
    await p.goto(url(`/engine.html?${query}&uiDiagnostics=1`));
    await until(p, expected => { const s = JSON.parse(document.querySelector('#ui-state')?.textContent || 'null'); return s?.diagnostics?.mode === expected && s.diagnostics.lines.length > 0; }, expected);
    assert.equal(await statusShown(p), false);
    await p.close();
  }
  assertNoErrors(errors);
  console.log('PASS: Rust diagnostics overlay: Off by default, F3 cycles Performance and Debug, ?debug=1/2, #status hidden');
} finally {
  await browser.close();
}
