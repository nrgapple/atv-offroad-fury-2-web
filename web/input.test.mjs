import { test } from 'node:test';
import assert from 'node:assert/strict';
import { defaultMapping, fromGamepad, combine, deadzone, stick, gesture, validMapping } from './input.js';
test('gamepad analog triggers, deadzone, axes, and custom bindings', () => {
  const buttons = Array.from({ length: 16 }, () => ({ value: 0 })); buttons[7].value = .42; buttons[6].value = .8; buttons[0].value = 1;
  const pad = { axes: [.05, -1, .6, 0], buttons };
  const input = fromGamepad(pad); assert.equal(input.steer, 0); assert.equal(input.lean, -1); assert.equal(input.throttle, .42); assert.equal(input.brake, .8); assert.equal(input.jump, 1);
  const remap = { ...defaultMapping, throttle: { button: 0 } }; assert.equal(fromGamepad(pad, remap).throttle, 1); assert(validMapping(remap)); assert(!validMapping({}));
});
test('neutral on disconnect; mixed touch/gamepad sources do not cancel each other', () => {
  assert(Object.values(fromGamepad(null)).every(v => v === 0));
  const input = combine({ steer: -.9 }, { steer: .3, throttle: .8 }, { preload: 1 });
  assert.equal(input.steer, -.9); assert.equal(input.throttle, .8); assert.equal(input.preload, 1);
});
test('floating stick stays in circle and gestures handle diagonals and jitter', () => {
  assert.equal(deadzone(.15), 0); assert.equal(deadzone(1), 1);
  const p = stick(100, -100); assert(Math.abs(Math.hypot(p.x, p.y) - 1) < .0001);
  assert.equal(gesture(10, 10), null); assert.equal(gesture(80, -80), 'up-right'); assert.equal(gesture(-80, 0), 'left'); assert.equal(gesture(0, 80), 'down');
});
