// Opt-in original Caribou ramp: preload takeoff and clean landing without a stunt.
// Drives only through actual Chromium touch events; no physics/state injection.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { writeFile, mkdir } from 'node:fs/promises';
import { captureSimulation } from './capture_simulation.mjs';
const { chromium } = createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || 'playwright');
await mkdir('work/verification', {recursive: true});
const browser = await chromium.launch({ headless: true });
try {
  const page = await browser.newPage({ viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true });
  await page.addInitScript(() => localStorage.setItem('atv-mobile-one-handed', 'false')); // Explicit optional two-finger mode.
  await captureSimulation(page, 'rampFrames');
  const errors = []; page.on('pageerror', e => errors.push(e.message));
  const url = new URL('/play.html', process.env.ATV_TEST_URL || 'http://127.0.0.1:4173');
  url.searchParams.set('drive', '1'); url.searchParams.set('environment', 'env01');
  await page.goto(url.href);
  await page.waitForFunction(() => window.atvTerrainStatus?.simulation, null, { timeout: 90000 });
  await page.bringToFront(); await page.locator('#game').focus();
  if (process.env.ATV_DIAGNOSTICS === 'debug') {
    for (let i = 0; i < 3 && (await page.evaluate(() => window.atvTerrainStatus.performance.mode)) !== 'debug'; i++) await page.keyboard.press('F3');
  }
  const cdp = await page.context().newCDPSession(page);
  const touch = (type, points = []) => cdp.send('Input.dispatchTouchEvent', { type, touchPoints: points.map(point => ({ radiusX: 6, radiusY: 6, force: 1, ...point })) });
  const left = { id: 1, x: 60, y: 560 };
  const right = { id: 2, x: 310, y: 530 }, drive = { ...right, y: 460 };
  await touch('touchStart', [left, right]); await touch('touchMove', [left, drive]);
  await page.waitForFunction(() => window.atvTerrainStatus.frame.preload_armed, null, { timeout: 45000, polling: 'raf' });
  // Release throttle and preload together; the neutral left thumb adds no drive.
  await touch('touchEnd', [drive]);
  await page.waitForFunction(() => window.rampFrames.some(s => s.frame.preload_active), null, { timeout: 5000, polling: 'raf' });
  await page.waitForFunction(() => !window.atvTerrainStatus.frame.grounded.some(Boolean), null, { timeout: 5000, polling: 'raf' });

  await touch('touchCancel');
  await page.waitForFunction(() => window.atvTerrainStatus.frame.grounded.some(Boolean),null,{timeout:10000,polling:'raf'});
  await page.waitForTimeout(1000);
  const { frames, final, steps } = await page.evaluate(() => ({
    frames: window.rampFrames, final: window.atvTerrainStatus.frame,
    steps: window.atvTerrainStatus.simulationSteps,
  }));
  await writeFile(process.env.ATV_LANDING_TRACE || 'work/verification/clean-landing-trace.json',JSON.stringify(frames,null,2));
  if (process.env.ATV_LANDING_SCREENSHOT) await page.screenshot({path:process.env.ATV_LANDING_SCREENSHOT});
  assert(frames.some(s=>!s.frame.grounded.some(Boolean)), 'A real airborne interval must occur');
  assert(frames.filter(s=>s.frame.preload_active).every(s=>s.input.throttle===0), 'Right release must drop throttle during the jump');
  assert(frames.every(s=>s.frame.wreck.phase==='Riding'), 'A clean hop must never enter wreck state');
  assert.equal(final.wreck.phase,'Riding');
  assert.equal(final.reset_count,0);
  assert.equal(frames.length, steps, 'Record every delivered batch step');
  assert.deepEqual(errors, []);
  assert.equal(await page.evaluate(() => window.atvTerrainStatus.glError), 0);
  console.log('PASS clean preload takeoff and landing without stunt or wreck');
} finally {await browser.close();}
