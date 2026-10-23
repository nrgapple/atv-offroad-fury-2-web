// Host-only gate: other riders' engine sounds in a Rust engine room. A
// JavaScript host rides; a Rust browser guest joins. Each plays the other's
// engine as a spatial remote voice, and the host's driving starts new
// engine samples on the guest.
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_multiplayer_audio.mjs
import assert from 'node:assert/strict';
import { assertNoErrors, base, button, holdKey, jsBase, launch, newPage } from './browser/harness.mjs';

const report = {};
const errors = [];
const browser = await launch({ args: ['--autoplay-policy=no-user-gesture-required'] });
const engine = p => p.evaluate(() => JSON.parse(window.furyMultiplayer ?? 'null'));
try {
  const host = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await host.addInitScript(() => Object.defineProperty(document, 'hasFocus', { value: () => true }));
  await host.goto(jsBase + '/play.html');
  await button(host, 'Press START').click({ timeout: 90000 });
  await button(host, 'Multiplayer').click();
  await button(host, 'Host Game').click();
  await host.getByLabel('Rider name').fill('JsHost');
  await button(host, 'Select Track').click();
  await host.locator('button.ms-ride').click({ timeout: 60000 });
  await host.waitForFunction(
    () => window.atvTerrainStatus?.simulation && window.atvTerrainStatus.multiplayer?.state?.code,
    null,
    {
      timeout: 120000
    }
  );
  const code = (await host.evaluate(() => window.atvTerrainStatus)).multiplayer.state.code;

  const guest = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await guest.addInitScript(() => Object.defineProperty(document, 'hasFocus', { value: () => true }));
  await guest.goto(`${base}/play.html?room=${code}&name=RustGuest&quad=12`);
  await guest.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 1, null, {
    timeout: 180000
  });
  // A key press is the gesture that starts the Rust engine's sound.
  await guest.mouse.click(640, 360);
  await guest.keyboard.press('KeyM');
  await guest.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.remoteSound?.voices >= 1, null, {
    timeout: 60000
  });
  let e = await engine(guest);
  report.guestBefore = e.remoteSound;
  // The JavaScript host hears the Rust rider's engine too.
  await host.mouse.click(640, 360);
  await host.waitForFunction(() => window.atvTerrainStatus.audio?.multiplayer?.voices >= 1, null, { timeout: 60000 });
  report.hostHears = await host.evaluate(() => window.atvTerrainStatus.audio.multiplayer);
  // Driving changes the host's engine state: the guest starts new samples.
  await host.bringToFront();
  await holdKey(host, 'KeyW', 180);
  await guest.bringToFront();
  await guest.waitForTimeout(1000);
  e = await engine(guest);
  report.guestAfter = e.remoteSound;
  assert.ok(e.remoteSound.starts > report.guestBefore.starts, JSON.stringify(report));
  assertNoErrors(errors);
  console.log(JSON.stringify(report, null, 2));
} finally {
  await browser.close();
}
