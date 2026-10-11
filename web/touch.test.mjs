import test from 'node:test';
import assert from 'node:assert/strict';
import { TouchInput, TouchDrive } from './touch.js';

test('whole-right gesture separates physical lean from hold/release preload', () => {
  const touch = new TouchInput(); touch.begin(1, 300, 400);
  assert.deepEqual(touch.sample(), { lean: 0, preload_axis: 1 });
  touch.move(1, 200, 330); assert.deepEqual(touch.sample(), { lean: -1, preload_axis: 1 });
  touch.move(1, 350, 470); assert.deepEqual(touch.sample(), { lean: 1, preload_axis: 1 });
  touch.end(1); assert.deepEqual(touch.sample(), { lean: 0, preload_axis: -1 });
  assert.deepEqual(touch.sample(), { lean: 0, preload_axis: null });
});

test('cancel, blur/reset and an unseen tap never emit preload release', () => {
  for (const reason of ['cancel', 'clear', 'tap']) {
    const touch = new TouchInput(); touch.begin(1);
    if (reason !== 'tap') touch.sample();
    if (reason === 'clear') touch.clear(); else touch.end(1, reason === 'cancel');
    assert.equal(touch.sample().preload_axis, null);
  }
});

test('second right finger cannot steal gesture or cause premature release', () => {
  const touch = new TouchInput(); assert(touch.begin(1)); assert(!touch.begin(2));
  touch.move(2, 0, 70); touch.end(2); assert.deepEqual(touch.sample(), { lean: 0, preload_axis: 1 });
  touch.end(1); touch.end(1, true); assert.equal(touch.sample().preload_axis, -1);
  touch.begin(3); assert.equal(touch.sample().preload_axis, 1);
});

test('backward left input brakes forward motion and reverses only near standstill', () => {
  const drive = new TouchDrive();
  assert.deepEqual(drive.sample(-1, 30), { throttle: 0, brake: 1, reverse: false });
  assert.deepEqual(drive.sample(-.5, 1.01), { throttle: 0, brake: .5, reverse: false });
  assert.deepEqual(drive.sample(-.5, .99), { throttle: .5, brake: 0, reverse: true });
  assert.deepEqual(drive.sample(-1, -10), { throttle: 1, brake: 0, reverse: true });
  assert.deepEqual(drive.sample(0, -10), { throttle: 0, brake: 0, reverse: true });
});

test('forward input brakes backward motion before selecting forward; reset is neutral', () => {
  const drive = new TouchDrive();
  assert.deepEqual(drive.sample(1, -10), { throttle: 0, brake: 1, reverse: true });
  assert.deepEqual(drive.sample(.5, -1.01), { throttle: 0, brake: .5, reverse: true });
  assert.deepEqual(drive.sample(.5, -.99), { throttle: .5, brake: 0, reverse: false });
  drive.clear(); assert.equal(drive.engaged, false); assert.deepEqual(drive.sample(0, 0), { throttle: 0, brake: 0, reverse: false });
});
