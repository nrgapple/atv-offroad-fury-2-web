import test from 'node:test';
import assert from 'node:assert/strict';
import { createPadTracker } from './gamepads.js';

const pad = (index, id, { mapping = 'standard', axes = [0, 0, 0, 0], pressed = [] } = {}) => ({
  index, id, mapping, connected: true, axes,
  buttons: Array.from({ length: 17 }, (_, i) => ({ pressed: pressed.includes(i), value: Number(pressed.includes(i)) })),
});

test('follows the pad in use, not an idle pad listed first', () => {
  let pads = [pad(0, 'Steam Virtual Gamepad'), pad(1, 'DualSense Wireless Controller')];
  const active = createPadTracker(() => pads);
  assert.equal(active().index, 0, 'nothing pressed yet: first standard pad');
  pads = [pads[0], pad(1, 'DualSense Wireless Controller', { pressed: [0] })];
  assert.equal(active().index, 1, 'a button press selects the real controller');
  pads = [pads[0], pad(1, 'DualSense Wireless Controller')];
  assert.equal(active().index, 1, 'released: stays on the last used controller');
  pads = [pads[0], pad(1, 'DualSense Wireless Controller', { axes: [-.8, 0, 0, 0] })];
  assert.equal(active().index, 1);
  pads = [pad(0, 'Steam Virtual Gamepad', { axes: [.9, 0, 0, 0] }), pads[1]];
  assert.equal(active().index, 1, 'the current pad keeps priority while both are active');
});

test('idle devices with resting axes never take over', () => {
  // A wheel/HID interface listed first, with a pedal or hat resting at an extreme.
  let pads = [pad(0, 'Generic HID', { mapping: '', axes: [-1, 1.2857, 0, 0] }), pad(1, 'Xbox Controller')];
  const active = createPadTracker(() => pads);
  assert.equal(active().index, 1, 'prefers a standard-mapping pad when idle');
  pads = [pads[0], pad(1, 'Xbox Controller', { pressed: [7] })];
  assert.equal(active().index, 1);
  pads = [pads[0], pad(1, 'Xbox Controller')];
  assert.equal(active().index, 1, 'resting -1 / hat axes are not activity');
  pads = [pad(0, 'Generic HID', { mapping: '', axes: [0, 1.2857, 0, 0] }), pads[1]];
  assert.equal(active().index, 0, 'real movement on that device does count');
});

test('disconnects, holes and unavailable gamepads', () => {
  let pads = [null, pad(1, 'Xbox Controller', { pressed: [0] }), pad(2, 'Pro Controller')];
  const active = createPadTracker(() => pads);
  assert.equal(active().index, 1);
  pads = [null, null, pad(2, 'Pro Controller')];
  assert.equal(active().index, 2, 'falls back when the current pad disconnects');
  pads = [null, null, { ...pad(2, 'Pro Controller'), connected: false }];
  assert.equal(active(), null);
  assert.equal(createPadTracker(() => { throw new Error('denied'); })(), null);
});

import { ControllerSettings, CONTROLLER_STORAGE_KEY, padName, controllerLabel, cycleController } from './gamepads.js';
import { GameplayBindings } from './bindings.js';
const memory = () => { const map = new Map(); return { getItem: k => map.get(k) ?? null, setItem: (k, v) => map.set(k, v), map }; };

test('readable pad names', () => {
  assert.equal(padName({ id: 'DualSense Wireless Controller (STANDARD GAMEPAD Vendor: 054c Product: 0ce6)', index: 0 }), 'DualSense Wireless Controller');
  assert.equal(padName({ id: 'Xbox 360 Controller (XInput STANDARD GAMEPAD)', index: 1 }), 'Xbox 360 Controller');
  assert.equal(padName({ id: '054c-0ce6-DualSense Wireless Controller', index: 0 }), 'DualSense Wireless Controller');
  assert.equal(padName({ id: '', index: 2 }), 'Controller 3');
});

