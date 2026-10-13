import test from 'node:test';
import assert from 'node:assert/strict';
import { vertexBounds, worldBounds, viewPlanes, outsideView, UphillSightline, chasePresentation } from './render-support.js';
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

const actor = { orientation: [1,0,0,0], origin: [0,1,0], grounded: [true,true,true,true], wreck: { phase: 'Riding' }, speed: 20, reset_count: 0 };
const camera = { preset: 0 };
const advance = (sightline, a, height, seconds = 3, hz = 60) => {
  for (let i=0;i<seconds*hz;i++) sightline.update(a,camera,1/hz,height);
};
test('flat-ground lean and wheelies never move the camera, even past vertical', () => {
  const sightline = new UphillSightline();
  for (const degrees of [0,30,80,89,90,91,110,150,-30]) {
    const pitch = degrees*Math.PI/360;
    const tilted = {...actor,orientation:[Math.cos(pitch),-Math.sin(pitch),0,0]};
    advance(sightline,tilted,()=>0);
    assert.equal(sightline.lift,0); assert.equal(sightline.pullback,0);
  }
});
test('hill anticipation ignores chassis pitch and rejects a single terrain spike', () => {
  const slope = new UphillSightline(); advance(slope,actor,(x,z)=>z);
  assert(slope.lift>.50 && slope.lift<.52);
  assert(slope.pullback>.23 && slope.pullback<.24);
  for (const degrees of [60,90,110]) {
    const other = new UphillSightline(), pitch=degrees*Math.PI/360;
    advance(other,{...actor,orientation:[Math.cos(pitch),-Math.sin(pitch),0,0]},(x,z)=>z);
    assert(Math.abs(other.lift-slope.lift)<1e-10, 'pitch must not reverse terrain lookahead');
  }
  const spike = new UphillSightline(); advance(spike,actor,(x,z)=>z===20?100:0);
  assert.equal(spike.lift,0);
  const downhill = new UphillSightline(); advance(downhill,actor,(x,z)=>-z);
  assert.equal(downhill.lift,0);
});
test('crest, wheel-contact gaps and reverse settle smoothly; airborne flips never steer the camera', () => {
  const slope = new UphillSightline(); advance(slope,actor,(x,z)=>z);
  const previous=slope.lift;
  const flying={...actor,grounded:[false,false,false,false],orientation:[0,1,0,0]};
  advance(slope,flying,()=>0,.2); assert.equal(slope.lift,previous);
  advance(slope,flying,()=>0,3); assert(slope.lift<.02);
  advance(slope,actor,(x,z)=>z);
  const before=slope.lift; slope.update({...actor,speed:-5},camera,1/60,()=>0);
  assert(slope.lift>before*.97 && slope.lift<before,'reverse releases without a snap');
  advance(slope,actor,()=>0,4); assert(slope.lift<.005);
  for(const [a,c] of [[actor,{preset:1}],[actor,{preset:0,mode:'wreck'}],[{...actor,wreck:{phase:'Front'}},camera],[{...actor,reset_count:1},camera]]) {
    advance(slope,actor,(x,z)=>z); slope.update(a,c,1/60,()=>0);
    assert.equal(slope.lift,0); assert.equal(slope.pullback,0);
  }
});
test('damping has the same response across render rates and a delayed frame', () => {
  const results=[30,60,120].map(hz=>{const s=new UphillSightline();advance(s,actor,(x,z)=>z,1,hz);return s.lift;});
  assert(Math.max(...results)-Math.min(...results)<1e-12);
  const delayed=new UphillSightline(); delayed.update(actor,camera,1,(x,z)=>z);
  assert(Math.abs(delayed.lift-results[0])<1e-12);
});
test('orbit and pullback keep the rider framed and the camera above terrain', () => {
  const frame={position:[0,10,-15],target:[0,3,0],fov:85,preset:0};
  const level=chasePresentation(frame,actor,16/9,{lift:0,pullback:0},()=>0);
  assert.deepEqual(level.position,frame.position); assert.equal(level.presentation.uphillLift,0);
  for(const aspect of [16/9,390/844]) {
    const baseline=chasePresentation(frame,actor,aspect,{lift:0,pullback:0},()=>-100);
    const hill=chasePresentation(frame,actor,aspect,{lift:.50,pullback:.23},()=>-100);
    const screenAngle=view=>Math.atan2(actor.origin[1]+3-view.position[1],Math.hypot(view.position[0],view.position[2]))-view.pitch;
    assert(Math.abs(screenAngle(hill)-screenAngle(baseline))<.1,'rider screen angle remains stable while looking up');
    assert(hill.presentation.distanceScale>baseline.presentation.distanceScale);
    assert(Math.abs(hill.presentation.uphillLift-.50)<1e-12);
    const obstructed=chasePresentation(frame,actor,aspect,{lift:.50,pullback:.23},()=>8);
    assert(obstructed.position[1]>=10); assert(Number.isFinite(obstructed.pitch));
  }
  assert.deepEqual(frame.position,[0,10,-15],'recovered frame is not mutated');
});

