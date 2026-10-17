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
