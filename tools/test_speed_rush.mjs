// Actual WebGL pixel proof plus native throttle-input driving in four-tick batches.
// Inputs remain held across resets; no injected velocity, pose or speed values.
import assert from 'node:assert/strict';
import { writeFile } from 'node:fs/promises';
import { assertNoErrors, base, button, launch, newPage } from './browser/harness.mjs';
const browser = await launch();
let page;
try {
  const errors = [];
  page = await newPage(
    browser,
    {
      viewport: {
        width: Number(process.env.ATV_RUSH_WIDTH || 1280),
        height: Number(process.env.ATV_RUSH_HEIGHT || 720)
      }
    },
    errors
  );
  await page.addInitScript(() => {
    const post = Worker.prototype.postMessage;
    Worker.prototype.postMessage = function (message, ...args) {
      if (message.op === 50 && window.rushDriving) {
        const source = JSON.parse(new TextDecoder().decode(message.bytes));
        source.count = 4;
        source.input = {
          ...source.input,
          throttle: 1,
          brake: 0,
          steer: 0,
          lean: 0,
          reverse: false,
          touch_drive: null,
          preload_axis: null,
          stunt_modifiers: 0,
          stunt_x: 0,
          stunt_y: 0
        };
        message = { ...message, bytes: new TextEncoder().encode(JSON.stringify(source)) };
      }
      return post.call(this, message, ...args);
    };
    window.rushSamples = [];
    const sample = () => {
      const s = window.atvTerrainStatus;
      if (s?.frame)
        window.rushSamples.push({
          rush: s.speedRush,
          speed: s.speedRush.speed,
          simulationSpeed: s.frame.speed_magnitude,
          phase: s.frame.wreck.phase,
          air: !s.frame.grounded.some(Boolean)
        });
      requestAnimationFrame(sample);
    };
    requestAnimationFrame(sample);
  });
  await page.goto(`${base}/play.html?mode=single&map=env01`);
  await page.waitForFunction(
    () => window.atvTerrainStatus?.simulation && document.querySelector('#frontend').hidden,
    null,
    { timeout: 120000 }
  );
  const pixelProof = await page.evaluate(async () => {
    const { SpeedRush } = await import('/speed-rush.js');
    const canvas = document.createElement('canvas');
    canvas.width = 256;
    canvas.height = 256;
    const gl = canvas.getContext('webgl2', { antialias: true, alpha: false, preserveDrawingBuffer: true });
    const effect = new SpeedRush(gl);
    const pixels = () => {
      const p = new Uint8Array(256 * 256 * 4);
      gl.readPixels(0, 0, 256, 256, gl.RGBA, gl.UNSIGNED_BYTE, p);
      return p;
    };
    const draw = () => {
      gl.viewport(0, 0, 256, 256);
      gl.enable(gl.SCISSOR_TEST);
      for (let y = 0; y < 256; y += 8)
        for (let x = 0; x < 256; x += 8) {
          gl.scissor(x, y, 8, 8);
          gl.clearColor(((x + y) / 8) % 2, 0.25, 0.5, 1);
          gl.clear(gl.COLOR_BUFFER_BIT);
        }
      gl.disable(gl.SCISSOR_TEST);
    };
    draw();
    const before = pixels();
    effect.draw({ speed: 120, intensity: 'high', active: true, dt: 2, width: 256, height: 256, focus: [0.5, 0.5] });
    const after = pixels();
    let central = 0,
      edge = 0;
    for (let y = 0; y < 256; y++)
      for (let x = 0; x < 256; x++) {
        const delta = Math.abs(before[(y * 256 + x) * 4] - after[(y * 256 + x) * 4]);
        if (Math.hypot(x - 128, y - 128) < 24) central += delta;
        else if (x < 32 || x > 224 || y < 32 || y > 224) edge += delta;
      }
    draw();
    effect.draw({ speed: 120, intensity: 'off', active: true, dt: 1 / 60, width: 256, height: 256 });
    const disabled = pixels();
    const equal = before.every((v, i) => v === disabled[i]);
    const error = gl.getError();
    effect.dispose();
    // Stencil ref 1 (the player's ATV) stays pixel-identical inside a streaked edge.
    const masked = document.createElement('canvas');
    masked.width = 256;
    masked.height = 256;
    const mgl = masked.getContext('webgl2', {
      antialias: true,
      alpha: false,
      stencil: true,
      preserveDrawingBuffer: true
    });
    const guarded = new SpeedRush(mgl);
    const read = () => {
      const p = new Uint8Array(256 * 256 * 4);
      mgl.readPixels(0, 0, 256, 256, mgl.RGBA, mgl.UNSIGNED_BYTE, p);
      return p;
    };
    mgl.viewport(0, 0, 256, 256);
    mgl.enable(mgl.SCISSOR_TEST);
    for (let y = 0; y < 256; y += 8)
      for (let x = 0; x < 256; x += 8) {
        mgl.scissor(x, y, 8, 8);
        mgl.clearColor(((x + y) / 8) % 2, 0.25, 0.5, 1);
        mgl.clear(mgl.COLOR_BUFFER_BIT);
      }
    mgl.scissor(0, 0, 256, 256);
    mgl.clearStencil(0);
    mgl.clear(mgl.STENCIL_BUFFER_BIT);
    mgl.scissor(0, 0, 48, 48);
    mgl.clearStencil(1);
    mgl.clear(mgl.STENCIL_BUFFER_BIT);
    mgl.disable(mgl.SCISSOR_TEST);
    const plain = read();
    guarded.draw({ speed: 120, intensity: 'high', active: true, dt: 2, width: 256, height: 256, focus: [0.5, 0.5] });
    const streaked = read();
    let protectedDelta = 0,
      openDelta = 0;
    for (let y = 0; y < 256; y++)
      for (let x = 0; x < 256; x++) {
        const delta = Math.abs(plain[(y * 256 + x) * 4] - streaked[(y * 256 + x) * 4]);
        if (x < 48 && y < 48) protectedDelta += delta;
        else if (x > 208 && y > 208) openDelta += delta;
      }
    const stencil = guarded.snapshot().stencil;
    guarded.dispose();
    return {
      central,
      edge,
      equal,
      error,
      disposed: effect.snapshot().textureBytes,
      protectedDelta,
      openDelta,
      stencil
    };
  });
  assert.equal(pixelProof.central, 0);
  assert(pixelProof.stencil);
  assert.equal(pixelProof.protectedDelta, 0);
  assert(pixelProof.openDelta > 10000);
  assert(pixelProof.edge > 10000);
  assert(pixelProof.equal);
  assert.equal(pixelProof.error, 0);
  assert.equal(pixelProof.disposed, 0);
  // Orbiting at rest cannot start blur.
  await page.locator('#game').focus();
  await page.mouse.move(120, 200);
  await page.mouse.down();
  await page.mouse.move(250, 250, { steps: 6 });
  await page.mouse.up();
  assert.equal(await page.evaluate(() => window.atvTerrainStatus.speedRush.draws), 0);
  await page.locator('#game').focus();
  await page.keyboard.press('Escape');
  await button(page, 'Reset ATV').click();
  await page.waitForFunction(() => document.querySelector('#frontend').hidden && window.atvTerrainStatus?.simulation);
  await page.locator('#game').focus();
  await page.evaluate(() => (window.rushDriving = true));
  await page.keyboard.down('KeyW');
  await page.waitForFunction(
    () => window.atvTerrainStatus?.speedRush.draws === 1 && window.atvTerrainStatus.speedRush.amount > 0.001,
    null,
    { timeout: 60000 }
  );
  assert.equal(await page.evaluate(() => window.atvTerrainStatus.speedRush.failed), false);
  assert.equal(await page.evaluate(() => document.querySelector('#game').getContext('webgl2').getError()), 0);
  console.log('Pixel and riding pass');
  if (process.env.ATV_RUSH_SCREENSHOT) await page.screenshot({ path: process.env.ATV_RUSH_SCREENSHOT });
  await page.keyboard.up('KeyW');
  await page.evaluate(() => (window.rushDriving = false));
  await page.keyboard.press('Escape');
  await button(page, 'Settings').click();
  await button(page, 'Display').click();
  await button(page, 'Speed rush: Normal').click();
  await button(page, 'Speed rush: High').click();
  assert.equal(await page.evaluate(() => localStorage.getItem('atv-speed-rush')), 'off');
  assert.equal(await page.evaluate(() => window.atvTerrainStatus.speedRush.amount), 0);
  await page.reload();
  await page.waitForFunction(
    () => window.atvTerrainStatus?.simulation && document.querySelector('#frontend').hidden,
    null,
    { timeout: 120000 }
  );
  assert.equal(await page.evaluate(() => window.atvTerrainStatus.speedRush.setting), 'off');
  // Explicitly re-enable before testing real GPU context restoration.
  await page.keyboard.press('Escape');
  await button(page, 'Settings').click();
  await button(page, 'Display').click();
  await button(page, 'Speed rush: Off').click();
  for (let i = 0; i < 2; i++) await button(page, 'Back').click();
  await button(page, 'Resume').click();
  await page.evaluate(() => {
    window.rushContext = document.querySelector('#game').getContext('webgl2').getExtension('WEBGL_lose_context');
    window.rushContext.loseContext();
  });
  await page.locator('#loading-screen').waitFor({ state: 'visible' });
  await page.evaluate(() => window.rushContext.restoreContext());
  await page.locator('#loading-screen').waitFor({ state: 'hidden' });
  await page.locator('#game').focus();
  await page.keyboard.press('Escape');
  await button(page, 'Reset ATV').click();
  await page.waitForFunction(() => document.querySelector('#frontend').hidden && window.atvTerrainStatus?.simulation);
  await page.locator('#game').focus();
  await page.evaluate(() => (window.rushDriving = true));
  await page.keyboard.down('KeyW');
  await page.waitForFunction(() => window.atvTerrainStatus?.speedRush.draws === 1, null, { timeout: 60000 });
  await page.keyboard.up('KeyW');
  await page.evaluate(() => (window.rushDriving = false));
  const samples = await page.evaluate(() => window.rushSamples);
  assert(samples.some(s => s.rush.draws === 1 && (s.speed * 3600) / 5280 > 40));
  assert(samples.filter(s => (s.speed * 3600) / 5280 <= 40).every(s => s.rush.draws === 0));
  assert.equal(await page.evaluate(() => window.atvTerrainStatus.glError), 0);
  assertNoErrors(errors);
  console.log(
    'PASS:',
    JSON.stringify({ pixelProof, samples: samples.length, airRush: samples.filter(s => s.air && s.rush.draws).length }),
    'real riding above 40mph, turn-at-rest off, persistent intensity, context recovery'
  );
} catch (error) {
  const failure = await page.evaluate(() => ({
    status: window.atvTerrainStatus,
    maxSpeed: Math.max(...window.rushSamples.map(s => s.speed)),
    last: window.rushSamples.slice(-3)
  }));
  await writeFile('work/verification/speed-failure.json', JSON.stringify(failure, null, 2));
  console.error({ input: failure.status.input, rush: failure.status.speedRush, maxSpeed: failure.maxSpeed });
  await page.screenshot({ path: 'work/verification/speed-failure.png' });
  throw error;
} finally {
  await browser.close();
}
