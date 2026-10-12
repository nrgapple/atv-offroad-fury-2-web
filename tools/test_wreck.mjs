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
  await page.waitForFunction(() => window.atvTerrainStatus?.simulation, null, { timeout: 90000 });
  await page.bringToFront(); await page.locator('#game').focus();
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
        window.rampFrames.push({ step: previous, frame: state.frame, input: state.input, camera: state.followCamera });
      }
      requestAnimationFrame(record);
    }
    requestAnimationFrame(record);
  });
  await touch('touchStart', [left, right]); await touch('touchMove', [left, drive]);
  await page.waitForFunction(() => window.atvTerrainStatus.frame.preload_armed, null, { timeout: 45000, polling: 'raf' });
  // Right release lifts throttle and releases preload; left stays neutral.
  await touch('touchEnd', [drive]);
  await page.waitForFunction(() => window.rampFrames.some(s => s.frame.preload_active), null, { timeout: 5000, polling: 'raf' });
  await page.waitForFunction(() => !window.atvTerrainStatus.frame.grounded.some(Boolean), null, { timeout: 5000, polling: 'raf' });
  // Primary right is always drive/preload. An extra right finger owns stunts.
  const stunt = { id: 3, x: 230, y: 530 };
  await touch('touchStart', [left, right]);
  await touch('touchStart', [left, right, stunt]);
  const swipe = { ...stunt, y: stunt.y - 70 };
  await touch('touchMove', [left, right, swipe]);

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
  await touch('touchStart',[right]); await touch('touchMove',[drive]);
  await page.waitForFunction(() => Math.hypot(...window.atvTerrainStatus.frame.linear_velocity) > 2, null, {timeout:10000,polling:'raf'});
  await touch('touchCancel');
  const frames = await page.evaluate(() => window.rampFrames);
  assert(frames.some(s => s.frame.stunt?.id === 0));
  assert(frames.some(s => s.frame.stunt_score?.active && s.frame.stunt_score.pending_points > 0), 'Accepted stunt must enter native scoring');
  assert.equal(reset.stunt_score.active, false);
  assert.equal(reset.stunt_score.pending_points, 0);
  assert.equal(reset.stunt_score.committed_points, 0, 'A failed landing must not award the displayed pending score');
  assert(frames.some(s => s.frame.hud?.stunt_name === 'Nac Nac' && s.frame.hud.stunt_visible), 'Native stunt HUD must consume the scoring callbacks');
  assert.equal(await page.locator('#native-hud').getAttribute('data-renderer'), 'rust');
  const detached = frames.map(s => s.frame.rider_wreck).filter(f => f && f.phase !== 'attached');
  assert(detached.length > 0);
  assert(detached.every(f => f.center_of_mass.every(Number.isFinite)));
  assert(detached.some(f => Math.hypot(...f.center_of_mass) > 0.05), 'Detached rider must use animated contact bounds instead of a fixed pelvis center');
  assert.equal(start.wreck.kind, 'Vertical');
  assert(frames.some(s => s.camera?.mode === 'wreck'), 'Original wreck camera must run during separation');
  assert(frames.filter(s => s.camera?.mode === 'wreck').every(s => [...s.camera.position, ...s.camera.target, s.camera.fov].every(Number.isFinite)), 'Wreck camera must remain finite');
  await writeFile('work/verification/wreck-browser-trace.json',JSON.stringify({start,reset,frames,errors},null,2));
  assert.deepEqual(errors, []);
  assert.equal(await page.evaluate(() => window.atvTerrainStatus.glError), 0);
  console.log('PASS: real touch Nac Nac → vertical wreck → original separation/ground clip → terrain contact → automatic reset → resumed driving.');
} finally { await browser.close(); }
