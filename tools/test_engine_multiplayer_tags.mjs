// Host-only gate: name tags and minimap markers for other riders in the Rust
// engine. A JavaScript host rides ahead; a Rust browser guest draws its name
// tag over it and a slot-coloured dot on the GPS map.
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_multiplayer_tags.mjs
// ATV_MP_SCREENSHOT=out.png saves the guest's view.
import assert from 'node:assert/strict';
import { assertNoErrors, base, button, holdKey, launch, newPage } from './browser/harness.mjs';

const shots = process.env.ATV_MP_SCREENSHOT;
const errors = [];
const browser = await launch();
const engine = p => p.evaluate(() => JSON.parse(window.furyMultiplayer ?? 'null'));
try {
  const host = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await host.addInitScript(() => Object.defineProperty(document, 'hasFocus', { value: () => true }));
  await host.goto(base + '/play.html');
  await button(host, 'Press START').click({ timeout: 90000 });
  await button(host, 'Multiplayer').click();
  await button(host, 'Host Game').click();
  await host.getByLabel('Rider name').fill('JsHost');
  await button(host, 'Select Track').click();
  await host.locator('button.ms-ride').click({ timeout: 60000 });
  await host.waitForFunction(() => window.atvTerrainStatus?.multiplayer?.state?.code, null, { timeout: 120000 });
  const code = (await host.evaluate(() => window.atvTerrainStatus)).multiplayer.state.code;
  const guest = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await guest.addInitScript(() => {
    Object.defineProperty(document, 'hasFocus', { value: () => true });
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
  });
  await guest.goto(`${base}/engine.html?room=${code}&name=RustGuest&quad=3`);
  await guest.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 1, null, {
    timeout: 180000
  });
  // The host rides on ahead of the guest's camera.
  await host.bringToFront();
  await holdKey(host, 'KeyW', 90);
  await guest.bringToFront();
  await guest.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.tags >= 1, null, { timeout: 30000 });
  const e = await engine(guest);
  assert.equal(e.tags, 1);
  if (shots) await guest.screenshot({ path: shots });
  assertNoErrors(errors);
  console.log(JSON.stringify({ code, tags: e.tags, drawn: e.drawn }));
} finally {
  await browser.close();
}
