// Opt-in source-content gate. No textures, models or source fixture data in Git.
import assert from 'node:assert/strict';
import { assertNoErrors, base, launch, newPage, waitForGame } from './browser/harness.mjs';
const environment = process.env.ATV_AMBIENT_ENVIRONMENT || 'env07';
assert(['env07', 'free06', 'env05', 'env04'].includes(environment));
const browser = await launch();
const reports = [];
try {
  for (const mobile of [false, true]) {
    const errors = [];
    const page = await newPage(
      browser,
      { viewport: mobile ? { width: 390, height: 844 } : { width: 960, height: 600 } },
      errors
    );
    await page.addInitScript(environment => {
      const WorkerBase = Worker;
      window.ambientPending = 0;
      window.Worker = class extends WorkerBase {
        constructor(...args) {
          super(...args);
          this.ops = new Map();
          this.addEventListener('message', ({ data }) => {
            const op = this.ops.get(data.id);
            this.ops.delete(data.id);
            if (op === 50) window.ambientPending--;
            if (op === 131 && environment !== 'env07' && data.bytes) {
              const plan = JSON.parse(new TextDecoder().decode(data.bytes)).plan;
              window.ambientTestPlacement = plan.placements.find(p =>
                environment === 'free06'
                  ? p.type_id === 0 && p.filename === 'grillsmokeblowing.emt'
                  : environment === 'env04'
                    ? p.type_id === 12
                    : p.type_id === 11 && p.filename === 'fountain.emt'
              );
            }
          });
        }
        postMessage(data, ...args) {
          if (data.op === 23 && window.ambientTestPlacement) {
            const payload = JSON.parse(new TextDecoder().decode(data.bytes)),
              position = window.ambientTestPlacement.position;
            payload.origin = [position[0], position[1] + 3, position[2] - 16];
            payload.yaw = 0;
            data.bytes = new TextEncoder().encode(JSON.stringify(payload));
          }
          this.ops.set(data.id, data.op);
          if (data.op === 50) window.ambientPending++;
          super.postMessage(data, ...args);
        }
      };
      const dataUpload = WebGL2RenderingContext.prototype.bufferData;
      WebGL2RenderingContext.prototype.bufferData = function (target, data, ...args) {
        const result = dataUpload.call(this, target, data, ...args);
        if (environment === 'env05' && data instanceof Float32Array && data.length === 4800 * 20) {
          window.lastAmbientStreamBuffer = this.getParameter(this.ARRAY_BUFFER_BINDING);
          window.lastAmbientStreamCPU = data;
        }
        return result;
      };
      const draw = WebGL2RenderingContext.prototype.drawArraysInstanced;
      WebGL2RenderingContext.prototype.drawArraysInstanced = function (...args) {
        const p = this.getParameter(this.CURRENT_PROGRAM);
        if (p && this.getUniformLocation(p, 'ambientStreamAtlas'))
          window.lastAmbientStreamBuffer = this.getVertexAttrib(1, this.VERTEX_ATTRIB_ARRAY_BUFFER_BINDING);
        const atlas =
          p && (this.getUniformLocation(p, 'ambientAtlas') || this.getUniformLocation(p, 'ambientStreamAtlas'));
        if (!atlas || !window.captureAmbientPixels) return draw.apply(this, args);
        if (environment === 'env04') {
          const buffer = this.getVertexAttrib(1, this.VERTEX_ATTRIB_ARRAY_BUFFER_BINDING),
            old = this.getParameter(this.ARRAY_BUFFER_BINDING),
            data = new Float32Array(args[3] * 10);
          this.bindBuffer(this.ARRAY_BUFFER, buffer);
          this.getBufferSubData(this.ARRAY_BUFFER, 0, data);
          const original = data.slice(),
            isFog = i => original[i * 10 + 3] >= 30 && original[i * 10 + 3] <= 50;
          for (let i = 0; i < args[3]; i++) if (isFog(i)) data[i * 10 + 8] = 0;
          this.bufferSubData(this.ARRAY_BUFFER, 0, data);
          draw.apply(this, args);
          const before = new Uint8Array(this.canvas.width * this.canvas.height * 4),
            after = new Uint8Array(before.length);
          this.readPixels(0, 0, this.canvas.width, this.canvas.height, this.RGBA, this.UNSIGNED_BYTE, before);
          data.set(original);
          for (let i = 0; i < args[3]; i++) if (!isFog(i)) data[i * 10 + 8] = 0;
          this.bufferSubData(this.ARRAY_BUFFER, 0, data);
          draw.apply(this, args);
          this.readPixels(0, 0, this.canvas.width, this.canvas.height, this.RGBA, this.UNSIGNED_BYTE, after);
          this.bufferSubData(this.ARRAY_BUFFER, 0, original);
          this.bindBuffer(this.ARRAY_BUFFER, old);
          let changed = 0;
          for (let i = 0; i < before.length; i += 4)
            if (before[i] !== after[i] || before[i + 1] !== after[i + 1] || before[i + 2] !== after[i + 2]) changed++;
          if (changed) {
            window.ambientPixels = { changed, glError: this.getError(), isolatedFog: true };
            window.captureAmbientPixels = false;
          }
          return;
        }
        const before = new Uint8Array(this.canvas.width * this.canvas.height * 4),
          after = new Uint8Array(before.length);
        this.readPixels(0, 0, this.canvas.width, this.canvas.height, this.RGBA, this.UNSIGNED_BYTE, before);
        draw.apply(this, args);
        this.readPixels(0, 0, this.canvas.width, this.canvas.height, this.RGBA, this.UNSIGNED_BYTE, after);
        let changed = 0;
        for (let i = 0; i < before.length; i += 4)
          if (before[i] !== after[i] || before[i + 1] !== after[i + 1] || before[i + 2] !== after[i + 2]) changed++;
        if (changed) {
          window.ambientPixels = { changed, glError: this.getError() };
          window.captureAmbientPixels = false;
        }
      };
    }, environment);
    await page.goto(`${base}/play.html?drive=1&environment=${environment}&quad=${process.env.ATV_AMBIENT_QUAD || 1}`);
    await waitForGame(page);
    await page.locator('#game').focus();
    await page.evaluate(() => (window.captureAmbientPixels = true));
    await page.waitForFunction(
      () =>
        window.ambientPixels &&
        (window.atvTerrainStatus.ambientEffects.frame.total_emitted > 20 ||
          window.atvTerrainStatus.ambientEffects.frame.gpu_total > 20),
      null,
      { timeout: 30000 }
    );
    const active = await page.evaluate(() => structuredClone(window.atvTerrainStatus.ambientEffects));
    assert.equal(active.plan.declared, environment === 'free06' ? 15 : environment === 'env04' ? 2 : 5);
    assert.equal(
      active.active,
      environment === 'free06' ? 21 : environment === 'env05' ? 10 : environment === 'env04' ? 2 : 5
    );
    assert.deepEqual(active.unsupported, []);
    assert(active.frame.particles.length <= 600);
    assert(active.draws <= 2);
    assert.equal(active.gpuBytes, environment === 'env05' ? 515120 : 286192);
    assert(environment === 'env05' ? active.gpuSlots > 0 : active.visible > 0);
    if (environment === 'env04') {
      const fog = active.frame.particles.filter(p => p.fade_distance === 20);
      assert.equal(fog.length, 200);
      assert(fog.every(p => p.alpha === 0.25 && p.size >= 30 && p.size <= 50));
      assert(active.frame.particles.filter(p => !p.fade_distance).every(p => p.size < 30));
      await page.waitForFunction(t => window.atvTerrainStatus.ambientEffects.frame.time > t + 2, active.frame.time);
      const later = await page.evaluate(() =>
        window.atvTerrainStatus.ambientEffects.frame.particles.filter(p => p.fade_distance === 20)
      );
      assert.equal(later.length, 200);
      assert.deepEqual(
        later.map(p => [p.size, p.alpha, p.frame, p.mirror]),
        fog.map(p => [p.size, p.alpha, p.frame, p.mirror])
      );
      assert(later.some((p, i) => p.position.some((v, j) => v !== fog[i].position[j])));
    }
    if (environment === 'env05') {
      await page.waitForFunction(() => window.atvTerrainStatus.ambientEffects.frame.gpu_total > 5000);
      const streams = await page.evaluate(() => window.atvTerrainStatus.ambientEffects);
      assert.equal(streams.gpuSlots, 4800);
      assert(streams.gpuWraps > 0);
      assert(streams.gpuUploadCalls <= 2);
    }
    await page.keyboard.press('Escape');
    await page.getByRole('button', { name: 'Resume', exact: true }).waitFor();
    await page.waitForFunction(() => ambientPending === 0);
    const paused = await page.evaluate(() => structuredClone(window.atvTerrainStatus.ambientEffects.frame));
    // Time-based on purpose: proves the paused particle frame does not change over wall-clock time.
    await page.waitForTimeout(300);
    assert.deepEqual(await page.evaluate(() => window.atvTerrainStatus.ambientEffects.frame), paused);
    await page.evaluate(() => document.querySelector('#reset').click());
    // Time-based on purpose: reset must leave the paused frame unchanged, so there is no condition to wait for.
    await page.waitForTimeout(400);
    const reset = await page.evaluate(() => window.atvTerrainStatus.ambientEffects.frame);
    assert.deepEqual(reset, paused, 'ATV reset preserves world particles and clock');
    await page.getByRole('button', { name: 'Resume', exact: true }).click();
    await page.waitForFunction(t => window.atvTerrainStatus.ambientEffects.frame.time > t, paused.time);
    await page.keyboard.press('Escape');
    await page.getByRole('button', { name: 'Resume', exact: true }).waitFor();
    await page.waitForFunction(() => ambientPending === 0);
    const before = await page.evaluate(() => structuredClone(window.atvTerrainStatus.ambientEffects.frame));
    const streamBuffer = async cpu =>
      page.evaluate(cpu => {
        const gl = document.querySelector('#game').getContext('webgl2'),
          old = gl.getParameter(gl.ARRAY_BUFFER_BINDING),
          words = new Uint32Array(4800 * 20);
        if (cpu)
          words.set(
            new Uint32Array(
              lastAmbientStreamCPU.buffer,
              lastAmbientStreamCPU.byteOffset,
              lastAmbientStreamCPU.byteLength / 4
            )
          );
        else {
          gl.bindBuffer(gl.ARRAY_BUFFER, lastAmbientStreamBuffer);
          gl.getBufferSubData(gl.ARRAY_BUFFER, 0, words);
          gl.bindBuffer(gl.ARRAY_BUFFER, old);
        }
        let hash = 2166136261;
        for (const word of words) hash = Math.imul(hash ^ word, 16777619);
        window.previousStreamBuffer = lastAmbientStreamBuffer;
        return { hash, error: gl.getError() };
      }, cpu);
    const previousBuffer = environment === 'env05' ? await streamBuffer(true) : null;
    await page.evaluate(() => {
      window.ambientLost = document.querySelector('#game').getContext('webgl2').getExtension('WEBGL_lose_context');
      ambientLost.loseContext();
    });
    await page.waitForFunction(() => !window.atvTerrainStatus.ready);
    // Time-based on purpose: keeps the WebGL context lost briefly before restoring it.
    await page.waitForTimeout(100);
    await page.evaluate(() => ambientLost.restoreContext());
    await page.waitForFunction(() => window.atvTerrainStatus.ready);
    assert.deepEqual(await page.evaluate(() => window.atvTerrainStatus.ambientEffects.frame), before);
    if (environment === 'env05') {
      await page.waitForFunction(() => lastAmbientStreamBuffer !== previousStreamBuffer);
      assert.deepEqual(
        await streamBuffer(),
        previousBuffer,
        'context restoration uploads every retained fountain ring slot, including pending births'
      );
    }
    await page.getByRole('button', { name: 'Resume', exact: true }).click();
    await page.waitForFunction(t => window.atvTerrainStatus.ambientEffects.frame.time > t, before.time);
    assertNoErrors(errors);
    assert.equal(await page.evaluate(() => document.querySelector('#game').getContext('webgl2').getError()), 0);
    if (process.env.ATV_AMBIENT_SCREENSHOTS)
      await page.screenshot({ path: `${process.env.ATV_AMBIENT_SCREENSHOTS}${mobile ? '-mobile' : ''}.png` });
    const report = {
      environment,
      testStart:
        environment === 'env07'
          ? 'native spawn'
          : environment === 'env05'
            ? 'source fountain placement'
            : environment === 'env04'
              ? 'source fog volume placement'
              : 'source static grill placement',
      mobile,
      pixels: await page.evaluate(() => window.ambientPixels),
      active: active.active,
      particles: active.frame.particles.length,
      draws: active.draws,
      gpuBytes: active.gpuBytes
    };
    reports.push(report);
    console.log(JSON.stringify(report));
    await page.close();
  }
  console.log(
    'PASS: selected original ambient effect changes actual desktop/390px pixels; bounded draws, pause, ATV reset and graphics restoration retain world clock/state. Other emitter families and PS2 hardware are separate gates.'
  );
} finally {
  await browser.close();
}
