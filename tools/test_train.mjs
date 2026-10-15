// Original train integration; test placement comes from decoded route/MOT output.
import assert from 'node:assert/strict';
import { assertNoErrors, base, button, launch, newPage, waitForGame } from './browser/harness.mjs';
const browser = await launch();
try {
  for (const mobile of [false, true]) {
    const errors = [];
    const page = await newPage(
      browser,
      { viewport: mobile ? { width: 390, height: 844 } : { width: 960, height: 600 } },
      errors
    );
    await page.addInitScript(() => {
      const NativeWorker = Worker;
      window.Worker = class extends NativeWorker {
        constructor(...args) {
          super(...args);
          this.trainOps = new Map();
          this.addEventListener('message', ({ data }) => {
            const op = this.trainOps.get(data.id);
            this.trainOps.delete(data.id);
            if (op === 67 && data.bytes && !data.error)
              window.originalTrainStart = JSON.parse(new TextDecoder().decode(data.bytes));
          });
        }
        postMessage(data, ...args) {
          if (data.op === 23 && window.originalTrainStart) {
            const start = JSON.parse(new TextDecoder().decode(data.bytes));
            const p = window.originalTrainStart.wheel_matrices[0].slice(12, 15);
            // A test camera beside the real train; never an invented train route.
            start.origin = [p[0] + 35, p[1] + 2, p[2] + 20];
            start.yaw = -Math.PI / 2;
            data = { ...data, bytes: new TextEncoder().encode(JSON.stringify(start)) };
          }
          this.trainOps.set(data.id, data.op);
          super.postMessage(data, ...args);
        }
      };
      const draw = WebGL2RenderingContext.prototype.drawElements;
      WebGL2RenderingContext.prototype.drawElements = function (...args) {
        const cars = window.atvTerrainStatus?.train?.frame?.cars;
        if (!window.captureTrainPixels || !cars || args[0] !== this.TRIANGLES) return draw.apply(this, args);
        const program = this.getParameter(this.CURRENT_PROGRAM),
          model = this.getUniformLocation(program, 'model');
        if (!model) return draw.apply(this, args);
        const matrix = this.getUniform(program, model);
        if (!cars.some(car => car.matrix.every((v, i) => v === matrix[i]))) return draw.apply(this, args);
        const pixels = this.canvas.width * this.canvas.height,
          before = new Uint8Array(pixels * 4),
          after = new Uint8Array(pixels * 4);
        this.readPixels(0, 0, this.canvas.width, this.canvas.height, this.RGBA, this.UNSIGNED_BYTE, before);
        draw.apply(this, args);
        this.readPixels(0, 0, this.canvas.width, this.canvas.height, this.RGBA, this.UNSIGNED_BYTE, after);
        let changed = 0;
        for (let i = 0; i < pixels; i++) if ([0, 1, 2].some(c => before[i * 4 + c] !== after[i * 4 + c])) changed++;
        if (changed > 100) {
          window.trainPixelEvidence = { changed, glError: this.getError() };
          window.captureTrainPixels = false;
        }
      };
    });
    try {
      await page.goto(`${base}/play.html?drive=1&environment=env08`);
      await waitForGame(page);
      await page.waitForFunction(() => window.atvTerrainStatus.train?.time > 0.5, null, { timeout: 30000 });
      await page.evaluate(() => {
        window.captureTrainPixels = true;
      });
      await page.waitForFunction(() => window.trainPixelEvidence, null, { timeout: 30000 });
      const first = await page.evaluate(() => structuredClone(window.atvTerrainStatus.train));
      assert.equal(first.cars, 11);
      assert.equal(first.wheelNodes, 5);
      assert(first.draws > 0);
      assert(first.triangles > 0);
      await page.waitForFunction(time => window.atvTerrainStatus.train.time > time + 1, first.time, { timeout: 30000 });
      const moving = await page.evaluate(() => structuredClone(window.atvTerrainStatus.train));
      assert.notDeepEqual(first.frame.cars[0].matrix, moving.frame.cars[0].matrix);
      assert.notDeepEqual(first.frame.wheel_matrices, moving.frame.wheel_matrices);
      await page.keyboard.press('Escape');
      await button(page, 'Resume').waitFor();
      // Time-based on purpose: let any in-flight step settle, then prove the paused train clock does not advance.
      await page.waitForTimeout(100);
      const paused = await page.evaluate(() => window.atvTerrainStatus.train.time);
      await page.waitForTimeout(300);
      assert.equal(await page.evaluate(() => window.atvTerrainStatus.train.time), paused);
      await button(page, 'Resume').click();
      await page.waitForFunction(time => window.atvTerrainStatus.train.time > time, paused);
      await page.evaluate(() => {
        window.trainLose = document.querySelector('#game').getContext('webgl2').getExtension('WEBGL_lose_context');
        window.trainLose.loseContext();
      });
      await page.waitForFunction(() => !window.atvTerrainStatus.ready);
      // Kept: brief pause after context loss before restoring; no observable readiness signal.
      await page.waitForTimeout(100);
      await page.evaluate(() => window.trainLose.restoreContext());
      await page.waitForFunction(() => window.atvTerrainStatus.ready);
      await page.waitForFunction(time => window.atvTerrainStatus.train.time > time, paused);
      assert.equal(await page.evaluate(() => document.querySelector('#game').getContext('webgl2').getError()), 0);
      const evidence = await page.evaluate(() => window.trainPixelEvidence);
      assert.equal(evidence.glError, 0);
      assertNoErrors(errors);
      if (process.env.ATV_TRAIN_SCREENSHOTS)
        await page.screenshot({ path: `${process.env.ATV_TRAIN_SCREENSHOTS}${mobile ? '-mobile' : ''}.png` });
      console.log(
        JSON.stringify({ mobile, evidence, draws: first.draws, triangles: first.triangles, time: moving.time })
      );
    } finally {
      await page.close();
    }
  }
  console.log(
    'PASS: original train changes pixels and body/wheel poses, pauses and resumes, and restores graphics at desktop/390px. Dynamic train contacts/audio/smoke remain separate gates.'
  );
} finally {
  await browser.close();
}
