// Opt-in same-origin original-content test; no game data is included in CI.
// ATV_TEST_URL=<host/public HTTPS> PLAYWRIGHT_MODULE=<module> node tools/test_maps.mjs
import assert from 'node:assert/strict';
import { ENVIRONMENTS } from '../web/launch.js';
import { createRequire } from 'node:module';
const { chromium } = createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || 'playwright');
const base = process.env.ATV_TEST_URL || 'http://127.0.0.1:4173';
const browser = await chromium.launch({ headless: true });
try {
  for (const environment of (process.env.ATV_TEST_MAPS?.split(',') ?? ENVIRONMENTS)) {
    const page = await browser.newPage({ viewport: { width: 960, height: 540 } });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    try {
      await page.goto(`${base}/play.html?drive=1&environment=${environment}`);
      await page.waitForFunction(() => (window.atvTerrainStatus?.ready && window.atvTerrainStatus?.simulation) || document.querySelector('#status')?.classList.contains('error') || document.querySelector('#frontend')?.dataset.error, null, { timeout: 120000 });
      const loaded = await page.evaluate(() => window.atvTerrainStatus);
      assert(loaded.ready, `${await page.locator('#status').textContent()} / ${await page.locator('#menu-notice').textContent()}`);
      assert.match(loaded.source, new RegExp(`${environment}\\.tr2$`));
      assert(loaded.terrainGrid.every(n=>Number.isInteger(n)&&n>=257&&n<=1281));
      assert(loaded.terrainTextureResolution.every(n=>Number.isInteger(n)&&n>0));
      assert.equal(loaded.terrainSampleSpacing, /^(sx|training|game)|^free0[123]$/.test(environment)?3:4.5);
      assert.deepEqual(loaded.terrainDetailTexture, [64,64], 'Original detail texture is decoded separately from the base color');
      if (/^env|^free0[56]$/.test(environment)) {
        assert(loaded.vegetationPlacements > 6000 && loaded.vegetationVisible > 0);
        assert(loaded.vegetationDraws > 0 && loaded.vegetationDraws < 40, 'Vegetation remains batched');
      } else assert((loaded.vegetationPlacements ?? 0)>=0 && (loaded.vegetationDraws ?? 0)<40,'Optional original ecosystems remain bounded and batched');
      // FREE04's common SCN places only Cruiseliner; its runtime archive has
      // no static collision mesh. Terrain/water still drive the shared cache.
      if (environment === 'free04') assert.equal(loaded.obstacles.instances, 0);
      else assert(loaded.obstacles.instances > 0, `${environment} must load its source scenery colliders`);
      assert.deepEqual(loaded.unsupportedSceneMeshes, [], 'All placed original scenery meshes decode, including the v4 towers');
      await page.bringToFront(); await page.locator('#game').focus();
      await page.keyboard.down('KeyW');
      const target = loaded.simulationSteps + (environment === 'env01' ? 480 : 180);
      await page.waitForFunction(target => window.atvTerrainStatus.simulationSteps >= target, target, { timeout: 120000 });
      await page.keyboard.up('KeyW');
      await page.waitForFunction(() => window.atvTerrainStatus.minimap?.visible, null, {timeout:90000});
      const state = await page.evaluate(() => window.atvTerrainStatus);
      assert.equal(state.minimap.extraSceneDraws, 0);
      assert(state.minimap.backingBytes < 4.5 * 1048576, 'GPS pixel storage stays bounded');
      if (['env04', 'env05', 'env07', 'env08', 'free04', 'nat01'].includes(environment)) {
        assert(state.water.signature_channels > 0, 'Original WaterFile must load its raw height signatures');
        assert(Number.isFinite(state.frame.water_height) && state.frame.water_height !== -256, 'The shared animated field must feed the vehicle water-height cache');
      } else {
        assert.equal(state.water, null);
        assert.equal(state.frame.water_height, -256, 'Dry scenes retain the original no-water sentinel');
      }
      assert(state.frame.origin.every(Number.isFinite));
      assert.equal(state.frame.scene_query.enabled, true, 'Scene metadata and ESB placements must reach the original shared candidate cache');
      assert(state.frame.scene_query.cached_nodes <= 256, 'Native cache capacity remains bounded with vegetation included');
      assert.equal(state.frame.scene_query.unsupported_vegetation_proxies, 0, 'All configured proxies in the eight original Freestyle ecosystems are supported cylinders');
      assert(state.frame.orientation.every(Number.isFinite));
      assert(Number.isFinite(state.frame.speed));
      assert.equal(state.frame.wheelie_scoring_enabled, false, 'Original wheelie scoring cheat stays off in baseline sessions');
      assert.equal(state.frame.wheelie_score.eligible, false);
      assert(Number.isFinite(state.frame.wheelie_score.elapsed));
      assert.equal(state.frame.stunt_score.committed_points, 0, 'Driving alone must not award stunt or wheelie points');
      assert(Math.hypot(state.frame.origin[0]-loaded.frame.origin[0],state.frame.origin[2]-loaded.frame.origin[2])>.5, 'Throttle must move the ATV across the terrain');
      assert.equal(state.glError, 0);
      if (environment === 'env01') {
        assert(state.terrainDetailUpdates > loaded.terrainDetailUpdates, 'Finest terrain follows the moving ATV');
        assert.equal(state.sceneInstances, loaded.sceneInstances);
        assert.equal(state.vehicleTextures, loaded.vehicleTextures);
      }
      assert.deepEqual(errors, []);
      if (process.env.ATV_MAP_SCREENSHOTS) await page.screenshot({ path: `${process.env.ATV_MAP_SCREENSHOTS}-${environment}.png` });
      console.log(JSON.stringify({ environment, ready: state.ready, minimap: state.minimap, terrainTriangles: state.triangles, terrainUpdates: state.terrainDetailUpdates, vegetation: state.vegetationPlacements, draws: state.vegetationDraws, sceneInstances: state.sceneInstances, unsupportedMeshes: state.unsupportedSceneMeshes, water: state.water, waterHeight: state.frame.water_height, glError: state.glError }));
    } catch (error) {
      console.error(JSON.stringify({ environment, errors, status: await page.locator('#status').textContent().catch(() => null), state: await page.evaluate(() => ({ ready: window.atvTerrainStatus?.ready, simulation: window.atvTerrainStatus?.simulation })).catch(() => null) }));
      throw error;
    } finally { await page.close(); }
  }
  console.log('PASS: all requested original environments load and drive, vegetation is batched, full-sample terrain follows the ATV, no browser/GL errors.');
} finally { await browser.close(); }
