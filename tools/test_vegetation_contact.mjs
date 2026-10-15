// Opt-in original-content physical tree contact, through normal keyboard input.
import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';
import { captureSimulation } from './capture_simulation.mjs';
import { assertNoErrors, base, launch, newPage } from './browser/harness.mjs';
await mkdir('work/verification', { recursive: true });
const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 960, height: 540 } }, errors);
  await captureSimulation(page, 'plantFrames');
  const url = new URL('/play.html', base);
  url.searchParams.set('drive', '1');
  url.searchParams.set('environment', process.env.ATV_PLANT_MAP || 'env04');
  await page.goto(url.href);
  await page.waitForFunction(() => window.atvTerrainStatus?.simulation, null, { timeout: 90000 });
  await page.bringToFront();
  await page.locator('#game').focus();
  await page.keyboard.down('KeyW');
  try {
    await page.waitForFunction(
      () => window.plantFrames.some(({ frame: f }) => f.scene_query.vegetation_chassis_callbacks > 0),
      null,
      { timeout: 180000 }
    );
    await page.keyboard.up('KeyW');
    const hit = await page.evaluate(() =>
      window.plantFrames.find(({ frame: f }) => f.scene_query.vegetation_chassis_callbacks > 0)
    );
    assert(hit.frame.obstacle_contact_count > 0, 'Tree callback must enter the chassis contact response');
    assert(hit.frame.scene_query.vegetation_proxy_queries > 0);
    assert.equal(hit.frame.scene_query.unsupported_vegetation_proxies, 0);
    assert(hit.frame.origin.every(Number.isFinite) && hit.frame.linear_velocity.every(Number.isFinite));
    console.log(
      JSON.stringify({
        step: hit.step,
        origin: hit.frame.origin,
        velocity: hit.frame.linear_velocity,
        scene: hit.frame.scene_query,
        contacts: hit.frame.obstacle_contact_count,
        wreck: hit.frame.wreck
      })
    );
    if (process.env.ATV_PLANT_SCREENSHOT) await page.screenshot({ path: process.env.ATV_PLANT_SCREENSHOT });
    assertNoErrors(errors);
    assert.equal(await page.evaluate(() => window.atvTerrainStatus.glError), 0);
    console.log(
      'PASS original placed vegetation produces chassis callbacks and physical contacts through normal driving.'
    );
  } finally {
    await page.keyboard.up('KeyW');
    await writeFile(
      process.env.ATV_PLANT_TRACE || 'work/verification/vegetation-contact-trace.json',
      JSON.stringify(await page.evaluate(() => window.plantFrames))
    );
  }
} finally {
  await browser.close();
}
