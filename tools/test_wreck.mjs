// Opt-in original Caribou ramp: failed stunt, detached rider and automatic recovery.
// Drives only through actual Chromium touch events; no physics/state injection.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { writeFile, mkdir } from 'node:fs/promises';
const { chromium } = createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || 'playwright');
await mkdir('work/verification', {recursive: true});
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
  const left = { id: 1, x: 60, y: 560 }, drive = { ...left, y: 490 };
  const right = { id: 2, x: 310, y: 530 };
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
  await touch('touchStart', [left]); await touch('touchMove', [drive]); await touch('touchStart', [drive, right]);
  await page.waitForFunction(() => window.atvTerrainStatus.frame.preload_armed, null, { timeout: 45000, polling: 'raf' });
  // CDP touchEnd carries the released finger, preserving the left throttle.
  await touch('touchEnd', [right]);
  await page.waitForFunction(() => window.rampFrames.some(s => s.frame.preload_active), null, { timeout: 5000, polling: 'raf' });
  await page.waitForFunction(() => !window.atvTerrainStatus.frame.grounded.some(Boolean), null, { timeout: 5000, polling: 'raf' });
  // Start a NEW airborne right swipe, distinct from the grounded preload hold.
  await touch('touchStart', [drive, right]);
  const swipe = { ...right, y: right.y - 70 };
  await touch('touchMove', [drive, swipe]);

  await page.waitForFunction(() => window.atvTerrainStatus.frame.rider_mode === 'wreck', null, {timeout: 15000,polling:'raf'});
  const start = await page.evaluate(() => window.atvTerrainStatus.frame);
  await touch('touchCancel');
  await page.waitForFunction(() => window.atvTerrainStatus.frame.rider_wreck && window.atvTerrainStatus.frame.rider_wreck.phase !== 'attached', null, {timeout: 10000,polling:'raf'});
  await page.screenshot({path:'work/verification/rider-wreck-separation.png'});
  await page.waitForFunction(() => window.atvTerrainStatus.frame.rider_wreck?.grounded, null, {timeout: 10000,polling:'raf'});
  await page.screenshot({path:'work/verification/rider-wreck-ground.png'});
  await page.waitForFunction(count => window.atvTerrainStatus.frame.reset_count > count, start.reset_count, {timeout:15000,polling:'raf'});
  const reset = await page.evaluate(() => window.atvTerrainStatus.frame);
  assert.equal(reset.wreck.phase, 'Riding');
  assert.equal(reset.rider_wreck, undefined);
  assert.equal(reset.stunt, null);
  await touch('touchStart',[left]); await touch('touchMove',[drive]);
  await page.waitForFunction(() => Math.hypot(...window.atvTerrainStatus.frame.linear_velocity) > 2, null, {timeout:10000,polling:'raf'});
  await touch('touchCancel');
  const frames = await page.evaluate(() => window.rampFrames);
  assert(frames.some(s => s.frame.stunt?.id === 0));
  assert.equal(start.wreck.kind, 'Vertical');
  await writeFile('work/verification/wreck-browser-trace.json',JSON.stringify({start,reset,frames,errors},null,2));
  assert.deepEqual(errors, []);
  assert.equal(await page.evaluate(() => window.atvTerrainStatus.glError), 0);
  console.log('PASS: real touch Nac Nac → vertical wreck → original separation/ground clip → terrain contact → automatic reset → resumed driving.');
} finally { await browser.close(); }
