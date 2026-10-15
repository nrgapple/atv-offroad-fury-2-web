// Opt-in original Caribou ramp: failed stunt, detached rider and automatic recovery.
// Drives only through actual Chromium touch events; no physics/state injection.
import assert from 'node:assert/strict';
import { writeFile, mkdir } from 'node:fs/promises';
import { captureSimulation } from './capture_simulation.mjs';
import { captureMinimapTracking } from './minimap_tracking.mjs';
import {
  captureCrashIsolation,
  verifyCrashIsolation,
  captureCrashDraws,
  verifyCrashImpacts
} from './crash_isolation.mjs';
import { assertNoErrors, base, button, launch, newPage, waitForSteps } from './browser/harness.mjs';
await mkdir('work/verification', { recursive: true });
const screenshotPrefix = process.env.ATV_WRECK_SCREENSHOT_PREFIX || 'work/verification/rider-wreck';
const browser = await launch();
try {
  const errors = [];
  const page = await newPage(
    browser,
    { viewport: { width: Number(process.env.ATV_WRECK_WIDTH || 390), height: 844 }, hasTouch: true, isMobile: true },
    errors
  );
  await page.addInitScript(enabled => {
    localStorage.setItem('atv-mobile-one-handed', 'false');
    localStorage.setItem('atv-funny-crashes', String(enabled));
  }, process.env.ATV_FUNNY_CRASHES === '1'); // Explicit optional two-finger mode.
  if (process.env.ATV_CRASH_MODE === 'gorn')
    await page.addInitScript(() => localStorage.setItem('atv-crash-mode', 'gorn'));
  if (process.env.ATV_CRASH_ISOLATION === '1') await captureCrashIsolation(page);
  await captureCrashDraws(page);
  await captureMinimapTracking(page);
  await captureSimulation(page, 'rampFrames');
  const url = new URL('/play.html', base);
  url.searchParams.set('drive', '1');
  url.searchParams.set('environment', 'env01');
  if (process.env.ATV_WRECK_QUAD || process.env.ATV_WRECK_RIDER) {
    await page.goto(new URL('/play.html', url).href);
    await button(page, 'Press START').click({ timeout: 120000 });
    await button(page, 'Single Player').click();
    await button(page, 'Quad and Rider').click();
    for (let i = 1; i < Number(process.env.ATV_WRECK_QUAD || 1); i++) await button(page, 'Next quad').click();
    if (process.env.ATV_WRECK_RIDER === '1') await button(page, 'Change rider').click();
    await button(page, 'Done').click();
    await button(page, 'Freestyle').click();
    await button(page, 'Ride Caribou Peak').click();
  } else await page.goto(url.href);
  await page.waitForFunction(() => window.atvTerrainStatus?.simulation, null, { timeout: 90000 });
  await page.bringToFront();
  await page.locator('#game').focus();
  // Native actor reset starts a two-second stunt cooldown. Wait in simulation
  // steps; renderer startup speed must not decide whether the gesture is legal.
  await waitForSteps(page, 125, { timeout: 10000 });
  const cdp = await page.context().newCDPSession(page);
  const touch = (type, points = []) =>
    cdp.send('Input.dispatchTouchEvent', {
      type,
      touchPoints: points.map(point => ({ radiusX: 6, radiusY: 6, force: 1, ...point }))
    });
  const left = { id: 1, x: 60, y: 560 };
  const right = { id: 2, x: Number(process.env.ATV_WRECK_WIDTH || 390) - 80, y: 530 },
    drive = { ...right, y: 460 };
  await touch('touchStart', [left, right]);
  await touch('touchMove', [left, drive]);
  await page.waitForFunction(() => window.atvTerrainStatus.frame.preload_armed, null, {
    timeout: 45000,
    polling: 'raf'
  });
  // Right release lifts throttle and releases preload; left stays neutral.
  await touch('touchEnd', [drive]);
  await page.waitForFunction(() => window.rampFrames.some(s => s.frame.preload_active), null, {
    timeout: 5000,
    polling: 'raf'
  });
  await page.waitForFunction(() => !window.atvTerrainStatus.frame.grounded.some(Boolean), null, {
    timeout: 5000,
    polling: 'raf'
  });
  // The primary right thumb switches to tricks after takeoff.
  await touch('touchStart', [left, right]);
  const swipe = { ...right, y: right.y - 70 };
  await touch('touchMove', [left, swipe]);

  await page
    .waitForFunction(() => window.atvTerrainStatus.frame.rider_mode === 'wreck', null, {
      timeout: 15000,
      polling: 'raf'
    })
    .catch(async error => {
      console.error(
        'WRECK_DIAGNOSTIC',
        JSON.stringify(
          await page.evaluate(() => ({
            status: document.querySelector('#status').textContent,
            simulation: window.atvTerrainStatus.simulation,
            steps: window.atvTerrainStatus.simulationSteps,
            recent: window.rampFrames
              .slice(-10)
              .map(s => ({
                step: s.step,
                input: s.input,
                grounded: s.frame.grounded,
                stunt: s.frame.stunt,
                origin: s.frame.origin,
                preload_active: s.frame.preload_active
              }))
          }))
        )
      );
      throw error;
    });
  const start = await page.evaluate(() => window.atvTerrainStatus.frame);
  await touch('touchCancel');
  await page.waitForFunction(
    () => window.atvTerrainStatus.frame.rider_wreck && window.atvTerrainStatus.frame.rider_wreck.phase !== 'attached',
    null,
    { timeout: 10000, polling: 'raf' }
  );
  if (process.env.ATV_FUNNY_CRASHES === '1')
    await page.waitForFunction(
      () =>
        window.atvTerrainStatus.frame.funny_crash?.rider_node_matrices.length > 0 &&
        window.atvTerrainStatus.frame.funny_crash.age > 0.35
    );
  await page.screenshot({ path: `${screenshotPrefix}-separation.png` });
  if (process.env.ATV_FUNNY_CRASHES === '1') {
    await page.waitForFunction(() => crashDraws.quad > 0 && crashDraws.rider > 0);
    await page.keyboard.press('Escape');
    const initialMode = process.env.ATV_CRASH_MODE === 'gorn' ? 'Crashes: Gorn (blood & limbs)' : 'Crashes: Crazy';
    await button(page, initialMode).waitFor();
    const paused = await page.evaluate(() => window.atvTerrainStatus.frame.funny_crash);
    await page.waitForTimeout(250); // Time-based on purpose: proves paused crash physics stays frozen over wall-clock time.
    assert.deepEqual(
      await page.evaluate(() => window.atvTerrainStatus.frame.funny_crash),
      paused,
      'Paused crash physics must freeze'
    );
    const before = await page.evaluate(() => crashDraws.baseline);
    if (process.env.ATV_CRASH_MODE !== 'gorn') await button(page, 'Crashes: Crazy').click();
    await button(page, 'Crashes: Gorn (blood & limbs)').click();
    assert.equal(await page.evaluate(() => window.atvTerrainStatus.funnyCrashes), false);
    await button(page, 'Resume').click();
    await page.waitForFunction(before => crashDraws.baseline > before, before);
    await page.keyboard.press('Escape');
    await button(page, 'Crashes: Original').click();
    await button(page, 'Resume').click();
  }
  await page.waitForFunction(() => window.atvTerrainStatus.frame.rider_wreck?.grounded, null, {
    timeout: 10000,
    polling: 'raf'
  });
  await page.waitForFunction(() => window.minimapTracking.samples.some(s => s.actor !== 'vehicle'), null, {
    timeout: 10000
  });
  await page.screenshot({ path: `${screenshotPrefix}-ground.png` });
  await page.waitForFunction(count => window.atvTerrainStatus.frame.reset_count > count, start.reset_count, {
    timeout: 15000,
    polling: 'raf'
  });
  const reset = await page.evaluate(() => window.atvTerrainStatus.frame);
  assert.equal(reset.wreck.phase, 'Riding');
  assert.equal(reset.rider_wreck, undefined);
  assert.equal(reset.stunt, null);
  await touch('touchStart', [right]);
  await touch('touchMove', [drive]);
  await page.waitForFunction(() => Math.hypot(...window.atvTerrainStatus.frame.linear_velocity) > 2, null, {
    timeout: 10000,
    polling: 'raf'
  });
  await touch('touchCancel');
  const frames = await page.evaluate(() => window.rampFrames);
  assert(frames.some(s => s.frame.stunt?.id === 0));
  assert(
    frames.some(s => s.frame.stunt_score?.active && s.frame.stunt_score.pending_points > 0),
    'Accepted stunt must enter native scoring'
  );
  assert.equal(reset.stunt_score.active, false);
  assert.equal(reset.stunt_score.pending_points, 0);
  assert.equal(reset.stunt_score.committed_points, 0, 'A failed landing must not award the displayed pending score');
  assert(
    frames.some(s => s.frame.hud?.stunt_name === 'Nac Nac' && s.frame.hud.stunt_visible),
    'Native stunt HUD must consume the scoring callbacks'
  );
  assert.equal(await page.locator('#native-hud').getAttribute('data-renderer'), 'rust');
  if (process.env.ATV_FUNNY_CRASHES === '1') {
    const effects = frames.map(s => s.frame.funny_crash).filter(Boolean);
    verifyCrashImpacts(frames);
    assert(
      effects.some(f => f.rider_node_matrices.length > 0),
      'Detached rider must become articulated ragdoll'
    );
    assert(
      effects.every(f => [...f.quad_node_matrices.flat(), ...f.rider_node_matrices.flat()].every(Number.isFinite))
    );
    assert(!reset.funny_crash, 'Recovery must rebuild the intact quad and rider');
    await page.keyboard.press('Escape');
    await button(page, 'Crashes: Crazy').click();
    await button(page, 'Crashes: Gorn (blood & limbs)').click();
    assert.equal(await page.evaluate(() => localStorage.getItem('atv-funny-crashes')), 'false');
    await button(page, 'Resume').click();
    await page.waitForFunction(() => !window.atvTerrainStatus.frame.funny_crash);
  } else
    assert(
      frames.every(s => !s.frame.funny_crash),
      'Original baseline must have no enhanced pose'
    );
  const riders = frames.map(s => s.frame.rider_wreck).filter(Boolean);
  assert(
    riders.every(f => f.scene_query?.cached),
    'Every active wreck phase must borrow the quad scene cache'
  );
  assert(
    riders.every(
      f =>
        f.scene_query.cached_nodes <= 256 &&
        f.scene_query.radius >= 0 &&
        [...f.scene_query.center, f.scene_query.radius].every(Number.isFinite)
    ),
    'Cached rider bounds and membership must remain valid'
  );
  assert(
    riders.some(f => f.phase === 'attached'),
    'The shared cache must also run before separation'
  );
  assert(
    riders.some(f => f.grounded),
    'The shared cache must remain bound through ground contact'
  );
  const detached = riders.filter(f => f.phase !== 'attached');
  assert(detached.length > 0);
  assert(detached.every(f => f.center_of_mass.every(Number.isFinite)));
  assert(
    detached.some(f => Math.hypot(...f.center_of_mass) > 0.05),
    'Detached rider must use animated contact bounds instead of a fixed pelvis center'
  );
  assert.equal(start.wreck.kind, 'Vertical');
  assert(
    frames.some(s => s.camera?.mode === 'wreck'),
    'Original wreck camera must run during separation'
  );
  assert(
    frames
      .filter(s => s.camera?.mode === 'wreck')
      .every(s => [...s.camera.position, ...s.camera.target, s.camera.fov].every(Number.isFinite)),
    'Wreck camera must remain finite'
  );
  const cameras = frames.filter(s => s.camera?.mode === 'wreck').map(s => s.camera);
  assert(
    cameras.every(camera => camera.scene_query?.direct),
    'Wreck camera must use its independent original partition query'
  );
  assert(
    cameras.some(camera => camera.scene_query.queried_this_step),
    'The direct obstruction query must execute before any latch'
  );
  assert(
    cameras.every(
      camera =>
        camera.scene_query.radius >= 0 &&
        [...camera.scene_query.center, camera.scene_query.radius].every(Number.isFinite)
    ),
    'Direct camera bounds must remain finite'
  );
  const tracking = await page.evaluate(() => window.minimapTracking);
  assert(tracking.samples.some(s => s.actor === 'vehicle'));
  assert(tracking.samples.some(s => s.actor === (process.env.ATV_FUNNY_CRASHES === '1' ? 'ragdoll rider' : 'rider')));
  assert.deepEqual(tracking.errors, [], 'GPS center, bearing and aerial crop match actual GPU actor transforms');
  await writeFile(
    process.env.ATV_WRECK_TRACE || 'work/verification/wreck-browser-trace.json',
    JSON.stringify({ start, reset, frames, errors, tracking }, null, 2)
  );
  if (process.env.ATV_CRASH_ISOLATION === '1') await verifyCrashIsolation(page);
  if (process.env.ATV_FUNNY_CRASHES === '1') console.log('Crash GPU draws', await page.evaluate(() => crashDraws));
  assertNoErrors(errors);
  assert.equal(await page.evaluate(() => window.atvTerrainStatus.glError), 0);
  console.log(
    'PASS: real touch Nac Nac → vertical wreck → original separation/ground clip → terrain contact → automatic reset → resumed driving.'
  );
} finally {
  await browser.close();
}
