import test from 'node:test';
import assert from 'node:assert/strict';
import { MotorcyclePresentation } from './motorcycle-presentation.js';
const matrix = x => [1,0,0,0,0,1,0,0,0,0,1,0,x,2,3,1];
const frame = (tick, x) => ({ vehicle_kind:'motorcycle', wreck:{phase:'Riding'}, reset_count:0, motorcycle_tick:tick, origin:[x,0,0], orientation:[1,0,0,0], quad_node_matrices:[matrix(x)], rider_node_matrices:[matrix(x)] });
const camera = x => ({preset:0,position:[x,2,-10],target:[x,2,0]});
test('one blend clock moves the bike, rider and camera together between batch endpoints', () => {
  const p=new MotorcyclePresentation(),a=frame(10,0),b=frame(14,4),ca=camera(0),cb=camera(4);
  p.accept(a,b,ca,cb,100);
  const s=p.sample(b,cb,100+1000/30);
  assert(Math.abs(s.alpha-.5)<1e-12); assert(s.interpolated);
  assert(Math.abs(s.frame.origin[0]-2)<1e-12);
  assert.deepEqual(s.frame.quad_node_matrices,s.frame.rider_node_matrices);
  assert.equal(s.camera.position[0],s.frame.origin[0]);
  assert.equal(b.origin[0],4,'Presentation never mutates Rust state');
  assert.equal(p.sample(b,cb,1000).alpha,1);
});
test('antipodal quaternions retain the same orientation', () => {
  const p=new MotorcyclePresentation(),a=frame(1,0),b=frame(2,1),c=camera(0);b.orientation=[-1,0,0,0];
  p.accept(a,b,c,c,0);assert.deepEqual(p.sample(b,c,5).frame.orientation,[1,0,0,0]);
});
test('resets, stunts, wrecks, camera changes, pauses and missed intervals snap safely', () => {
  for(const change of [b=>b.reset_count++,b=>b.wreck.phase='Ejected',b=>b.stunt={name:'Nac Nac'},b=>b.motorcycle_tick=10,b=>b.vehicle_kind='atv']) {
    const p=new MotorcyclePresentation(),a=frame(1,0),b=frame(2,1),c=camera(0);change(b);
    p.accept(a,b,c,c,0);assert.equal(p.sample(b,c,5).frame,b);
  }
  const p=new MotorcyclePresentation(),a=frame(1,0),b=frame(2,1),c=camera(0);
  p.accept(a,b,c,{...c,preset:1},0);assert.equal(p.sample(b,c,5).alpha,1);
  p.accept(a,b,c,c,0);assert.equal(p.sample(b,c,5,false).alpha,1);assert.equal(p.sample(b,c,6).alpha,1);
  p.accept(a,b,c,c,0,false);assert.equal(p.sample(b,c,5).alpha,1);
});
import { AtvPresentation } from './motorcycle-presentation.js';
const atv = (x, extra = {}) => ({ wreck:{phase:'Riding'}, reset_count:0, origin:[x,0,0], orientation:[1,0,0,0], quad_node_matrices:[matrix(x)], rider_node_matrices:[matrix(x),matrix(x)], ...extra });
test('ATV interpolation blends quad, rider, detached rider, crash poses, CPUs and camera on one clock', () => {
  const p = new AtvPresentation(), cpu = x => ({ origin:[x,0,0], yaw:3.1, physical:{ orientation:[1,0,0,0], quad_matrices:[matrix(x)], rider_matrices:[matrix(x)], rider_world_matrix:matrix(x) } });
  const a = atv(0, { rider_world_matrix:matrix(0), funny_crash:{quad_node_matrices:[matrix(0)],rider_node_matrices:[matrix(0)]}, race:{opponents:[cpu(0)]} });
  const b = atv(4, { rider_world_matrix:matrix(4), funny_crash:{quad_node_matrices:[matrix(4)],rider_node_matrices:[matrix(4)]}, race:{opponents:[{...cpu(4),yaw:-3.1}]} });
  p.accept(a, b, camera(0), camera(4), 100, 4);
  const s = p.sample(b, camera(4), 100 + 1000/30);
  assert(s.interpolated); assert(Math.abs(s.alpha - .5) < 1e-12);
  for (const value of [s.frame.origin[0], s.frame.quad_node_matrices[0][12], s.frame.rider_node_matrices[1][12], s.frame.rider_world_matrix[12], s.frame.funny_crash.quad_node_matrices[0][12], s.frame.funny_crash.rider_node_matrices[0][12], s.frame.race.opponents[0].origin[0], s.frame.race.opponents[0].physical.quad_matrices[0][12], s.frame.race.opponents[0].physical.rider_world_matrix[12], s.camera.position[0]]) assert(Math.abs(value - 2) < 1e-12);
  assert(Math.abs(Math.abs(s.frame.race.opponents[0].yaw) - Math.PI) < .01, 'yaw takes the short way round');
  assert.equal(b.origin[0], 4, 'presentation never mutates Rust frames');
  assert.equal(p.sample(b, camera(4), 100 + 1000).interpolated, false, 'the segment ends at the delivered frame');
  assert.equal(p.sample(atv(5), camera(5), 120).interpolated, false, 'a newer frame without a segment shows as delivered');
});
test('ATV interpolation snaps resets, teleports, motorcycles, attach changes, camera cuts and pauses', () => {
  const cases = [b => b.reset_count++, b => b.origin = [100,0,0], b => b.vehicle_kind = 'motorcycle', b => b.rider_world_matrix = matrix(1), b => b.funny_crash = {quad_node_matrices:[],rider_node_matrices:[]}, b => b.rider_node_matrices = [matrix(1)]];
  for (const change of cases) {
    const p = new AtvPresentation(), a = atv(0), b = atv(1); change(b);
    p.accept(a, b, camera(0), camera(1), 0, 1); assert.equal(p.sample(b, camera(1), 5).interpolated, false);
  }
  const p = new AtvPresentation(), a = atv(0), b = atv(1);
  p.accept(a, b, camera(0), {...camera(1), preset:1}, 0, 1); assert.equal(p.sample(b, camera(1), 5).interpolated, false);
  p.accept(a, b, camera(0), camera(1), 0, 6); assert.equal(p.sample(b, camera(1), 5).interpolated, false, 'missed interval');
  p.accept(a, b, camera(0), camera(1), 0, 1, false); assert.equal(p.sample(b, camera(1), 5).interpolated, false);
  p.accept(a, b, camera(0), camera(1), 0, 1); assert.equal(p.sample(b, camera(1), 5, false).interpolated, false); assert.equal(p.sample(b, camera(1), 6).interpolated, false);
  const c = atv(1, { race:{opponents:[{origin:[500,0,0]}]} }), d = atv(2, { race:{opponents:[{origin:[0,0,0]}]} });
  p.accept(c, d, camera(0), camera(1), 0, 1); assert.deepEqual(p.sample(d, camera(1), 8).frame.race.opponents[0].origin, [0,0,0], 'a reset CPU snaps alone');
});
import { blendMatrix } from './motorcycle-presentation.js';
test('node blends stay rigid: a fast-spinning wheel keeps its size mid-interval', () => {
  // A wheel rotating about X by 1.6 rad between ticks (about 60 mph), mirrored and scaled nodes too.
  const spin = (angle, sx = 1, x = 0) => [sx,0,0,0, 0,Math.cos(angle),Math.sin(angle),0, 0,-Math.sin(angle),Math.cos(angle),0, x,1,2,1];
  for (const sx of [1, -1, 2]) {
    const m = blendMatrix(spin(0.2, sx, 0), spin(1.8, sx, 4), 0.5), expected = spin(1, sx, 2);
    for (let c = 0; c < 3; c++) assert(Math.abs(Math.hypot(m[c*4], m[c*4+1], m[c*4+2]) - Math.abs(c ? 1 : sx)) < 1e-9, `axis ${c} keeps its length`);
    m.forEach((v, i) => assert(Math.abs(v - expected[i]) < 1e-9, `entry ${i}: ${v} vs ${expected[i]}`));
  }
  assert.deepEqual(blendMatrix(spin(0.3), spin(0.3), 0.7).map(v => Math.round(v * 1e9) / 1e9), spin(0.3).map(v => Math.round(v * 1e9) / 1e9));
});
