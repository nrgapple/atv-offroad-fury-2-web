// Opt-in same-origin original-content test; no game data is included in CI.
// ATV_TEST_URL=<host/public HTTPS> PLAYWRIGHT_MODULE=<module> node tools/test_maps.mjs
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
const { chromium } = createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || 'playwright');
const base = process.env.ATV_TEST_URL || 'http://127.0.0.1:4173';
const browser = await chromium.launch({ headless: true });
try {
  for (const environment of (process.env.ATV_TEST_MAPS?.split(',') ?? ['env01','env02','env03','env04','env05','env06','env07','env08'])) {
    const page = await browser.newPage({ viewport: { width: 960, height: 540 } });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    try {
      await page.goto(`${base}/play.html?drive=1&environment=${environment}`);
      await page.waitForFunction(() => (window.atvTerrainStatus?.ready && window.atvTerrainStatus?.simulation) || document.querySelector('#status')?.classList.contains('error'), null, { timeout: 120000 });
      const loaded = await page.evaluate(() => window.atvTerrainStatus);
      assert(loaded.ready, await page.locator('#status').textContent());
      assert.match(loaded.source, new RegExp(`${environment}\\.tr2$`));
      assert.deepEqual(loaded.terrainGrid, environment === 'env04' ? [1025,1281] : [1281,1281]);
      assert.deepEqual(loaded.terrainTextureResolution, [256,256]);
      assert.equal(loaded.terrainSampleSpacing, 4.5);
      assert.deepEqual(loaded.terrainDetailTexture, [64,64], 'Original detail texture is decoded separately from the base color');
      assert(loaded.vegetationPlacements > 6000 && loaded.vegetationVisible > 0);
      assert(loaded.vegetationDraws > 0 && loaded.vegetationDraws < 40, 'Vegetation remains batched');
      assert(loaded.obstacles.instances > 0);
      assert.deepEqual(loaded.unsupportedSceneMeshes, [], 'All placed original scenery meshes decode, including the v4 towers');
      await page.bringToFront(); await page.locator('#game').focus();
      await page.keyboard.down('KeyW');
      const target = loaded.simulationSteps + (environment === 'env01' ? 480 : 60);
      await page.waitForFunction(target => window.atvTerrainStatus.simulationSteps >= target, target, { timeout: 120000 });
      await page.keyboard.up('KeyW');
      const state = await page.evaluate(() => window.atvTerrainStatus);
      if (['env04', 'env05', 'env07', 'env08'].includes(environment)) {
        assert(state.water.signature_channels > 0, 'Original WaterFile must load its raw height signatures');
        assert(Number.isFinite(state.frame.water_height) && state.frame.water_height !== -256, 'The shared animated field must feed the vehicle water-height cache');
      } else {
        assert.equal(state.water, null);
        assert.equal(state.frame.water_height, -256, 'Dry scenes retain the original no-water sentinel');
      }
      assert(state.frame.origin.every(Number.isFinite));
      assert(state.frame.orientation.every(Number.isFinite));
      assert(Number.isFinite(state.frame.speed));
      assert(Math.hypot(state.frame.origin[0]-loaded.frame.origin[0],state.frame.origin[2]-loaded.frame.origin[2])>.5, 'Throttle must move the ATV across the terrain');
      assert.equal(state.glError, 0);
      if (environment === 'env01') {
        assert(state.terrainDetailUpdates > loaded.terrainDetailUpdates, 'Finest terrain follows the moving ATV');
        assert.equal(state.sceneInstances, loaded.sceneInstances);
        assert.equal(state.vehicleTextures, loaded.vehicleTextures);
      }
      assert.deepEqual(errors, []);
      if (process.env.ATV_MAP_SCREENSHOTS) await page.screenshot({ path: `${process.env.ATV_MAP_SCREENSHOTS}-${environment}.png` });
      console.log(JSON.stringify({ environment, ready: state.ready, terrainTriangles: state.triangles, terrainUpdates: state.terrainDetailUpdates, vegetation: state.vegetationPlacements, draws: state.vegetationDraws, sceneInstances: state.sceneInstances, unsupportedMeshes: state.unsupportedSceneMeshes, water: state.water, waterHeight: state.frame.water_height, glError: state.glError }));
    } catch (error) {
      console.error(JSON.stringify({ environment, errors, status: await page.locator('#status').textContent().catch(() => null), state: await page.evaluate(() => ({ ready: window.atvTerrainStatus?.ready, simulation: window.atvTerrainStatus?.simulation })).catch(() => null) }));
      throw error;
    } finally { await page.close(); }
  }
  console.log('PASS: all requested original environments load and drive, vegetation is batched, full-sample terrain follows the ATV, no browser/GL errors.');
} finally { await browser.close(); }
