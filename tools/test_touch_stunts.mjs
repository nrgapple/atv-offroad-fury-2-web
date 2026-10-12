// Opt-in short Caribou hop: original stunt entry/release and unfinished-landing wreck.
// Main/held-motion timing is covered by core tests; this hop is too short to hold.
// Drives only through actual Chromium touch events; no physics/state injection.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { writeFile } from 'node:fs/promises';
const { chromium } = createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || 'playwright');
const browser = await chromium.launch({ headless: true });
try {
  const page = await browser.newPage({ viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true });
  const errors = []; page.on('pageerror', e => errors.push(e.message));
  const url = new URL('/play.html', process.env.ATV_TEST_URL || 'http://127.0.0.1:4173');
  url.searchParams.set('drive', '1'); url.searchParams.set('environment', 'env01');
  await page.goto(url.href);
  await page.waitForFunction(() => window.atvTerrainStatus?.simulation && !document.querySelector('#touch-controls').hidden, null, { timeout: 90000 });
  const cdp = await page.context().newCDPSession(page);
  const touch = (type, points = []) => cdp.send('Input.dispatchTouchEvent', { type, touchPoints: points.map(point => ({ radiusX: 6, radiusY: 6, force: 1, ...point })) });
  const left = { id: 1, x: 60, y: 560 };
  const right = { id: 2, x: 310, y: 530 }, drive = { ...right, y: 460 };
  await page.evaluate(() => {
    window.rampFrames = []; let previous = 0;
    function record() {
      const state = window.atvTerrainStatus;
      if (state.simulationSteps !== previous) {
        previous = state.simulationSteps;
        window.rampFrames.push({ step: previous, frame: state.frame, input: state.input });
      }
      requestAnimationFrame(record);
    }
    requestAnimationFrame(record);
  });
  await touch('touchStart', [left, right]); await touch('touchMove', [left, drive]);
  await page.waitForFunction(() => window.atvTerrainStatus.frame.preload_armed, null, { timeout: 45000, polling: 'raf' });
  const armed = await page.evaluate(() => window.atvTerrainStatus.frame);
  // CDP touchEnd carries the right finger; throttle and preload release together.
  await touch('touchEnd', [drive]);
  await page.waitForFunction(() => window.rampFrames.some(s => s.frame.preload_active), null, { timeout: 5000, polling: 'raf' });
  await page.waitForFunction(() => !window.atvTerrainStatus.frame.grounded.some(Boolean), null, { timeout: 5000, polling: 'raf' });
  const before = await page.evaluate(() => window.atvTerrainStatus.frame);
  // Primary right retains drive/preload; secondary airborne right selects trick.
  await touch('touchStart', [left, right]);
  const trick = { id: 3, x: 230, y: 530 }, swipe = { ...trick, y: 460 };
  await touch('touchStart', [left, right, trick]);
  await touch('touchMove', [left, right, swipe]);
  await page.waitForFunction(() => window.atvTerrainStatus.frame.stunt?.id === 0, null, { timeout: 5000, polling: 'raf' });
  await page.waitForFunction(() => window.atvTerrainStatus.frame.stunt?.main_time > .05, null, { timeout: 5000, polling: 'raf' });
  const performing = await page.evaluate(() => window.atvTerrainStatus.frame);
  assert.equal(performing.rider_mode, 'stunt');
  assert.notDeepEqual(performing.rider_node_matrices, before.rider_node_matrices, 'Original Nac Nac MOT must change the actual rendered rider pose');
  // Release while the main motion is still playing. This short hop cannot
  // reach the original 0.92-second hold threshold before landing.
  await touch('touchEnd', [swipe]);
  await page.waitForFunction(time => window.rampFrames.some(s => s.input.stunt_modifiers === 0 && s.frame.stunt?.id === 0 && !s.frame.stunt.holding && s.frame.stunt.main_time > time), performing.stunt.main_time, { timeout: 5000, polling: 'raf' });
  if (process.env.ATV_TOUCH_STUNT_SCREENSHOT) await page.screenshot({ path: process.env.ATV_TOUCH_STUNT_SCREENSHOT });
  await page.waitForFunction(() => window.atvTerrainStatus.frame.rider_mode === 'wreck', null, { timeout: 10000, polling: 'raf' });
  const landing = await page.evaluate(() => window.atvTerrainStatus.frame);
  assert.equal(landing.wreck.kind, 'Vertical', 'An unfinished original stunt at wheel contact must enter the native landing wreck');
  await touch('touchCancel');
  const frames = await page.evaluate(() => window.rampFrames);
  assert(frames.some(s => s.frame.stunt && !s.frame.grounded.some(Boolean)));
  assert(frames.some(s => s.frame.preload_active));
  assert.deepEqual(errors, []);
  if (process.env.ATV_TOUCH_STUNT_TRACE) await writeFile(process.env.ATV_TOUCH_STUNT_TRACE, JSON.stringify({ armed, before, performing, landing, frames, errors }, null, 2));
  console.log('PASS: real Caribou short hop enters original Nac Nac, changes rider pose, releases modifier and advances main motion, then unfinished landing triggers native Vertical wreck. No held-stunt completion claim or injected state/forces.');
} finally { await browser.close(); }
