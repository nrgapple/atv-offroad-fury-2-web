// Inspection camera and fixtures are derived from the loaded original scene.
import assert from 'node:assert/strict';
import { assertNoErrors, base, launch, newPage, waitForGame, waitForSteps } from './browser/harness.mjs';
const baseline = process.env.ATV_RAIL_BASELINE === '1';
const browser = await launch();
try {
  for (const mobile of [false, true]) {
    const errors = [],
      page = await newPage(
        browser,
        { viewport: mobile ? { width: 390, height: 844 } : { width: 960, height: 600 } },
        errors
      );
    await page.addInitScript(() => {
      const NativeWorker = Worker;
      window.Worker = class extends NativeWorker {
        constructor(...args) {
          super(...args);
          this.ops = new Map();
          this.addEventListener('message', ({ data }) => {
            const op = this.ops.get(data.id);
            this.ops.delete(data.id);
            if (op === 60 && data.bytes && !data.error) {
              const scene = JSON.parse(new TextDecoder().decode(data.bytes));
              window.railModels = scene.models.filter(m => /^trackcurve(?:short)?$/i.test(m.name));
            }
          });
        }
        postMessage(data, ...args) {
          if (data.op === 23 && window.railModels) {
            const m = window.railModels[window.railIndex ?? 0].world_matrix;
            const start = JSON.parse(new TextDecoder().decode(data.bytes));
            start.origin = [m[12], m[13] + 2, m[14]];
            data = { ...data, bytes: new TextEncoder().encode(JSON.stringify(start)) };
          }
          this.ops.set(data.id, data.op);
          super.postMessage(data, ...args);
        }
      };
      const names = new WeakMap(),
        getLocation = WebGL2RenderingContext.prototype.getUniformLocation;
      WebGL2RenderingContext.prototype.getUniformLocation = function (...args) {
        const location = getLocation.apply(this, args);
        if (location) names.set(location, args[1]);
        return location;
      };
      const matrix = WebGL2RenderingContext.prototype.uniformMatrix4fv;
      WebGL2RenderingContext.prototype.uniformMatrix4fv = function (location, transpose, value) {
        const selected = window.railModels?.[window.railIndex ?? 0];
        const p = this.getParameter(this.CURRENT_PROGRAM);
        if (selected && names.get(location) === 'viewProjection') {
          const center = selected.world_matrix.slice(12, 15),
            eye = [center[0], center[1] + 60, center[2] - 90];
          const forward = [0, -60 / Math.hypot(60, 90), 90 / Math.hypot(60, 90)];
          const right = [-1, 0, 0],
            up = [0, forward[2], -forward[1]];
          const view = [
            right[0],
            up[0],
            -forward[0],
            0,
            right[1],
            up[1],
            -forward[1],
            0,
            right[2],
            up[2],
            -forward[2],
            0,
            -right.reduce((s, v, i) => s + v * eye[i], 0),
            -up.reduce((s, v, i) => s + v * eye[i], 0),
            forward.reduce((s, v, i) => s + v * eye[i], 0),
            1
          ];
          const halfWidth = 70,
            halfHeight = (halfWidth * this.canvas.height) / this.canvas.width;
          const projection = [
            1 / halfWidth,
            0,
            0,
            0,
            0,
            1 / halfHeight,
            0,
            0,
            0,
            0,
            -2 / 1999,
            0,
            0,
            0,
            -2001 / 1999,
            1
          ];
          value = new Float32Array(16);
          for (let c = 0; c < 4; c++)
            for (let r = 0; r < 4; r++)
              for (let k = 0; k < 4; k++) value[c * 4 + r] += projection[k * 4 + r] * view[c * 4 + k];
          this.uniform3fv(this.getUniformLocation(p, 'eye'), eye);
        }
        return matrix.call(this, location, transpose, value);
      };
      const draw = WebGL2RenderingContext.prototype.drawElements;
      WebGL2RenderingContext.prototype.drawElements = function (...args) {
        const selected = window.railModels?.[window.railIndex ?? 0];
        if (!window.captureRail || !selected) return draw.apply(this, args);
        const program = this.getParameter(this.CURRENT_PROGRAM),
          loc = this.getUniformLocation(program, 'model');
        if (!loc) return draw.apply(this, args);
        const model = this.getUniform(program, loc);
        if (!selected.world_matrix.every((v, i) => Math.fround(v) === model[i])) return draw.apply(this, args);
        const count = this.canvas.width * this.canvas.height,
          before = new Uint8Array(count * 4),
          after = new Uint8Array(count * 4);
        this.readPixels(0, 0, this.canvas.width, this.canvas.height, this.RGBA, this.UNSIGNED_BYTE, before);
        draw.apply(this, args);
        this.readPixels(0, 0, this.canvas.width, this.canvas.height, this.RGBA, this.UNSIGNED_BYTE, after);
        let changed = 0;
        for (let i = 0; i < count; i++) if ([0, 1, 2].some(c => before[i * 4 + c] !== after[i * 4 + c])) changed++;
        window.railEvidence = {
          name: selected.name,
          index: window.railIndex ?? 0,
          changed,
          triangles: args[1] / 3,
          glError: this.getError()
        };
        window.captureRail = false;
      };
    });
    try {
      await page.goto(`${base}/play.html?drive=1&environment=env08`);
      await waitForGame(page);
      assert.equal(await page.evaluate(() => window.railModels.length), 12);
      const rows = [];
      for (let i = 0; i < 12; i++) {
        await page.evaluate(index => {
          window.railIndex = index;
          document.querySelector('#reset').click();
        }, i);
        await waitForSteps(page, 21);
        await page.evaluate(() => {
          window.railEvidence = null;
          window.captureRail = true;
        });
        await page.waitForFunction(() => window.railEvidence, null, { timeout: 15000 });
        const row = await page.evaluate(() => window.railEvidence);
        rows.push(row);
        assert.equal(row.glError, 0);
        if (!baseline) assert(row.changed > 20, `curve ${i} did not change sufficient pixels: ${row.changed}`);
        if (i === 0 && process.env.ATV_RAIL_SCREENSHOTS)
          await page.screenshot({ path: `${process.env.ATV_RAIL_SCREENSHOTS}${mobile ? '-mobile' : ''}.png` });
      }
      assertNoErrors(errors);
      console.log(JSON.stringify({ mobile, baseline, curves: rows }));
    } finally {
      await page.close();
    }
  }
  console.log(
    baseline
      ? 'Baseline rail inspection recorded.'
      : 'PASS: all twelve original curved rail placements change pixels at desktop and390px with no JS/GL errors. Inspection views are separate from the gameplay camera.'
  );
} finally {
  await browser.close();
}
