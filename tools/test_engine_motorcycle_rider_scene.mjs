// A thrown motorcycle rider meets scenery (#751): the Suzuki RM125 (Garage
// selection 29) rides ENV01 flat out, bears right and wrecks; the rider is
// thrown into the scenery ahead, touches it and drops beside it. Natively
// (crates/atv-game/tests/thrown_rider.rs) the same script with terrain-only
// rider collision carries him about 42 ft in the first second; against the
// scenery he travels under 7 ft. The held final frame shows where he lands.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, optionalScreenshot, url } from './browser/harness.mjs';

const script = [
  { frames: 60, input: {} },
  { frames: 300, input: { throttle: 1 } },
  { frames: 300, input: { throttle: 1, steer: 0.4 } },
];
const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 960, height: 540 } }, errors);
  await page.addInitScript(text => localStorage.setItem('furyScript', text), JSON.stringify(script));
  await page.goto(url('/play.html?venue=env01&quad=29&weather=off&traceAll=1&script=storage'));
  await page.waitForFunction(() => document.querySelector('#trace') || /error|fail/i.test(document.querySelector('#status')?.textContent), null, { timeout: 180000 });
  const trace = JSON.parse(await page.locator('#trace').textContent());
  assert.equal(trace.length, 660);
  const thrown = trace.filter(r => r.rider);
  assert.ok(thrown.length > 60, `the rider was thrown (${thrown.length} steps)`);
  const touching = thrown.filter(r => r.rider.scene > 0);
  assert.ok(touching.length > 0, 'the thrown rider touched scenery');
  const [start, second] = [thrown[0].rider.pelvis, thrown[60].rider.pelvis];
  const travel = Math.hypot(second[0] - start[0], second[2] - start[2]);
  assert.ok(travel < 15, `scenery stopped the rider: ${travel.toFixed(1)} ft in the first second (terrain only: about 42 ft)`);
  for (const r of thrown) assert.ok(r.rider.pelvis.every(Number.isFinite), `finite rider at step ${r.step}`);
  await page.waitForTimeout(1500);
  await optionalScreenshot(page, 'ATV_RIDER_SCENE_SHOTS', 'rest');
  assertNoErrors(errors);
  console.log(`PASS: thrown rider touched scenery on ${touching.length} steps from step ${touching[0].step} and moved ${travel.toFixed(1)} ft in the first second`);
} finally {
  await browser.close();
}
