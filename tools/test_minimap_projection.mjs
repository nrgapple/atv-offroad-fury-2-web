import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';
import { captureMinimapTracking } from './minimap_tracking.mjs';
import { assertNoErrors, base, launch, newPage, nextFrames, waitForGame, waitForSteps } from './browser/harness.mjs';
const out = process.env.ATV_MINIMAP_OUTPUT || 'work/verification/minimap-projection';
await mkdir(out, { recursive: true });
const browser = await launch(), results = [];
try {
  for (const [quadId, environment] of [[1, 'env02'], [26, 'env01']]) {
    const errors = [], page = await newPage(browser, { viewport: { width: 390, height: 844 }, hasTouch: true }, errors);
    await page.addInitScript(quadId => localStorage.setItem('atv-player-selection', JSON.stringify({ quadId, riderId: 0, freestyleEnvironment: 'env01' })), quadId);
    await captureMinimapTracking(page);
    await page.goto(`${base}/play.html?drive=1&environment=${environment}`);
    await waitForGame(page, { until: () => window.atvTerrainStatus.minimap.visible });
    await page.locator('#game').focus();
    await page.keyboard.down('KeyW'); await page.keyboard.down('KeyD');
    await waitForSteps(page, 108); await page.keyboard.up('KeyD'); await page.keyboard.up('KeyW');
    await page.waitForFunction(() => window.minimapTracking.samples.length > 10);
    await page.screenshot({ path: `${out}/${quadId}-wider-gps.png` });
    await page.getByRole('button', { name: 'Pause', exact: true }).click();
    await page.getByRole('button', { name: 'HUD settings', exact: true }).click();
    await page.getByRole('button', { name: 'Minimap configuration', exact: true }).click();
    const zoom = page.getByRole('slider', { name: 'Minimap zoom', exact: true });
    await zoom.waitFor(); await zoom.focus(); await page.keyboard.press('Home');
    await page.waitForFunction(() => window.atvTerrainStatus.minimap.zoom === 25);
    await page.waitForFunction(() => !document.querySelector('.menu-transition'));
    await nextFrames(page, 3); // Let the next Rust menu frame paint the updated label.
    await page.screenshot({ path: `${out}/${quadId}-zoom-settings.png` });
    for (const name of ['Back', 'Back', 'Resume']) await page.getByRole('button', { name, exact: true }).click();
    await page.waitForFunction(() => window.minimapTracking.samples.some(s => s.zoom === 25));
    const movingTracking = await page.evaluate(() => window.minimapTracking);
    assert.deepEqual(movingTracking.errors, [], 'Moving actor GPS matches the rendered GPU pose and crop');
    await page.reload(); await waitForGame(page, { until: () => window.atvTerrainStatus.minimap.visible });
    assert.equal(await zoom.count(), 0);
    assert.equal(await page.evaluate(() => window.atvTerrainStatus.minimap.zoom), 25);
    await page.waitForFunction(() => window.minimapTracking.samples.length > 3);
    const tracking = await page.evaluate(() => window.minimapTracking);
    assert.deepEqual(tracking.errors, [], 'GPS crop, heading and position agree with actual GPU actor transforms');
    assertNoErrors(errors);
    results.push({ quadId, environment, movingTracking, tracking }); await page.close();
  }
  await writeFile(`${out}/results.json`, JSON.stringify(results, null, 2));
  console.log('PASS: real ATV and interpolated motorcycle GPU pose/crop tracking, wider default, keyboard zoom and saved zoom.');
} finally { await browser.close(); }
