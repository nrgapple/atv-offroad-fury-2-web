import test from 'node:test';
import assert from 'node:assert/strict';
import { FrameStats } from './diagnostics.js';

test('frame timing measures rendered cadence and CPU cost separately', () => {
  const stats = new FrameStats();
  for (let i = 0; i <= 60; i++) stats.sample(i * 1000 / 60, 2, true);
  const value = stats.snapshot();
  assert(Math.abs(value.fps - 60) < .00001);
  assert(Math.abs(value.p95Ms - 1000 / 60) < .00001);
  assert.equal(value.renderCpuMs, 2);
  assert.equal(value.samples, 60);
});
test('pause gaps do not count as slow gameplay frames and history is bounded', () => {
  const stats = new FrameStats(); stats.sample(0, 1, true); stats.sample(20, 1, true);
  stats.sample(5000, 5000, false); stats.sample(10000, 1, true); stats.sample(10020, 1, true);
  assert.equal(stats.snapshot().fps, 50); assert.equal(stats.snapshot().renderCpuMs, 1);
  for (let i = 1; i <= 200; i++) stats.sample(10020 + i * 10, 3, true);
  assert.equal(stats.snapshot().samples, 120); assert.equal(stats.snapshot().fps, 100);
});
