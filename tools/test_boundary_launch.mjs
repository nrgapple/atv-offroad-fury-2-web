// Opt-in real-content edge launch. Uses normal keyboard controls; no state injection.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { mkdir, writeFile } from 'node:fs/promises';
const { chromium } = createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || 'playwright');
await mkdir('work/verification', { recursive: true });
const browser = await chromium.launch({ headless: true });
try {
  const page = await browser.newPage({ viewport: { width: 960, height: 540 } });
  const errors = []; page.on('pageerror', error => errors.push(error.message));
  await page.addInitScript(() => {
    window.boundaryFrames = [];
    const watched = new WeakSet();
    const post = Worker.prototype.postMessage;
    Worker.prototype.postMessage = function(data, ...rest) {
      if (!watched.has(this)) {
        watched.add(this); this.addEventListener('message', ({data}) => {
          if (data.error || !data.bytes) return;
          try {
            const decoded = JSON.parse(new TextDecoder().decode(data.bytes));
            if (Array.isArray(decoded) && decoded[0]?.frame?.boundary_launch) {
              window.boundaryFrames.push(...decoded.map(({frame: f}) => ({
                origin: f.origin, orientation: f.orientation, linear_velocity: f.linear_velocity,
                local_angular_velocity: f.local_angular_velocity, grounded: f.grounded,
                boundary_launch: f.boundary_launch, skip_vegetation_query: f.skip_vegetation_query,
                wreck: f.wreck, reset_count: f.reset_count, rider_wreck: f.rider_wreck,
              })));
            }
          } catch { /* Other codec outputs are binary. */ }
        });
      }
      return post.call(this, data, ...rest);
    };
  });
  const environment = process.env.ATV_BOUNDARY_MAP || 'env04';
  await page.goto(new URL(`/play.html?drive=1&environment=${environment}`, process.env.ATV_TEST_URL || 'http://127.0.0.1:4180').href);
  await page.waitForFunction(() => window.atvTerrainStatus?.simulation, null, { timeout: 90000 });
  await page.bringToFront(); await page.locator('#game').focus();
  for (let i = 0; i < 3 && (await page.evaluate(() => window.atvTerrainStatus.performance.mode)) !== 'debug'; i++) await page.keyboard.press('F3');
  await page.keyboard.down('KeyW');
  try {
    await page.waitForFunction(() => window.boundaryFrames.some(f => f.boundary_launch.active), null, { timeout: 360000 });
    await page.keyboard.up('KeyW');
    const launch = await page.evaluate(() => window.boundaryFrames.find(f => f.boundary_launch.active));
    console.log(JSON.stringify({ launch: { origin: launch.origin, velocity: launch.linear_velocity, spin: launch.local_angular_velocity, wreck: launch.wreck, boundary: launch.boundary_launch } }));
    assert.equal(launch.wreck.phase, 'Falling');
    assert.equal(launch.boundary_launch.elapsed, 0);
    assert(launch.linear_velocity[1] > 112 && launch.linear_velocity[1] < 113);
    const inward = [(environment === 'env04' ? 2304 : 2880) - launch.origin[0], 2880 - launch.origin[2]];
    assert(launch.linear_velocity[0] * inward[0] + launch.linear_velocity[2] * inward[1] > 0);
    const launchReset = launch.reset_count;
    if (process.env.ATV_BOUNDARY_SCREENSHOT) await page.screenshot({ path: process.env.ATV_BOUNDARY_SCREENSHOT });
    await page.waitForFunction(count => window.atvTerrainStatus.frame.reset_count > count, launchReset, { timeout: 90000 });
    const recovered = await page.evaluate(() => window.atvTerrainStatus.frame);
    assert.equal(recovered.wreck.phase, 'Riding');
    assert.equal(recovered.boundary_launch.active, false);
    assert.equal(recovered.boundary_launch.elapsed, 0);
    assert.equal(recovered.skip_vegetation_query, false);
    const center = [environment === 'env04' ? 2304 : 2880, 2880];
    const launchDistance = Math.hypot(launch.origin[0] - center[0], launch.origin[2] - center[1]);
    const recoveryDistance = Math.hypot(recovered.origin[0] - center[0], recovered.origin[2] - center[1]);
    assert(recoveryDistance < launchDistance - 100, 'Recovery must follow the inward flight instead of returning to the old edge checkpoint');
    const frames = await page.evaluate(() => window.boundaryFrames);
    const duringFlight = frames.filter(f => f.boundary_launch.active);
    assert(duringFlight.some(f => f.rider_wreck?.phase === 'air'), 'Rider must separate during launch');
    const riders = duringFlight.filter(f => f.rider_wreck);
    assert(riders.length > 0 && riders.every(f => f.rider_wreck.scene_query?.cached), 'Boundary rider must retain the shared scene cache');
    assert(riders.every(f => f.rider_wreck.scene_query.skip_vegetation_query === f.skip_vegetation_query), 'Native boundary launch must set and clear both actor query flags');
    assert(duringFlight.some(f => f.boundary_launch.elapsed > 1 && !f.skip_vegetation_query));
    assert.equal(duringFlight.filter(f => f.boundary_launch.elapsed === 0).length, 1);
    console.log(JSON.stringify({ recovered: { origin: recovered.origin, boundary: recovered.boundary_launch, resets: recovered.reset_count } }));
    assert.deepEqual(errors, []);
    assert.equal(await page.evaluate(() => window.atvTerrainStatus.glError), 0);
    console.log('PASS normal drive crosses original edge, launches inward/upward into wreck, separates and recovers with boundary state cleared.');
  } finally {
    const frames = await page.evaluate(() => window.boundaryFrames);
    await writeFile(process.env.ATV_BOUNDARY_TRACE || 'work/verification/boundary-launch-trace.json', JSON.stringify(frames));
    console.log(JSON.stringify({ frames: frames.length, final: frames.at(-1) }));
  }
} finally { await browser.close(); }
