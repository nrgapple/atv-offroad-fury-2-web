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

test('chase anticipates uphill terrain/props, smooths the crest and ignores airborne flips, reverse, wreck and other presets', async () => {
  const { UphillSightline } = await import('./render-support.js');
  const camera = { preset: 0 }, actor = { orientation: [1,0,0,0], origin: [0,1,0], grounded: [true,true,true,true], wreck: { phase: 'Riding' }, speed: 20, reset_count: 0 };
  const slope = new UphillSightline();
  for (let i = 0; i < 60; i++) slope.update(actor, camera, 1/60, (x,z) => z);
  assert(slope.lift > .58 && slope.lift < .61, '45 degree incline raises the sightline');
  const previous = slope.lift;
  const flying = { ...actor, grounded: [false,false,false,false], orientation: [0,1,0,0] };
  for (let i = 0; i < 12; i++) slope.update(flying, camera, 1/60, () => 0);
  assert.equal(slope.lift, previous, 'takeoff holds view independent of flip');
  for (let i = 0; i < 120; i++) slope.update(flying, camera, 1/60, () => 0);
  assert(slope.lift < .02, 'airborne view settles');
  const ramp = { ...actor, orientation: [Math.cos(Math.PI/8), -Math.sin(Math.PI/8),0,0] };
  for (let i = 0; i < 60; i++) slope.update(ramp, camera, 1/60, () => 0);
  assert(slope.lift > .58, 'tilted prop on flat terrain raises view');
  for (const [a,c] of [[actor,{preset:1}],[actor,{preset:0,mode:'wreck'}],[{...actor,speed:-5},camera],[{...actor,wreck:{phase:'Front'}},camera]]) {
    slope.update(a,c,1/60,()=>100); assert.equal(slope.lift,0);
  }
  for (let i = 0; i < 60; i++) slope.update(actor,camera,1/60,()=>0);
  assert.equal(slope.lift,0,'flat ground preserves recovered framing');
});
