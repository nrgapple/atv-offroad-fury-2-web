import test from 'node:test';
import assert from 'node:assert/strict';
import { TouchInput, TouchDriveIntent } from './touch.js';

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

test('left touch sends raw intent and preserves center while a second finger cannot steal it', () => {
  const left = new TouchDriveIntent(); const initial = left.sample();
  assert.deepEqual(initial, { generation: 1, held: false, axis: 0 });
  assert(left.begin(1, 60, 500)); assert(!left.begin(2, 70, 510));
  left.move(1, 130, 570); assert.equal(left.steer, 1);
  assert.deepEqual(left.sample(), { generation: 1, held: true, axis: -1 });
  left.move(2, 0, 0); left.end(2, true);
  assert.equal(left.sample().axis, -1); assert.equal(left.sample().generation, 1);
  left.move(1, 60, 430); assert.equal(left.sample().axis, 1);
  left.end(1); assert.deepEqual(left.sample(), initial);
  assert(left.begin(3, 60, 500));
});

test('left cancel and clear advance generation to invalidate queued takeoff memory', () => {
  const left = new TouchDriveIntent(); left.begin(1, 50, 500); left.move(1, 50, 430);
  const queued = left.sample(); left.end(1, true);
  assert.equal(queued.axis, 1); assert.equal(left.sample().axis, 0);
  assert.equal(left.sample().generation, queued.generation + 1);
  left.begin(2, 50, 500); left.clear();
  assert.equal(left.sample().generation, queued.generation + 2);
  assert.equal(left.sample().held, false); assert.equal(left.steer, 0);
});

test('airborne right swipes hold original Circle directions and never emit preload release', () => {
  for (const [dx,dy,x,y] of [[0,-70,0,-1],[0,70,0,1],[-70,0,-1,0],[70,0,1,0]]) {
    const touch = new TouchInput(); touch.begin(1,300,500,true);
    assert.equal(touch.sample().stunt_modifiers,0);
    touch.move(1,300+dx,500+dy);
    assert.deepEqual(touch.sample(), { lean:0,preload_axis:null,stunt_modifiers:1,stunt_x:x,stunt_y:y });
    touch.move(1,300-dx,500-dy); assert.equal(touch.sample().stunt_x,x);
    touch.end(1); assert.deepEqual(touch.sample(), { lean:0,preload_axis:null });
  }
});
