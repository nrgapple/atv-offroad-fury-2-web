import test from 'node:test';
import assert from 'node:assert/strict';
import { vertexBounds, worldBounds, viewPlanes, outsideView } from './render-support.js';
const identity = [1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1];
const planes = viewPlanes(identity);
test('boxes crossing clip planes survive, including near-plane and camera-containing boxes', () => {
  assert(!outsideView({ min: [-2,-2,-2], max: [2,2,2] }, planes));
  assert(!outsideView({ min: [.9,0,0], max: [100,1,1] }, planes));
  for (const axis of [0,1,2]) for (const sign of [-1,1]) {
    const min = [0,0,0], max = [.1,.1,.1]; min[axis] = sign === 1 ? 2 : -3; max[axis] = sign === 1 ? 3 : -2;
    assert(outsideView({ min,max }, planes));
  }
});
test('rotated, reflected and scaled placements bound every transformed vertex', () => {
  const input = [-2,0,1,0,0,0, 3,4,-1,0,0,0], local = vertexBounds(input);
  const m = [0,0,2,0, 1,3,0,0, -2,0,0,0, 4000,50,3500,1];
  const b = worldBounds(local,m);
  for (let i=0;i<input.length;i+=6) for (let a=0;a<3;a++) {
    const v=m[a]*input[i]+m[4+a]*input[i+1]+m[8+a]*input[i+2]+m[12+a];
    assert(v>=b.min[a]&&v<=b.max[a]);
  }
});
test('unknown bounds and degenerate matrices fail open, float32 margins retain boundary boxes', () => {
  assert(!outsideView(null,planes)); assert.deepEqual(viewPlanes(new Array(16).fill(0)),[]);
  assert(!outsideView({min:[1.00001,0,0],max:[1.00002,1,1]},planes));
  assert.equal(vertexBounds([]),null);
});
