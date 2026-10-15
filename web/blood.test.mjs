import test from 'node:test';
import assert from 'node:assert/strict';
import { packBlood, bloodCount, BLOOD_STRIDE, BLOOD_CAPACITY } from './wheel-effects.js';

test('blood packs splats, then stump caps, then droplet streaks', () => {
  const blood = {
    drops: [1, 2, 3, 0, -20, 10, 0.05],
    splats: [4, 0, 5, 0, 1, 0, 0.6, 0.8, 0.3, 0.7],
    stumps: [7, 1, 8, 0, 0, 1, 0.2],
  };
  const out = new Float32Array(BLOOD_CAPACITY * BLOOD_STRIDE);
  assert.equal(packBlood(blood, out), 3);
  assert.equal(bloodCount(blood), 2);
  const at = i => Array.from(out.subarray(i * BLOOD_STRIDE, (i + 1) * BLOOD_STRIDE));
  assert.deepEqual(at(0).slice(0, 4), [4, 0, 5, 1]);
  assert.deepEqual(at(0).slice(8).map(v => +v.toFixed(2)), [0.6, 0.8, 0.3, 0.7]);
  assert.deepEqual(at(1).slice(0, 4), [7, 1, 8, 2]);
  assert.equal(at(1)[10], Math.fround(0.2));
  const drop = at(2);
  assert.equal(drop[3], 0);
  // Fast droplets stretch along their velocity, capped at one foot.
  assert(drop[11] > drop[10] && drop[11] <= 1);
});

test('blood packing is bounded and tolerates missing data', () => {
  const out = new Float32Array(BLOOD_CAPACITY * BLOOD_STRIDE);
  assert.equal(packBlood(null, out), 0);
  assert.equal(bloodCount(undefined), 0);
  const drops = Array.from({ length: 400 }, (_, i) => [i, 1, 0, 0, 0, 0, 0.05]).flat();
  assert.equal(packBlood({ drops }, out), BLOOD_CAPACITY);
});
