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
