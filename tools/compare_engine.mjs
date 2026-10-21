// Parity captures for the shared Rust engine. Renders one venue from the same
// camera in (1) the current JavaScript game, (2) the Rust engine in the browser
// (WebGL2) and (3) the native Rust build, and writes the three PNGs.
//
//   node tools/compare_engine.mjs [venue] [out-dir]
// Environment: ATV_TEST_URL (served site with private content), FURY_BIN
// (native binary; skipped when unset) and FURY_CONTENT (package directory).
// Outputs belong in ignored private storage; they contain game imagery.
import { execFileSync } from 'node:child_process';
import { mkdir } from 'node:fs/promises';
import path from 'node:path';
import { assertNoErrors, launch, newPage, url, waitForGame } from './browser/harness.mjs';

const venue = process.argv[2] ?? 'env01';
const out = path.resolve(process.argv[3] ?? 'private/reports/engine-parity');
const viewport = { width: 1280, height: 720 };
await mkdir(out, { recursive: true });
const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport }, errors);
  await page.goto(url(`/play.html?drive=1&environment=${venue}`));
  await waitForGame(page);
  // Let the start camera settle before sampling it.
  await page.waitForTimeout(2500);
  const camera = await page.evaluate(() => atvTerrainStatus.renderCamera);
  await page.locator('#game').screenshot({ path: path.join(out, `${venue}-javascript.png`) });
  assertNoErrors(errors);
  const view = [camera.x, camera.y, camera.z, camera.yaw, camera.pitch, camera.focalLength].map(v => Number(v.toFixed(6))).join(',');
  console.log(`camera ${view}`);

  const rustErrors = [];
  const rust = await newPage(browser, { viewport }, rustErrors);
  await rust.goto(url(`/engine.html?venue=${venue}&camera=${view}`));
  await rust.waitForFunction(() => /fps/.test(document.querySelector('#status').textContent) || /fail|error/i.test(document.querySelector('#status').textContent), null, { timeout: 120000 });
  await rust.waitForTimeout(2000);
  const status = await rust.locator('#status').textContent();
  console.log(`rust web: ${status}`);
  await rust.evaluate(() => { document.querySelector('#status').hidden = true; });
  await rust.locator('canvas').screenshot({ path: path.join(out, `${venue}-rust-web.png`) });
  assertNoErrors(rustErrors);

  if (process.env.FURY_BIN) {
    const target = path.join(out, `${venue}-rust-native.png`);
    execFileSync(process.env.FURY_BIN, ['--venue', venue, '--camera', view, '--capture', target, `${viewport.width}x${viewport.height}`], { stdio: 'inherit' });
  }
  console.log(`PASS: wrote ${venue} parity captures to ${out}`);
} finally {
  await browser.close();
}
