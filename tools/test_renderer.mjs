// Opt-in real-disc renderer/simulation verification. No disc data leaves the browser.
// ATV_TEST_ISO=/private/disc.iso PLAYWRIGHT_MODULE=/path/to/playwright node tools/test_renderer.mjs
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const iso = process.env.ATV_TEST_ISO;
const bundled = process.env.ATV_TEST_BUNDLED === '1';
if (!iso && !bundled) throw new Error('Set ATV_TEST_ISO to the private USA v2.00 ISO path.');
const base = process.env.ATV_TEST_URL || 'http://127.0.0.1:4173';
const browser = await chromium.launch({ headless: true });
const errors = [], requests = [];
const distance = (a, b) => Math.hypot(...a.map((value, index) => value - b[index]));
const finite = state => {
  assert(state.simulation, 'Rust simulation must remain running');
  assert(state.camera.every(Number.isFinite));
  assert(state.frame.origin.every(Number.isFinite));
  assert(state.frame.orientation.every(Number.isFinite));
  assert(Math.abs(Math.hypot(...state.frame.orientation) - 1) < .001);
  assert(Number.isFinite(state.frame.speed) && Number.isFinite(state.frame.rpm));
  assert(state.frame.quad_node_matrices.every(matrix => matrix.length === 16 && matrix.every(Number.isFinite)));
  assert(state.followCamera.position.every(Number.isFinite) && state.followCamera.target.every(Number.isFinite));
  assert(state.followCamera.fov >= 80 && state.followCamera.fov <= 95);
  assert.equal(state.glError, 0);
};
try {
  const page = await browser.newPage({ viewport: { width: 1280, height: 720 }, hasTouch: true });
  const read = () => page.evaluate(() => window.atvTerrainStatus);
  const steps = async count => {
    const start = (await read()).simulationSteps;
    await page.waitForFunction(target => window.atvTerrainStatus.simulationSteps >= target, start + count, { timeout: 90000 });
  };
  page.on('pageerror', error => errors.push(error.message));
  page.on('request', request => requests.push({ url: request.url(), method: request.method(), resourceType: request.resourceType(), postDataLength: request.postDataBuffer()?.byteLength ?? 0 }));
  const startUrl = new URL(base);
  if (!bundled) startUrl.searchParams.set('source', 'disc');
  else startUrl.searchParams.set('drive', '1');
  await page.goto(startUrl.href);
  if (!bundled) await page.locator('#disc').setInputFiles(iso);
  try {
    await page.waitForFunction(() => window.atvTerrainStatus?.ready && window.atvTerrainStatus.simulation && window.atvTerrainStatus.frames > 3, null, { timeout: 90000 });
  } catch (error) {
    throw new Error(`Disc load failed: ${await page.locator('#status').textContent()}; browser errors: ${JSON.stringify(errors)}`, { cause: error });
  }
  await page.locator('#game').click({ position: { x: 640, y: 300 } });
  await page.waitForFunction(() => window.atvTerrainStatus.audio?.context === 'running' && window.atvTerrainStatus.audio.commands > 0);
  await page.waitForTimeout(1000);
  const loaded = await read();
  assert(loaded.width > 100 && loaded.depth > 100 && loaded.triangles > 1000);
  assert(loaded.vehicleVertices > 1000 && loaded.vehicleTriangles > 1000 && loaded.vehicleTextures > 0);
  assert(loaded.riderTriangles > 1000 && loaded.riderTextures > 0);
  assert(loaded.terrainTextures > 0);
  assert(loaded.sceneInstances > 0 && loaded.sceneTriangles > 0);
  assert(loaded.skyTiles > 0);
  assert(loaded.chassis.points > 0 && loaded.chassis.rays > 0);
  assert(loaded.animatedQuadNodes > 4);
  assert.equal(loaded.animatedRiderNodes, 18);
  assert.equal(loaded.cameraPresets.length, 7);
  assert.equal(loaded.followCamera.fov, 85);
  assert(loaded.obstacles.instances > 0 && loaded.obstacles.meshes > 0 && loaded.obstacles.rays > 0);
  assert(loaded.audio.ready && loaded.audio.samples > 0 && !loaded.audioError);
  finite(loaded);
  if (bundled) {
    assert.equal(loaded.contentSource, 'hosted-original');
    assert(await page.locator('#disc').isHidden(), 'Hosted startup has no disc selection gate');
    assert(requests.some(request => new URL(request.url).pathname === '/content/manifest.json'));
    assert(!requests.some(request => /\.(iso|elf|irx|img)$/i.test(new URL(request.url).pathname)));
  }
  assert(loaded.frame.grounded.some(Boolean), 'Vehicle settles onto original terrain');
  await page.keyboard.down('KeyW');
  await steps(90);
  const driven = await read();
  await page.keyboard.up('KeyW');
  finite(driven);
  assert.equal(driven.input.throttle, 1);
  assert(distance(driven.frame.origin, loaded.frame.origin) > 3, `Throttle must move the actual Rust body: ${JSON.stringify({ before: loaded.frame.origin, after: driven.frame.origin, speed: driven.frame.speed, obstacleContacts: driven.frame.obstacle_contact_count })}`);
  assert(driven.frame.speed > loaded.frame.speed + 1);
  assert.notDeepEqual(driven.frame.quad_node_matrices, loaded.frame.quad_node_matrices, 'Recovered quad linkage must animate with the simulation');
  await page.keyboard.down('KeyS'); await steps(108); await page.keyboard.up('KeyS');
  const braked = await read(); finite(braked);
  assert(braked.frame.speed < driven.frame.speed, 'Original brakes must reduce speed');
  await page.locator('#menu').click(); await page.getByRole('button', {name:'Reset ATV', exact:true}).click(); await page.waitForTimeout(800);
  assert(distance((await read()).frame.origin, loaded.frame.origin) < 2, 'Reset returns the body to the original scene start');
  for (const preset of [1, 2, 3, 4, 0]) {
    await page.locator('#camera').click();
    await page.waitForFunction(preset => window.atvTerrainStatus.followCamera.preset === preset, preset);
    finite(await read());
  }
  await page.keyboard.press('KeyC');
  await page.waitForFunction(() => window.atvTerrainStatus.followCamera.preset === 1);
  await page.evaluate(() => {
    window.testPad = { connected: true, axes: [.6, 0, 0, 0], buttons: Array.from({ length: 8 }, (_, index) => ({ value: index === 7 ? 1 : 0, pressed: index === 7 })) };
    Object.defineProperty(navigator, 'getGamepads', { configurable: true, value: () => [window.testPad] });
    window.dispatchEvent(new Event('focus'));
  });
  await page.waitForTimeout(600);
  const gamepad = await read(); finite(gamepad);
  assert.notDeepEqual(gamepad.frame.rider_node_matrices, loaded.frame.rider_node_matrices, 'Original rider controllers must respond to driving input');
  assert.equal(gamepad.input.throttle, 1); assert(gamepad.input.steer < 0, 'Right input converts to original left-positive steering scalar');
  await page.evaluate(() => { window.testPad.buttons[3] = { value: 1, pressed: true }; });
  await page.waitForFunction(() => window.atvTerrainStatus.followCamera.preset === 2);
  await steps(15);
  assert.equal((await read()).followCamera.preset, 2, 'Held controller camera button cycles only once');
  await page.evaluate(() => window.dispatchEvent(new Event('blur')));
  await page.waitForTimeout(100);
  const blurred = await read();
  await page.waitForTimeout(200);
  assert.deepEqual((await read()).frame, blurred.frame, 'Focus loss pauses the Rust body even with a held gamepad trigger');
  assert.equal((await read()).audio.context, 'suspended');
  await page.evaluate(() => {
    window.testPad = null; window.dispatchEvent(new Event('focus'));
    window.lose = document.querySelector('#game').getContext('webgl2').getExtension('WEBGL_lose_context');
    if (!window.lose) throw new Error('Browser test requires WEBGL_lose_context.');
    window.lose.loseContext();
  });
  await page.waitForFunction(() => !window.atvTerrainStatus.ready);
  await page.waitForTimeout(100);
  await page.evaluate(() => window.lose.restoreContext());
  await page.waitForFunction(() => window.atvTerrainStatus.ready);
  await page.waitForTimeout(100);
  const restored = await read(); finite(restored);
  assert.equal(restored.vehicleTextures, loaded.vehicleTextures);
  assert.equal(restored.riderTextures, loaded.riderTextures);
  assert.equal(restored.terrainTextures, loaded.terrainTextures);
  assert.equal(restored.skyTiles, loaded.skyTiles);
  assert.equal(restored.sceneInstances, loaded.sceneInstances);
  await page.evaluate(() => Object.defineProperty(navigator, 'getGamepads', { configurable: true, value: () => { throw new DOMException('blocked', 'SecurityError'); } }));
  await page.setViewportSize({ width: 390, height: 844 });
  await page.locator('#menu').click(); await page.getByRole('button', {name:'Reset ATV', exact:true}).click(); await page.waitForTimeout(300);
  const cdp = await page.context().newCDPSession(page);
  const point = (id, x, y) => ({ id, x, y, radiusX: 1, radiusY: 1, force: 1 });
  const before = await read();
  await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [point(1, 80, 440)] });
  await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [point(1, 80, 440), point(2, 290, 440)] });
  await cdp.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: [point(1, 105, 380), point(2, 330, 440)] });
  await page.waitForTimeout(500);
  const during = await read(); finite(during);
  assert(during.input.throttle > .5 && during.input.steer < 0);
  assert.equal(during.input.preload_axis, 1, 'Right touch holds preload without orbiting the camera');
  await cdp.send('Input.dispatchTouchEvent', { type: 'touchCancel', touchPoints: [] });
  await page.waitForTimeout(100);
  const cancelled = await read();
  assert.equal(cancelled.input.throttle, 0); assert.equal(cancelled.input.brake, 0); assert.equal(Math.abs(cancelled.input.steer), 0);
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth), 390);
  finite(cancelled);
  if (process.env.ATV_TEST_SCREENSHOT) await page.screenshot({ path: process.env.ATV_TEST_SCREENSHOT });
  assert.deepEqual(errors, []);
  // Sites injects this Cloudflare challenge before a disc is selected. Keep its
  // known platform POST separate; game code has no upload path.
  const origin = new URL(base).origin;
  const platformRequests = requests.filter(request => request.method === 'POST' && request.resourceType === 'xhr' && new URL(request.url).origin === origin && new URL(request.url).pathname.startsWith('/cdn-cgi/challenge-platform/h/g/jsd/oneshot/'));
  const unexpectedRequests = requests.filter(request => !platformRequests.includes(request) && (request.method !== 'GET' || new URL(request.url).origin !== origin));
  assert.deepEqual(unexpectedRequests, [], `Unexpected request metadata (no body data): ${JSON.stringify(unexpectedRequests)}`);
  console.log(`PASS: original terrain/sky/props/ATV/rider, animated quad linkage, static obstacle loading, recovered camera/cycling, engine sample scheduling, Rust chassis/throttle/brakes/reset, gamepad/focus, touch/cancellation, GL recovery, no game uploads; ${platformRequests.length} known Cloudflare challenge requests.`);
} finally { await browser.close(); }
