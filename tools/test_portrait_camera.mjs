// Exercise the rendered camera across live viewport rotation. No vehicle injection.
import assert from 'node:assert/strict';
import { assertNoErrors, base, launch, newPage } from './browser/harness.mjs';
const browser = await launch();
try {
  const errors = [];
  const page = await newPage(
    browser,
    { viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true },
    errors
  );
  await page.goto(new URL('/play.html?drive=1&environment=env01', base).href);
  await page.waitForFunction(() => window.atvTerrainStatus?.simulationSteps > 90, null, { timeout: 90000 });
  const portrait = await page.evaluate(() => window.atvTerrainStatus);
  assert.equal(portrait.cameraPresentation.portraitDistanceScale, 1.35);
  assert.equal(portrait.cameraPresentation.pitchLift, 0.12);
  const nativePitch = frame =>
    Math.atan2(
      frame.target[1] - frame.position[1],
      Math.hypot(frame.target[0] - frame.position[0], frame.target[2] - frame.position[2])
    );
  assert(
    Math.abs(portrait.camera[4] - nativePitch(portrait.followCamera) - 0.12 - portrait.cameraPresentation.uphillLift) <
      1e-6
  );
  assert(portrait.cameraPresentation.focalLength < 4 / 3 / Math.tan((portrait.followCamera.fov * Math.PI) / 360));
  if (process.env.ATV_CAMERA_SCREENSHOT) await page.screenshot({ path: process.env.ATV_CAMERA_SCREENSHOT });
  await page.setViewportSize({ width: 1280, height: 720 });
  await page.waitForFunction(() => window.atvTerrainStatus.cameraPresentation.aspect > 1);
  const landscape = await page.evaluate(() => window.atvTerrainStatus);
  assert.equal(landscape.cameraPresentation.portraitDistanceScale, 1);
  if (landscape.cameraPresentation.requestedHillOrbit === 0)
    assert.deepEqual(landscape.camera.slice(0, 3), landscape.followCamera.position);
  assert(
    Math.abs(landscape.camera[4] - nativePitch(landscape.followCamera) - landscape.cameraPresentation.uphillLift) <
      1e-12,
    'landscape pitch matches the presented hill rig within floating-point rounding'
  );
  assert.equal(
    landscape.cameraPresentation.focalLength,
    4 / 3 / Math.tan((landscape.followCamera.fov * Math.PI) / 360)
  );
  await page.setViewportSize({ width: 390, height: 844 });
  await page.waitForFunction(() => window.atvTerrainStatus.cameraPresentation.portraitDistanceScale === 1.35);
  assertNoErrors(errors);
  console.log(
    'PASS: portrait camera widens and lifts sightline; rotation restores native landscape position/FOV with the independent uphill sightline.'
  );
} finally {
  await browser.close();
}
