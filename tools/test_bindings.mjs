// Opt-in actual-content gameplay binding integration. Menu capture is covered
// separately by test_frontend; this follows the same singleton into real WASM.
import assert from 'node:assert/strict';
import { assertNoErrors, base, launch, newPage, nextFrames, waitForGame, waitForSteps } from './browser/harness.mjs';
const browser = await launch();
try {
  const errors = [];
  const page = await newPage(
    browser,
    { viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true },
    errors
  );
  await page.addInitScript(() => {
    window.bindingPad = null;
    window.initialPackets = [];
    window.bindingBatches = [];
    window.bindingResets = 0;
    Object.defineProperty(navigator, 'getGamepads', { configurable: true, value: () => [window.bindingPad] });
    const load = new Set([20, 21, 22, 23, 26, 27, 28, 29, 44, 45, 46, 47, 48, 49, 51, 52, 53, 63]);
    const post = Worker.prototype.postMessage;
    Worker.prototype.postMessage = function (data, ...rest) {
      if (load.has(data.op) && !window.runtimePackets)
        window.initialPackets.push({ op: data.op, index: data.index, bytes: data.bytes.slice() });
      if (data.op === 23) window.bindingResets++;
      if (data.op === 50) {
        window.bindingBatches.push(JSON.parse(new TextDecoder().decode(data.bytes)));
        if (window.bindingBatches.length > 2000) window.bindingBatches.shift();
      }
      return post.call(this, data, ...rest);
    };
  });
  await page.goto(new URL('/play.html?drive=1', base).href);
  await waitForGame(page, { until: () => window.atvTerrainStatus.simulation });
  await page.evaluate(async () => {
    window.runtimePackets = window.initialPackets.slice();
    window.bindings = (await import('./bindings.js')).gameplayBindings;
  });
  const focus = async () => {
    await page.bringToFront();
    await page.locator('#game').focus();
  };
  const steps = (count = 4) => waitForSteps(page, count);
  const input = expected =>
    page.waitForFunction(
      expected =>
        Object.entries(expected).every(
          ([key, value]) => Math.abs(Number(window.atvTerrainStatus.input[key]) - Number(value)) < 1e-6
        ),
      expected
    );
  const set = (device, id, binding) =>
    page.evaluate(({ device, id, binding }) => window.bindings.setBinding(device, id, binding), {
      device,
      id,
      binding
    });
  const pad = async (buttons = [], axes = [0, 0, 0, 0]) => {
    await page.evaluate(
      ({ buttons, axes }) => {
        window.bindingPad = {
          id: 'Regression standard pad',
          index: 0,
          connected: true,
          axes,
          buttons: Array.from({ length: 17 }, (_, i) => ({
            value: buttons.includes(i) ? 1 : 0,
            pressed: buttons.includes(i)
          }))
        };
      },
      { buttons, axes }
    );
    await steps();
  };
  await focus();
  await steps();
  const quickTap = code =>
    page.evaluate(code => {
      // Both events in one JS task are necessarily between simulation samples.
      window.dispatchEvent(new KeyboardEvent('keydown', { code, bubbles: true, cancelable: true }));
      window.dispatchEvent(new KeyboardEvent('keyup', { code, bubbles: true, cancelable: true }));
    }, code);
  let cycles = await page.evaluate(() => window.bindingBatches.filter(b => b.cycle).length);
  await quickTap('KeyC');
  await steps();
  assert.equal(
    await page.evaluate(() => window.bindingBatches.filter(b => b.cycle).length),
    cycles + 1,
    'Brief camera key tap must survive'
  );
  let resets = await page.evaluate(() => window.bindingResets);
  await quickTap('Backspace');
  await steps();
  assert.equal(await page.evaluate(() => window.bindingResets), resets + 1, 'Brief reset key tap must survive');
  await quickTap('Escape');
  await page.waitForFunction(
    () => !document.querySelector('#frontend').hidden && document.querySelector('#frontend').dataset.page === 'pause'
  );
  assert(
    await page.evaluate(() => {
      document.querySelector('[data-action="resume"]').click();
      const resumed = document.querySelector('#frontend').hidden;
      // Same JS task: no neutral physics sample can occur between these events.
      window.dispatchEvent(new KeyboardEvent('keydown', { code: 'Escape', bubbles: true, cancelable: true }));
      window.dispatchEvent(new KeyboardEvent('keyup', { code: 'Escape', bubbles: true, cancelable: true }));
      return resumed;
    }),
    'Resume must hide the pause menu'
  );
  await page.waitForFunction(
    () => !document.querySelector('#frontend').hidden && document.querySelector('#frontend').dataset.page === 'pause'
  );
  await page.getByRole('button', { name: 'Resume', exact: true }).click();
  await page.waitForFunction(() => document.querySelector('#frontend').hidden);
  await steps();
  await page.keyboard.down('KeyE');
  await input({ lean: 1, brake: 0, throttle: 0 });
  await page.keyboard.up('KeyE');
  await page.keyboard.down('KeyQ');
  await input({ lean: -1, brake: 0 });
  await page.keyboard.up('KeyQ');
  await page.keyboard.down('KeyW');
  await input({ throttle: 1 });
  await page.keyboard.up('KeyW');
  // Arrows mirror WASD; S/Down from a stop selects reverse through Rust's keyboard adapter.
  await page.keyboard.down('ArrowUp');
  await input({ throttle: 1, lean: 0, reverse: false });
  await page.keyboard.up('ArrowUp');
  await steps(30);
  await page.keyboard.down('KeyS');
  await input({ throttle: 1, brake: 0, reverse: true });
  await page.keyboard.up('KeyS');
  await page.keyboard.down('KeyW');
  await input({ reverse: false, throttle: 1 });
  await page.keyboard.up('KeyW');
  await set('keyboard', 'throttle', { key: 'KeyT' });
  await page.keyboard.down('KeyW');
  await steps();
  await input({ throttle: 0 });
  await page.keyboard.up('KeyW');
  await page.keyboard.down('KeyT');
  await input({ throttle: 1 });
  await page.keyboard.up('KeyT');
  await set('keyboard', 'leanForward', { key: 'KeyI' });
  await set('keyboard', 'stuntTriangle', { key: 'KeyU' });
  await page.keyboard.down('KeyI');
  await page.keyboard.down('KeyU');
  await input({ lean: -1, stunt_y: -1, stunt_modifiers: 2 });
  await page.keyboard.up('KeyI');
  await page.keyboard.up('KeyU');
  await pad();
  await pad([0]);
  await input({ throttle: 1, reverse: false });
  await pad([2]);
  await input({ throttle: 0, brake: 1 });
  await pad([0, 2]);
  await input({ throttle: 1, brake: 0, reverse: true, reverse_chord_brake: 1 });
  await pad([0, 2, 6]);
  await input({ throttle: 1, brake: 1, reverse: true });
  await pad();
  await set('controller', 'throttle', { axis: 3, sign: -1 });
  await pad([], [0, 0, 0, -0.575]);
  await input({ throttle: 0.5 });
  await pad([], [0, 0, 0, 0.575]);
  await input({ throttle: 0 });
  await pad([1, 3, 4], [0.575, -0.575, 0, 0]);
  // Source +X is screen-right: the steering sign is no longer negated (3574454).
  await input({ stunt_modifiers: 7, stunt_x: 0.5, stunt_y: -0.5, steer: 0.5, lean: -0.5 });
  await pad();
  // Camera, reset and pause actions must be rising edges, not per-frame work.
  const beforeCamera = await page.evaluate(() => window.bindingBatches.filter(b => b.cycle).length);
  await pad([11]);
  await steps(8);
  assert.equal(await page.evaluate(() => window.bindingBatches.filter(b => b.cycle).length), beforeCamera + 1);
  await pad();
  const beforeReset = await page.evaluate(() => window.bindingResets);
  await pad([4, 5]);
  await steps(8);
  assert.equal(await page.evaluate(() => window.bindingResets), beforeReset + 1);
  await pad();
  await page.evaluate(() => {
    window.bindingPad.buttons[9] = { pressed: true, value: 1 };
  });
  await page.waitForFunction(
    () => !document.querySelector('#frontend').hidden && document.querySelector('#frontend').dataset.page === 'pause'
  );
  await nextFrames(page, 11); // The front end polls the held pad once per animation frame.
  assert.equal(
    await page.locator('#frontend').isVisible(),
    true,
    'The controller press that opens pause must not resume it'
  );
  const paused = await page.evaluate(() => window.atvTerrainStatus.simulationSteps);
  // Time-based on purpose: proves the paused simulation does not advance.
  await page.waitForTimeout(120);
  assert.equal(await page.evaluate(() => window.atvTerrainStatus.simulationSteps), paused);
  await page.evaluate(() => {
    window.bindingPad.buttons[9] = { pressed: false, value: 0 };
  });
  await page.getByRole('button', { name: 'Resume', exact: true }).click();
  await page.waitForFunction(() => document.querySelector('#frontend').hidden);
  // Losing a held remapped preload is cancellation, including a controller
  // replacement detected by polling when no disconnect event was delivered.
  await set('controller', 'preloadHold', { button: 5 });
  await pad();
  await pad([5]);
  await input({ preload_axis: 1 });
  const beforeLoss = await page.evaluate(() => window.bindingBatches.length);
  await page.evaluate(() => {
    window.bindingPad = null;
    window.dispatchEvent(new Event('gamepaddisconnected'));
  });
  await steps();
  assert(
    await page.evaluate(start => window.bindingBatches.slice(start).every(b => b.input.preload_axis !== -1), beforeLoss)
  );
  await page.keyboard.down('Space');
  await input({ preload_axis: 1 });
  await page.evaluate(() => window.dispatchEvent(new Event('blur')));
  await page.keyboard.up('Space');
  const beforeFocus = await page.evaluate(() => window.bindingBatches.length);
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await focus();
  await steps();
  const beforeRepeat = await page.evaluate(() => window.bindingBatches.filter(b => b.cycle).length);
  await page.evaluate(() =>
    window.dispatchEvent(new KeyboardEvent('keydown', { code: 'KeyC', repeat: true, bubbles: true, cancelable: true }))
  );
  await steps();
  assert.equal(
    await page.evaluate(() => window.bindingBatches.filter(b => b.cycle).length),
    beforeRepeat,
    'OS repeat after cancellation is not a fresh camera press'
  );
  assert(
    await page.evaluate(
      start => window.bindingBatches.slice(start).every(b => b.input.preload_axis !== -1),
      beforeFocus
    )
  );
  // Fresh actual-content simulations prove the native gesture state is also
  // cancelled. JS-only absence of a release packet would miss the stale flick.
  await page.evaluate(() => window.dispatchEvent(new Event('blur')));
  const result = await page.evaluate(async () => {
    const instances = await Promise.all([0, 1].map(() => WebAssembly.instantiateStreaming(fetch('atv_web.wasm'), {})));
    const encode = value => new TextEncoder().encode(JSON.stringify(value));
    const invoke = (instance, op, bytes, index = 0) => {
      const w = instance.instance.exports;
      if (!w.input_resize(bytes.length)) throw Error('input allocation');
      new Uint8Array(w.memory.buffer, w.input_ptr(), bytes.length).set(bytes);
      if (!w.run(op, index))
        throw Error(new TextDecoder().decode(new Uint8Array(w.memory.buffer, w.output_ptr(), w.output_len())));
      return JSON.parse(new TextDecoder().decode(new Uint8Array(w.memory.buffer, w.output_ptr(), w.output_len())));
    };
    for (const packet of window.runtimePackets)
      for (const instance of instances) invoke(instance, packet.op, packet.bytes, packet.index);
    let armed,
      count = 0;
    for (; count < 800; count++) {
      const input = { throttle: 1, brake: 0, steer: 0, lean: 0, reverse: false, preload_axis: 1, input_generation: 10 };
      const frames = instances.map(instance => invoke(instance, 24, encode({ dt: 1 / 60, input })));
      if (frames[0].preload_armed) {
        armed = frames[0];
        break;
      }
    }
    if (!armed) throw Error('Original Caribou route did not reach preload arming');
    const input = { throttle: 0, brake: 0, steer: 0, lean: -1, reverse: false, input_generation: 10 };
    const regular = invoke(instances[0], 24, encode({ dt: 1 / 60, input }));
    input.input_generation = 11;
    const cancelled = invoke(instances[1], 24, encode({ dt: 1 / 60, input }));
    return {
      count,
      armed: armed.preload_armed,
      regular: {
        active: regular.preload_active,
        strength: regular.preload_strength,
        vertical: regular.linear_velocity[1]
      },
      cancelled: {
        active: cancelled.preload_active,
        armed: cancelled.preload_armed,
        strength: cancelled.preload_strength,
        vertical: cancelled.linear_velocity[1]
      }
    };
  });
  assert(
    result.regular.active && result.regular.strength > 0,
    'Uncancelled forward lean must trigger the original armed preload'
  );
  assert(
    !result.cancelled.active && !result.cancelled.armed && result.cancelled.strength === 0,
    'Cancellation must clear the native flick, not synthesize release'
  );
  assert(result.regular.vertical > result.cancelled.vertical, 'Cancelled replay must have no jump boost');
  await page.evaluate(() => {
    window.bindings.reset('keyboard');
    window.bindings.reset('controller');
  });
  assertNoErrors(errors);
  console.log(
    `PASS: real WASM remapped keyboard/analog controller inputs, native reverse chord, keyboard brake-then-reverse, stunt axes, once-only camera/reset/pause, disconnect/blur cancellation, and original-map native stale-preload prevention after ${result.count + 1} driving steps.`
  );
} finally {
  await browser.close();
}
