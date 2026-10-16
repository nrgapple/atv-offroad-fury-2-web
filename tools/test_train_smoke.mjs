// Private-content gate: Arlington's original locomotive smoke (type0 locosmoke.emt
// attached to the moving Smoke.slt actor) follows the Rust train route.
// No map coordinates are committed; the camera placement is derived at runtime
// from the emitter's own travel.
import assert from 'node:assert/strict';
import {
  assertNoErrors,
  base,
  button,
  launch,
  newPage,
  optionalScreenshot,
  waitForGame,
  waitForSteps
} from './browser/harness.mjs';
const browser = await launch();
const smoke = page =>
  page.evaluate(() => {
    const t = window.atvTerrainStatus,
      fx = t.ambientEffects,
      actor = fx.frame.actors?.[0];
    return {
      steps: t.simulationSteps,
      actors: fx.actors,
      unsupported: fx.unsupported,
      capacity: fx.capacity,
      actor,
      total: fx.frame.total_emitted,
      particles: fx.frame.particles.length,
      near: actor
        ? fx.frame.particles.filter(
            p => Math.hypot(p.position[0] - actor[0], p.position[2] - actor[2]) < 400 && p.position[1] >= actor[1] - 0.5
          ).length
        : 0,
      visible: fx.visible,
      draws: fx.draws,
      screenRain: t.weather.screen_rain,
      glError: document.querySelector('#game').getContext('webgl2').getError()
    };
  });
try {
  for (const mobile of [false, true]) {
    const errors = [],
      page = await newPage(
        browser,
        mobile
          ? { viewport: { width: 390, height: 844 }, deviceScaleFactor: 3, isMobile: true, hasTouch: true }
          : { viewport: { width: 960, height: 600 } },
        errors
      );
    await page.addInitScript(() => {
      const NativeWorker = Worker;
      window.Worker = class extends NativeWorker {
        postMessage(data, ...args) {
          if (data.op === 23 && window.smokeTestSpawn) {
            const payload = JSON.parse(new TextDecoder().decode(data.bytes));
            Object.assign(payload, window.smokeTestSpawn);
            window.smokeTestSpawn = null;
            data.bytes = new TextEncoder().encode(JSON.stringify(payload));
          }
          super.postMessage(data, ...args);
        }
      };
    });
    await page.goto(`${base}/play.html?drive=1&environment=env08`);
    await waitForGame(page);
    await page.locator('#game').focus();
    const first = await smoke(page);
    assert.deepEqual(first.actors, ['locosmoke.emt -> smoke#0']);
    assert.deepEqual(first.unsupported, []);
    assert.equal(first.capacity, 728, 'separate bounded moving-actor slots');
    assert.equal(first.screenRain, false, 'no ScreenRain without its source Weather declaration');
    await waitForSteps(page, 60);
    const a = await smoke(page);
    await waitForSteps(page, 60);
    const b = await smoke(page);
    assert(a.actor && b.actor, 'train pose reaches the attached emitter');
    const travel = [b.actor[0] - a.actor[0], b.actor[2] - a.actor[2]],
      speed = (Math.hypot(...travel) * 60) / (b.steps - a.steps);
    assert(speed > 20 && speed < 80, `locomotive emitter speed ${speed}`);
    assert(b.total > a.total, 'smoke births continue while the route advances');
    assert(b.near >= 10, `smoke trails the locomotive (${b.near})`);
    assert(b.particles <= b.capacity);
    // Ride beside the route ahead of the train, facing it (runtime-derived).
    const dir = travel.map(v => v / Math.hypot(...travel)),
      ahead = [b.actor[0] + dir[0] * speed * 3, b.actor[2] + dir[1] * speed * 3],
      side = [dir[1], -dir[0]];
    await page.evaluate(
      ({ origin, yaw }) => {
        window.smokeTestSpawn = { origin, yaw };
      },
      { origin: [ahead[0] + side[0] * 110, b.actor[1] - 14, ahead[1] + side[1] * 110], yaw: Math.atan2(-side[0], -side[1]) }
    );
    await page.keyboard.press('r');
    await waitForSteps(page, 200);
    const passing = await smoke(page);
    assert(passing.total > b.total, 'ATV reset preserves and continues the scene particle clock');
    assert(passing.visible > 0 && passing.draws >= 1, 'smoke sprites reach the bounded instanced draw');
    await optionalScreenshot(page, 'ATV_TRAIN_SMOKE_SCREENSHOTS', mobile ? 'portrait' : 'desktop');
    // Pause stops the fixed scene clock and births.
    await page.keyboard.press('Escape');
    await button(page, 'Resume').waitFor();
    const paused = await smoke(page);
    // Time-based on purpose: prove nothing advances while paused.
    await page.waitForTimeout(600);
    const still = await smoke(page);
    assert.equal(still.total, paused.total);
    assert.equal(still.steps, paused.steps);
    await button(page, 'Resume').click();
    await page.locator('#game').focus();
    await waitForSteps(page, 30);
    const resumed = await smoke(page);
    assert(resumed.total >= still.total && resumed.actor, 'resumed smoke');
    assert.equal(resumed.glError, 0);
    assertNoErrors(errors);
    console.log(JSON.stringify({ mobile, speed, near: passing.near, visible: passing.visible, total: resumed.total }));
    await page.close();
  }
  console.log('PASS: original locomotive smoke follows the moving train at desktop and 390px, with pause/reset lifecycle.');
} finally {
  await browser.close();
}
