// Original-content render gate; source resources stay in the separate runtime manifest.
// ATV_TEST_URL=<host/public HTTPS> PLAYWRIGHT_MODULE=<module> node tools/test_water_render.mjs
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { readFile } from 'node:fs/promises';
const browserName = process.env.ATV_BROWSER || 'chromium';
const playwright = createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || 'playwright');
assert(['chromium', 'webkit'].includes(browserName));
const base = process.env.ATV_TEST_URL || 'http://127.0.0.1:4173';
const candidateModule = process.env.ATV_WATER_MODULE ? await readFile(process.env.ATV_WATER_MODULE, 'utf8') : null;
const browser = await playwright[browserName].launch({ headless: true });
try {
  for (const environment of (process.env.ATV_WATER_MAPS || 'env01,env04,env05,env07,env08,free04,nat01').split(',')) {
    for (const mobile of environment === 'env01' ? [false] : [false, true]) {
      const page = await browser.newPage({ viewport: mobile ? { width: 390, height: 844 } : { width: 960, height: 540 }, deviceScaleFactor: mobile ? 3 : 1, isMobile: mobile, hasTouch: mobile });
      if (candidateModule) await page.route(url => url.pathname === '/water.js', route => route.fulfill({ contentType: 'text/javascript', body: candidateModule }));
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
              if (op === 66) window.waterTestSource = JSON.parse(new TextDecoder().decode(data.bytes));
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
          if (!window.captureWaterPixels || args[0] !== this.TRIANGLES) return draw.apply(this, args);
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
          window.waterDrawInputs = { camera: { ...window.atvTerrainStatus.camera }, viewProjection: Array.from(this.getUniform(program, this.getUniformLocation(program, 'viewProjection'))), focalLength: 1 / this.getUniform(program, this.getUniformLocation(program, 'projection'))[1], aspect: this.canvas.width / this.canvas.height, farDistance: this.getUniform(program, this.getUniformLocation(program, 'farDistance')), fogColor: Array.from(this.getUniform(program, this.getUniformLocation(program, 'fogColor'))) };
          window.waterPixelEvidence = { changed, unchanged: size - changed, topChanged, width: this.canvas.width, height: this.canvas.height, indices: args[1], glError: this.getError() };
          window.captureWaterPixels = false;
        };
      });
      try {
        await page.goto(`${base}/play.html?drive=1&environment=${environment}`, { waitUntil: 'domcontentloaded', timeout: 120000 });
        await page.waitForFunction(() => window.atvTerrainStatus?.ready || document.querySelector('#status.error'), null, { timeout: 120000 });
        await page.waitForFunction(() => document.querySelector('#loading-screen')?.hidden && window.atvTerrainStatus?.simulationSteps >= 3, null, { timeout: 120000 });
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
          assert.equal(state.waterRender.channels, environment === 'nat01' ? 8 : 4); assert.equal(state.waterRender.draws, 1);
          assert(state.waterRender.gpuBytes > 0 && state.waterRender.gpuBytes < 3 * 1024 * 1024, 'Bounded resident mesh and original textures');
          assert(evidence.indices > 6012 && evidence.indices < 144000, 'Dense depth axis within resident vertex budget');
          assert(Number.isFinite(state.waterRender.baseHeight));
          const resident = await page.evaluate(async () => {
            const { WaterRenderer } = await import('/water.js');
            const canvas = document.createElement('canvas'); canvas.width = 390; canvas.height = 844;
            const gl = canvas.getContext('webgl2'), renderer = new WaterRenderer(gl, window.waterTestSource);
            gl.viewport(0, 0, canvas.width, canvas.height); gl.enable(gl.DEPTH_TEST);
            let uploads = 0;
            for (const name of ['bufferData', 'bufferSubData', 'texImage2D', 'texSubImage2D', 'texImage3D', 'texSubImage3D', 'generateMipmap']) {
              const original = gl[name].bind(gl); gl[name] = (...args) => { uploads++; return original(...args); };
            }
            for (let frame = 0; frame < 12; frame++) renderer.draw(window.waterDrawInputs);
            const result = { uploads, error: gl.getError(), draws: renderer.draws, vertices: renderer.gridColumns * (renderer.gridRows + 1) };
            renderer.dispose(); return result;
          });
          assert.equal(resident.uploads, 0, 'Ordinary frames reuse resident mesh, heights, gradients and mip chains');
          assert.equal(resident.error, 0); assert.equal(resident.draws, 1); assert(resident.vertices <= 8000);
          if (process.env.ATV_WATER_SCREENSHOTS) await page.screenshot({ path: `${process.env.ATV_WATER_SCREENSHOTS}-${environment}${mobile ? '-mobile' : ''}.png` });
          console.log(JSON.stringify({ browser: browserName, environment, mobile, ...evidence, water: state.waterRender }));
          if (mobile) {
            const frame = state.water;
            await page.setViewportSize({ width: 844, height: 390 });
            await page.evaluate(() => { window.waterPixelEvidence = null; window.captureWaterPixels = true; });
            await page.waitForFunction(() => window.waterPixelEvidence?.width > window.waterPixelEvidence?.height);
            const resized = await page.evaluate(() => window.waterPixelEvidence);
            assert(resized.changed > 100); assert.equal(resized.glError, 0);
            const bytes = (await page.evaluate(() => window.atvTerrainStatus.waterRender)).gpuBytes;
            assert(bytes < 3 * 1024 * 1024);
            await page.evaluate(() => { window.waterContext = document.querySelector('#game').getContext('webgl2').getExtension('WEBGL_lose_context'); if (!window.waterContext) throw Error('Missing context-loss extension'); window.waterContext.loseContext(); });
            await page.waitForFunction(() => !window.atvTerrainStatus.ready);
            await page.waitForTimeout(100);
            await page.evaluate(() => window.waterContext.restoreContext());
            await page.waitForFunction(() => window.atvTerrainStatus.ready && document.querySelector('#loading-screen').hidden);
            await page.evaluate(() => { window.waterPixelEvidence = null; window.captureWaterPixels = true; });
            await page.waitForFunction(() => window.waterPixelEvidence);
            assert.equal((await page.evaluate(() => window.waterPixelEvidence)).glError, 0);
            assert.equal((await page.evaluate(() => window.atvTerrainStatus.waterRender)).gpuBytes, bytes);
            assert.deepEqual(await page.evaluate(() => window.atvTerrainStatus.water), frame, 'Restoration retains the completed shared field frame');
          }
        }
        assert(state.frame.origin.every(Number.isFinite)); assert.equal(state.glError, 0); assert.deepEqual(errors, []);
      } finally { await page.close(); }
    }
  }
  console.log('PASS: selected original water maps draw bounded depth-dense surfaces, banks/sky survive, portrait/landscape/context restoration retain the shared field, dry map stays dry.');
} finally { await browser.close(); }
