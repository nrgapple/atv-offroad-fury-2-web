// Original-content render gate; source resources stay in the separate runtime manifest.
// ATV_TEST_URL=<host/public HTTPS> PLAYWRIGHT_MODULE=<module> node tools/test_water_render.mjs
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
const { chromium } = createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || 'playwright');
const base = process.env.ATV_TEST_URL || 'http://127.0.0.1:4173';
const browser = await chromium.launch({ headless: true });
try {
  for (const environment of ['env01', 'env04', 'env05', 'env07', 'env08']) {
    for (const mobile of environment === 'env08' ? [false, true] : [false]) {
      const page = await browser.newPage({ viewport: mobile ? { width: 390, height: 844 } : { width: 960, height: 540 } });
      const errors = [];
      page.on('pageerror', error => errors.push(error.message));
      await page.addInitScript(() => {
        const NativeWorker = window.Worker;
        window.Worker = class extends NativeWorker {
          constructor(...args) {
            super(...args); this.waterRequests = new Map();
            this.addEventListener('message', ({ data }) => {
              const op = this.waterRequests.get(data.id); this.waterRequests.delete(data.id);
              if (data.error || !data.bytes) return;
              if (op === 9) window.waterTerrainInfo = JSON.parse(new TextDecoder().decode(data.bytes));
              if (op === 10) window.waterTerrainHeights = new Float32Array(data.bytes.buffer, data.bytes.byteOffset, data.bytes.byteLength / 4);
              if (op === 64) window.waterSourceBase = JSON.parse(new TextDecoder().decode(data.bytes)).base_height;
            });
          }
          postMessage(data, ...args) {
            if (data.op === 50 && window.holdWaterPhysics) return;
            if (data.op === 23 && Number.isFinite(window.waterSourceBase)) {
              // Select a real wet area from runtime terrain, rather than embedding
              // extracted map coordinates or fabricating a water footprint.
              const start = JSON.parse(new TextDecoder().decode(data.bytes));
              const info = window.waterTerrainInfo, heights = window.waterTerrainHeights;
              const width = info.width_cells + 1, base = window.waterSourceBase;
              let best;
              for (let z = 64; z < info.depth_cells - 64; z += 8) for (let x = 64; x < info.width_cells - 64; x += 8) {
                if (heights[z * width + x] >= base - 2) continue;
                const wet = [[x - 4, z], [x + 4, z], [x, z - 4], [x, z + 4]].filter(([a, b]) => heights[b * width + a] < base - 2).length;
                if (wet < 2) continue;
                const position = [x * info.spacing, base + 20, z * info.spacing];
                const distance = (position[0] - start.origin[0]) ** 2 + (position[2] - start.origin[2]) ** 2;
                if (!best || distance < best.distance) best = { position, distance, terrainHeight: heights[z * width + x] };
              }
              if (!best) throw new Error('No original wet terrain area found for render gate.');
              window.waterTestPlacement = best;
              data = { ...data, bytes: new TextEncoder().encode(JSON.stringify({ ...start, origin: best.position })) };
            }
            this.waterRequests.set(data.id, data.op); super.postMessage(data, ...args);
          }
        };
        const draw = WebGL2RenderingContext.prototype.drawElements;
        WebGL2RenderingContext.prototype.drawElements = function (...args) {
          if (!window.captureWaterPixels || args[0] !== this.TRIANGLES || args[1] !== 6012) return draw.apply(this, args);
          const program = this.getParameter(this.CURRENT_PROGRAM);
          if (!this.getUniformLocation(program, 'baseHeight')) return draw.apply(this, args);
          const size = this.canvas.width * this.canvas.height;
          const before = new Uint8Array(size * 4), after = new Uint8Array(size * 4);
          this.readPixels(0, 0, this.canvas.width, this.canvas.height, this.RGBA, this.UNSIGNED_BYTE, before);
          draw.apply(this, args);
          this.readPixels(0, 0, this.canvas.width, this.canvas.height, this.RGBA, this.UNSIGNED_BYTE, after);
          let changed = 0, topChanged = 0;
          for (let i = 0; i < size; i++) if ([0, 1, 2].some(component => before[i * 4 + component] !== after[i * 4 + component])) {
            changed++; if (i >= size - this.canvas.width * 10) topChanged++;
          }
          window.waterPixelEvidence = { changed, unchanged: size - changed, topChanged, width: this.canvas.width, height: this.canvas.height, glError: this.getError() };
          window.captureWaterPixels = false;
        };
      });
      try {
        await page.goto(`${base}/play.html?drive=1&environment=${environment}`);
        await page.waitForFunction(() => window.atvTerrainStatus?.ready || document.querySelector('#status.error'), null, { timeout: 120000 });
        let state = await page.evaluate(() => window.atvTerrainStatus);
        assert(state.ready, await page.locator('#status').textContent());
        if (environment === 'env01') {
          assert.equal(state.water, null); assert.equal(state.waterRender, null);
        } else {
          await page.evaluate(() => { window.holdWaterPhysics = true; window.captureWaterPixels = true; });
          await page.waitForFunction(() => window.waterPixelEvidence, null, { timeout: 30000 });
          const evidence = await page.evaluate(() => window.waterPixelEvidence);
          state = await page.evaluate(() => window.atvTerrainStatus);
          assert(evidence.changed > 100, 'Water must change pixels in a real wet area');
          assert(evidence.unchanged > 100, 'Depth-tested banks and sky must remain visible');
          assert.equal(evidence.topChanged, 0, 'Water must not cross the sky/horizon boundary');
          assert.equal(evidence.glError, 0);
          assert.equal(state.waterRender.channels, 4); assert.equal(state.waterRender.draws, 1);
          assert.equal(state.waterRender.gpuBytes, 137432);
          assert(Number.isFinite(state.waterRender.baseHeight));
          if (process.env.ATV_WATER_SCREENSHOTS) await page.screenshot({ path: `${process.env.ATV_WATER_SCREENSHOTS}-${environment}${mobile ? '-mobile' : ''}.png` });
          console.log(JSON.stringify({ environment, mobile, ...evidence, water: state.waterRender }));
        }
        assert(state.frame.origin.every(Number.isFinite)); assert.equal(state.glError, 0); assert.deepEqual(errors, []);
      } finally { await page.close(); }
    }
  }
  console.log('PASS: four original water maps draw displaced batched surfaces, banks/sky survive depth testing, dry map stays dry, 390px projection is finite.');
} finally { await browser.close(); }
