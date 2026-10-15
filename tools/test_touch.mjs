// Opt-in original-content browser touch integration. No vehicle state injection.
import assert from 'node:assert/strict';
import { assertNoErrors, base, button, launch, newPage, waitForScreen, waitForSteps } from './browser/harness.mjs';
const browser = await launch();
try {
  const errors = [];
  const page = await newPage(
    browser,
    { viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true },
    errors
  );
  // Explicit optional two-finger mode and original steering response for this baseline route.
  await page.addInitScript(() => {
    localStorage.setItem('atv-mobile-one-handed', 'false');
    localStorage.setItem('atv-mobile-steering-sensitivity', '100');
  });
  await page.addInitScript(() => {
    window.touchSamples = [];
    window.touchResolved = [];
    const watched = new WeakSet(),
      requests = new Map();
    const post = Worker.prototype.postMessage;
    Worker.prototype.postMessage = function (data, ...rest) {
      if (!watched.has(this)) {
        watched.add(this);
        this.addEventListener('message', ({ data }) => {
          if (!requests.has(data.id)) return;
          let speed = requests.get(data.id);
          requests.delete(data.id);
          if (data.error) return;
          for (const { frame } of JSON.parse(new TextDecoder().decode(data.bytes))) {
            window.touchResolved.push({ input: frame.applied_input, speed });
            speed = frame.speed;
          }
        });
      }
      if (data.op === 50) {
        requests.set(data.id, window.atvTerrainStatus?.frame?.speed);
        window.touchSamples.push({
          input: JSON.parse(new TextDecoder().decode(data.bytes)).input,
          speed: window.atvTerrainStatus?.frame?.speed
        });
      }
      return post.call(this, data, ...rest);
    };
  });
  await page.goto(new URL('/play.html', base).href);
  await waitForScreen(page, 'title');
  await page.keyboard.press('Enter');
  await button(page, 'Single Player').tap();
  await button(page, 'Freestyle').tap();
  await button(page, 'Ride Caribou Peak').tap();
  await page.waitForFunction(
    () => window.atvTerrainStatus.simulation && document.querySelector('#frontend').hidden,
    null,
    { timeout: 90000 }
  );
  const read = () => page.evaluate(() => window.atvTerrainStatus);
  const steps = count => waitForSteps(page, count, { timeout: 90000 });
  const samples = () => page.evaluate(() => window.touchSamples);
  const resetSamples = () =>
    page.evaluate(() => {
      window.touchSamples = [];
      window.touchResolved = [];
    });
  const cdp = await page.context().newCDPSession(page);
  const touch = (type, points = []) =>
    cdp.send('Input.dispatchTouchEvent', {
      type,
      touchPoints: points.map(p => ({ radiusX: 6, radiusY: 6, force: 1, ...p }))
    });
  assert.equal(await page.locator('#touch-controls button').count(), 0, 'No separate action buttons');
  assert.equal(await page.evaluate(() => getComputedStyle(document.body).webkitUserSelect), 'none');
  for (const position of [
    { x: 250, y: 240 },
    { x: 340, y: 640 },
    { x: 387, y: 420 }
  ]) {
    // Each stationary-press case needs a fresh grounded start. Accumulated
    // driving can otherwise reach the ramp and correctly switch to air tricks
    // during a later case's drive-only assertions. No vehicle state is injected.
    await page.goto(new URL('/play.html?drive=1&environment=env02', base).href);
    await page.waitForFunction(() => window.atvTerrainStatus?.simulation, null, { timeout: 90000 });
    await page.waitForFunction(() => window.atvTerrainStatus.frame.grounded.some(Boolean), null, { timeout: 10000 });
    const right = { id: 1, ...position };
    await resetSamples();
    await touch('touchStart', [right]);
    await steps(3);
    assert.equal((await read()).input.preload_axis, 1);
    assert.equal((await read()).input.lean, 0);
    assert.equal(
      Math.max((await read()).input.throttle, (await read()).input.brake),
      1,
      'Right press must request drive without a forward drag'
    );
    assert(
      (await samples()).some(s => s.input.touch_drive?.held && s.input.touch_drive?.axis === 1),
      'Stationary press must submit forward drive to Rust'
    );
    await touch('touchMove', [{ ...right, y: right.y - 70 }]);
    await steps(3);
    assert.equal((await read()).input.lean, 0);
    assert.equal(Math.max((await read()).input.throttle, (await read()).input.brake), 1);
    assert.equal((await read()).input.preload_axis, 1);
    await touch('touchMove', [{ ...right, y: right.y + 70 }]);
    await steps(3);
    assert.equal((await read()).input.lean, 0);
    assert.equal((await read()).input.preload_axis, 1);
    assert.equal((await read()).frame.preload_active, false);
    await touch('touchEnd');
    await steps(3);
    assert.equal((await samples()).filter(s => s.input.preload_axis === -1).length, 1);
    assert.equal((await read()).input.lean, 0);
  }
  await page.goto(new URL('/play.html?drive=1', base).href);
  await page.waitForFunction(
    () => window.atvTerrainStatus?.simulation && window.atvTerrainStatus.frame.grounded.some(Boolean),
    null,
    { timeout: 90000 }
  );
  console.log(
    'Anywhere on the right drives immediately and holds preload; back drag brakes without adding lean; release submits once.'
  );
  const right = { id: 1, x: 310, y: 530 };
  await resetSamples();
  await touch('touchStart', [right]);
  await steps(2);
  await touch('touchCancel');
  await steps(3);
  assert(!(await samples()).some(s => s.input.preload_axis === -1));
  const left = { id: 2, x: 60, y: 620 },
    lean = { ...left, x: 95, y: 690 };
  const drive = { ...right, y: 460 },
    reverse = { ...right, y: 600 };
  await touch('touchStart', [left]);
  await touch('touchMove', [lean]);
  await steps(3);
  assert.equal((await read()).input.throttle, 0);
  assert.equal((await read()).input.brake, 0);
  assert.equal((await read()).input.lean, 1);
  assert.equal((await read()).input.steer, -0.5);
  await touch('touchMove', [left]);
  await touch('touchStart', [left, right]);
  await touch('touchMove', [left, drive]);
  await page.waitForFunction(() => window.atvTerrainStatus.frame.speed > 5, null, { timeout: 30000 });
  let state = await read();
  assert.equal(state.input.throttle, 1);
  assert.equal(state.input.preload_axis, 1);
  if (process.env.ATV_TOUCH_SCREENSHOT) await page.screenshot({ path: process.env.ATV_TOUCH_SCREENSHOT });
  await resetSamples();
  await touch('touchMove', [left, reverse]);
  await page.waitForFunction(() => window.atvTerrainStatus.input.brake > 0.9, null, { timeout: 10000 });
  assert.equal((await read()).input.reverse, false);
  await page.waitForFunction(
    () => window.atvTerrainStatus.input.reverse && window.atvTerrainStatus.frame.speed < -2,
    null,
    { timeout: 45000 }
  );
  {
    const log = await page.evaluate(() => window.touchResolved),
      start = log.findIndex(s => s.input.brake > 0.9);
    assert(start >= 0);
    assert(
      log
        .slice(start)
        .filter(s => s.speed > 1)
        .every(s => !s.input.reverse && s.input.throttle === 0),
      JSON.stringify(log)
    );
  }
  await resetSamples();
  await touch('touchMove', [left, drive]);
  await page.waitForFunction(() => window.atvTerrainStatus.input.brake > 0.9, null, { timeout: 10000 });
  assert.equal((await read()).input.reverse, true);
  await page.waitForFunction(
    () => !window.atvTerrainStatus.input.reverse && window.atvTerrainStatus.frame.speed > 2,
    null,
    { timeout: 45000 }
  );
  {
    const log = await page.evaluate(() => window.touchResolved),
      start = log.findIndex(s => s.input.brake > 0.9);
    assert(start >= 0);
    assert(
      log
        .slice(start)
        .filter(s => s.speed < -1)
        .every(s => s.input.reverse && s.input.throttle === 0),
      JSON.stringify(log)
    );
  }
  await resetSamples();
  await touch('touchEnd', [drive]);
  await steps(3);
  assert.equal((await read()).input.throttle, 0);
  assert.equal((await samples()).filter(s => s.input.preload_axis === -1).length, 1);
  await touch('touchCancel');
  await steps(3);
  console.log('Left always steers/leans; right brakes before gear changes and releases throttle with preload.');
  await touch('touchStart', [right]);
  await steps(2);
  await resetSamples();
  await page.evaluate(() => window.dispatchEvent(new Event('blur')));
  await touch('touchCancel');
  // Time-based on purpose: lets an in-flight batch finish, then proves the blurred frame stays unchanged.
  await page.waitForTimeout(100);
  const paused = (await read()).frame;
  await page.waitForTimeout(100);
  assert.deepEqual((await read()).frame, paused);
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await steps(3);
  assert(!(await samples()).some(s => s.input.preload_axis === -1));
  await touch('touchStart', [right]);
  await steps(2);
  await resetSamples();
  await page.keyboard.press('Escape');
  await waitForScreen(page, 'pause');
  assert(await page.locator('#touch-controls').isHidden());
  await touch('touchCancel');
  // Time-based on purpose: lets an in-flight batch finish, then proves the paused frame stays unchanged.
  await page.waitForTimeout(100);
  const menuFrame = (await read()).frame;
  await page.waitForTimeout(100);
  assert.deepEqual((await read()).frame, menuFrame);
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth), 390);
  const log = await browser.newPage();
  await log.goto(new URL('/devlog.html', page.url()).href);
  assert.notEqual(
    await log.evaluate(() => getComputedStyle(document.body).webkitUserSelect),
    'none',
    'Dev log remains selectable'
  );
  assertNoErrors(errors);
  console.log(
    'PASS: right drive/preload, left steer/lean, safe automatic direction changes, cancellation, multi-touch, focus/menu pause, game text selection suppressed; dev log unaffected.'
  );
} finally {
  await browser.close();
}