test('a pinned pad wins while connected; Auto otherwise', () => {
  const settings = new ControllerSettings(memory());
  const xbox = pad(0, 'Xbox Controller'), sense = pad(1, 'DualSense');
  let pads = [xbox, sense];
  const active = createPadTracker(() => pads, settings);
  assert.equal(controllerLabel(settings, active), 'Auto (Xbox Controller)');
  cycleController(1, settings, active);
  assert.equal(settings.selected.id, 'Xbox Controller');
  cycleController(1, settings, active);
  assert.equal(settings.selected.id, 'DualSense');
  pads = [pad(0, 'Xbox Controller', { pressed: [0] }), sense];
  assert.equal(active().id, 'DualSense', 'input on another pad does not steal a pinned pad');
  assert.equal(controllerLabel(settings, active), 'DualSense');
  pads = [pads[0]];
  assert.equal(active().id, 'Xbox Controller', 'absent pinned pad falls back to Auto');
  assert.equal(controllerLabel(settings, active), 'DualSense (not connected, using Auto)');
  cycleController(-1, settings, active);
  assert.equal(settings.selected?.id, 'Xbox Controller', 'from an absent pin, stepping starts at Auto');
  cycleController(1, settings, active);
  assert.equal(settings.selected, null, 'wraps back to Auto');
});

test('settings persist, clamp to steps and reset responses only', () => {
  const storage = memory(), settings = new ControllerSettings(storage);
  settings.set('steering', 137); settings.set('deadzone', 90); settings.step('camera', -1); settings.select(pad(1, 'DualSense'));
  const saved = new ControllerSettings(storage);
  assert.deepEqual([saved.steering, saved.deadzone, saved.camera, saved.selected], [135, 40, 95, { id: 'DualSense', index: 1 }]);
  saved.reset();
  assert.deepEqual([saved.steering, saved.lean, saved.triggers, saved.camera, saved.deadzone], [100, 100, 100, 100, 15]);
  assert.equal(saved.selected.id, 'DualSense', 'Defaults keep the chosen controller');
  storage.setItem(CONTROLLER_STORAGE_KEY, '{"steering":"fast","selected":{"id":5}}');
  const bad = new ControllerSettings(storage);
  assert.deepEqual([bad.steering, bad.selected], [100, null]);
});

test('sensitivity curves and dead zone shape controller input only', () => {
  const settings = new ControllerSettings(memory()), bindings = new GameplayBindings(null);
  bindings.tuning = settings;
  const stick = (x, trigger = 0) => ({ connected: true, index: 0, id: 'p', axes: [x, 0, 0, 0], buttons: Array.from({ length: 17 }, (_, i) => ({ pressed: i === 7 && trigger > 0, value: i === 7 ? trigger : 0 })) });
  const read = pad => bindings.read(new Set(), pad).input;
  const close = (a, b, message) => assert.ok(Math.abs(a - b) < 1e-9, `${message}: ${a} vs ${b}`);
  const base = read(stick(.575, .5));
  close(base.steer, .5, 'defaults keep the existing 15% dead zone');
  close(base.throttle, .5, 'defaults keep the trigger response');
  settings.set('steering', 50); settings.set('triggers', 150);
  const tuned = read(stick(.575, .5));
  close(tuned.steer, .25, '50%: finer near centre');
  close(tuned.throttle, .5 ** (100 / 150), '150%: stronger near rest');
  assert.equal(read(stick(1, 1)).steer, 1, 'full lock still reached');
  assert.equal(read(stick(1, 1)).throttle, 1);
  settings.set('deadzone', 0);
  assert.ok(read(stick(.1)).steer > 0, 'a 0% dead zone passes small movement');
  settings.set('deadzone', 40);
  assert.equal(read(stick(.35)).steer, 0);
  assert.equal(bindings.value('keyboard', 'steerRight', new Set(['KeyD']), null), 1, 'keyboard unaffected');
});
