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
function createShake() {
 let seconds=0;const s=new VehicleShake(()=>seconds),update=s.update.bind(s);
 s.update=(actor,dt,...rest)=>{seconds+=Math.max(0,Number.isFinite(dt)?dt:0);update(actor,dt,...rest);};
 return s;
}
const shakeActor={origin:[0,3,0],linear_velocity:[0,0,0],speed:0,rpm:1000,grounded:[true,true],wreck:{phase:'Riding'},reset_count:0,crash_impact_speed:0};
test('ordinary riding, RPM, acceleration and wheel contacts remain completely steady',()=>{
 const s=createShake();s.accept(shakeActor);
 for(let i=0;i<600;i++) {
  const actor={...shakeActor,speed:70,rpm:8000,linear_velocity:[i%2?5:0,i%3,70],crash_impact_speed:35,suspension_stop_contacts:2};
  assert.equal(s.accept(actor),0);s.update(actor,1/60);assert.deepEqual(s.offset,[0,0,0,0]);
 }
 assert.equal(s.impacts,0);
});
const collision={...shakeActor,crash_impact_speed:80,obstacle_contact_count:1,linear_velocity:[0,0,-20],wreck:{phase:'Front'}};
test('a real crash has one bounded rotation pulse that ends exactly, without translation',()=>{
 const s=createShake();s.accept(shakeActor);assert.equal(s.accept(collision),80);
 const view={position:[0,10,-15],yaw:0,pitch:-.3};
 let moving=0;
 for(let i=0;i<180;i++) {
  s.update(collision,1/60);assert.equal(s.accept(collision),0);
  const shown=s.apply(view);assert.deepEqual(shown.position,view.position);
  if(s.offset.some(Boolean))moving++;
  assert(Math.abs(s.offset[0])<=.025 && Math.abs(s.offset[1])<=.035);
 }
 assert(moving>0 && moving<30);assert.equal(s.energy,0);assert.deepEqual(s.offset,[0,0,0,0]);assert.equal(s.impacts,1);
});
test('genuine airborne landing triggers; single wheel contact gaps do not',()=>{
 const s=createShake();s.accept(shakeActor);
 const air={...shakeActor,grounded:[false,false],linear_velocity:[0,-20,0]};
 s.accept(air);assert.equal(s.accept({...shakeActor,crash_impact_speed:35}),0);
 for(let i=0;i<15;i++)s.accept(air);
 assert.equal(s.accept({...shakeActor,crash_impact_speed:35}),35);
});
test('shake Off, pause and reset clear the pulse immediately',()=>{
 for(const clear of [s=>s.update(collision,1/60,false),s=>s.update(collision,1/60,true,false),s=>s.accept({...collision,reset_count:1}),s=>s.reset()]) {
  const s=createShake();s.accept(shakeActor);s.accept(collision);s.update(collision,1/60);assert(s.energy>0);
  clear(s);assert.equal(s.energy,0);assert.deepEqual(s.offset,[0,0,0,0]);
 }
});

test('blocked browser storage getter still permits session settings',()=>{
 const descriptor=Object.getOwnPropertyDescriptor(globalThis,'localStorage');
 try {
  Object.defineProperty(globalThis,'localStorage',{configurable:true,get(){throw new Error('Storage denied');}});
  const s=new RideEffectsSettings();assert.equal(s.mode,'crazy');s.toggle();s.toggleShake();assert.equal(s.mode,'gorn');assert.equal(s.shake,false);
 } finally {if(descriptor)Object.defineProperty(globalThis,'localStorage',descriptor);else delete globalThis.localStorage;}
});

test('a severe crash upgrades an earlier hit once without repeated bounce pulses',()=>{
 const s=createShake();s.accept(shakeActor);
 const mild={...collision,crash_impact_speed:40};assert.equal(s.accept(mild),40);s.update(mild,1/60);
 const severe={...collision,linear_velocity:[0,0,0]};assert.equal(s.accept(severe),80);
 for(let i=0;i<60;i++){s.update(severe,1/60);assert.equal(s.accept({...severe,linear_velocity:[0,0,i%2?20:0]}),0);}
 assert.equal(s.impacts,2);
});
const {severedIndices,meshNodeBounds,poseBounds,crashPresentation}=await import('./render-support.js');
test('severing removes bridge triangles but preserves geometry within each connected limb',()=>{
 const nodes=[{parent:-1},{parent:0},{parent:1},{parent:0}];
 const group={vertices:[{node:0},{node:1},{node:2},{node:3},{node:1},{node:2}],indices:[0,1,2,1,2,4,0,3,0,2,4,5]};
 assert.deepEqual([...severedIndices(group,nodes,[])],group.indices);
 assert.deepEqual([...severedIndices(group,nodes,[1])],[1,2,4,0,3,0,2,4,5]);
});
test('crash framing encloses diverging ATV, rider and debris in landscape and portrait',()=>{
 const bounds={min:[-20,0,-12],max:[30,25,22]};
 const original={position:[0,10,-15],yaw:.8,pitch:-.3,presentation:{focalLength:2.3}};
 for(const aspect of [.46,1.5,2.2])for(const floor of [0,80]) {
  const v=crashPresentation(original,bounds,aspect,()=>floor);
  assert(v.position[1]>=floor+2);
  const forward=[Math.sin(v.yaw)*Math.cos(v.pitch),Math.sin(v.pitch),Math.cos(v.yaw)*Math.cos(v.pitch)];
  const right=[-Math.cos(v.yaw),0,Math.sin(v.yaw)],up=[-Math.sin(v.yaw)*Math.sin(v.pitch),Math.cos(v.pitch),-Math.cos(v.yaw)*Math.sin(v.pitch)];
  for(let c=0;c<8;c++) {
   const p=bounds.min.map((x,i)=>((c&(1<<i))?bounds.max[i]:x)-v.position[i]);const dot=a=>a.reduce((sum,x,i)=>sum+x*p[i],0),z=dot(forward);
   assert(z>1);assert(Math.abs(dot(right)*2.3/aspect/z)<=.77);assert(Math.abs(dot(up)*2.3/z)<=.77);
  }
 }
 assert.deepEqual(original.position,[0,10,-15]);assert.equal(crashPresentation(original,null,1,()=>0),original);
});
test('crash bounds follow the actual rendered node transforms',()=>{
 const m=[1,0,0,0,0,1,0,0,0,0,1,0,10,20,30,1];
 const bounds=meshNodeBounds([{vertices:[{node:0,local_position:[-1,-2,-3]},{node:0,local_position:[1,2,3]}]}],2);
 assert.deepEqual(poseBounds(bounds,[m,m]),{min:[9,18,27],max:[11,22,33]});
});

test('a slow frame cannot prolong an impulse and a fresh hit does not inherit earlier stalled time',()=>{
 let seconds=0;const s=new VehicleShake(()=>seconds);
 s.accept(shakeActor);seconds=2;assert.equal(s.accept(collision),80);
 s.update(collision,.05);assert(s.energy>0,'fresh impulse starts at delivery, even after a stalled frame');
 seconds+=.016;s.update(collision,.016);assert(s.offset.some(Boolean));
 seconds+=.50;s.update(collision,.05);assert.equal(s.energy,0);assert.deepEqual(s.offset,[0,0,0,0]);
});