const {RideEffectsSettings,VehicleShake}=await import('./render-support.js');
test('effect preferences migrate saved Off, cycle three modes, persist and tolerate blocked storage',()=>{
  const values=new Map([['atv-funny-crashes','false']]);
  const store={getItem:k=>values.get(k)??null,setItem:(k,v)=>values.set(k,v)};
  const settings=new RideEffectsSettings(store);assert.equal(settings.mode,'original');
  settings.toggle();assert.equal(settings.mode,'crazy');settings.toggle();assert.equal(settings.mode,'gorn');
  assert.equal(new RideEffectsSettings(store).mode,'gorn');settings.toggle();assert.equal(settings.enabled,false);
  settings.toggleShake();assert.equal(new RideEffectsSettings(store).shake,false);
  assert.equal(new RideEffectsSettings(null,true).shake,false);
  const blocked=new RideEffectsSettings({getItem(){throw Error();},setItem(){throw Error();}});
  assert.equal(blocked.mode,'crazy');blocked.toggle();blocked.toggleShake();assert.equal(blocked.mode,'gorn');assert.equal(blocked.shake,false);
});
const shakeActor={origin:[0,3,0],linear_velocity:[0,0,0],speed:0,rpm:1000,grounded:[true,true],wreck:{phase:'Riding'},reset_count:0,crash_impact_speed:0};
test('vehicle shake scales with riding, smooths bumps, reacts once to hard impact and settles',()=>{
  const s=new VehicleShake();s.accept(shakeActor);s.update(shakeActor,1/60);assert.deepEqual(s.offset,[0,0,0,0]);
  const ride={...shakeActor,speed:60,rpm:7500,linear_velocity:[1,3,50]};s.accept(ride);s.update(ride,1/60);assert(s.roughness>0);assert(Math.abs(s.offset[0])>0);
  const air={...ride,grounded:[false,false]};s.update(air,1/60);assert.deepEqual(s.offset,[0,0,0,0]);
  const bad={...ride,crash_impact_speed:70,wreck:{phase:'Front'}};
  assert.equal(s.accept(bad),70);s.update(bad,1/60);assert(s.energy>.8);assert.equal(s.accept(bad),0);
  const view={position:[0,10,-15],yaw:0,pitch:-.3};const shown=s.apply(view,()=>0);assert.notDeepEqual(shown,view);assert.deepEqual(view.position,[0,10,-15]);
  for(let i=0;i<180;i++)s.update(bad,1/60);assert(s.energy<1e-6);
  assert.equal(s.accept({...bad,reset_count:1}),0);assert.equal(s.energy,0);
});
test('shake Off, pause, camera reset and terrain clearance are immediate',()=>{
 const s=new VehicleShake();s.accept(shakeActor);s.accept({...shakeActor,crash_impact_speed:90});s.update(shakeActor,1/60);
 assert(s.energy>0);s.update(shakeActor,1/60,false);assert.deepEqual(s.offset,[0,0,0,0]);assert.equal(s.energy,0);
 s.cooldown=0;s.accept(shakeActor);s.accept({...shakeActor,crash_impact_speed:90});s.update(shakeActor,1/60);
 assert(s.apply({position:[0,0,0],yaw:0,pitch:0},()=>10).position[1]>=11);
 s.update(shakeActor,1/60,true,false);assert.deepEqual(s.offset,[0,0,0,0]);s.reset();assert.equal(s.previous,null);
});
