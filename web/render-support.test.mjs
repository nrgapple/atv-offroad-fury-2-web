import test from 'node:test';
import assert from 'node:assert/strict';
import { cameraAxes, sceneViewProjection, vertexBounds, worldBounds, viewPlanes, outsideView, UphillSightline, chasePresentation } from './render-support.js';
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

const {RideEffectsSettings,ImpactEvents}=await import('./render-support.js');
test('crash preferences persist, migrate and ignore obsolete shake preferences',()=>{
 const values=new Map([['atv-funny-crashes','false'],['atv-camera-shake','true']]);
 const store={getItem:k=>values.get(k)??null,setItem:(k,v)=>values.set(k,v)};
 const settings=new RideEffectsSettings(store);assert.equal(settings.mode,'original');
 settings.toggle();settings.toggle();assert.equal(new RideEffectsSettings(store).mode,'gorn');
 settings.toggleCamera();assert.equal(new RideEffectsSettings(store).camera,'original');
 assert.equal(settings.shake,undefined);assert.equal(settings.toggleShake,undefined);
 const blocked=new RideEffectsSettings({getItem(){throw Error();},setItem(){throw Error();}});
 blocked.toggle();assert.equal(blocked.mode,'gorn');
});
test('impact audio retains contact gates without any camera transform',()=>{
 let clock=0;const events=new ImpactEvents(()=>clock);
 const actor={origin:[0,3,0],linear_velocity:[0,0,0],speed:0,rpm:1000,grounded:[true,true],wreck:{phase:'Riding'},reset_count:0,crash_impact_speed:0};
 events.accept(actor);
 for(let i=0;i<60;i++)assert.equal(events.accept({...actor,crash_impact_speed:35,suspension_stop_contacts:2}),0);
 const mild={...actor,crash_impact_speed:40,obstacle_contact_count:1,linear_velocity:[0,0,-20],wreck:{phase:'Front'}};
 assert.equal(events.accept(mild),40);
 const severe={...mild,crash_impact_speed:80,linear_velocity:[0,0,0]};assert.equal(events.accept(severe),80);
 for(let i=0;i<60;i++){clock+=1/60;assert.equal(events.accept({...severe,linear_velocity:[0,0,i%2?20:0]}),0);}
 assert.equal(events.impacts,2);assert.equal(events.apply,undefined);
 assert.equal(events.accept({...severe,reset_count:1}),0);
});
test('blocked browser storage getter permits session crash settings',()=>{
 const descriptor=Object.getOwnPropertyDescriptor(globalThis,'localStorage');
 try {Object.defineProperty(globalThis,'localStorage',{configurable:true,get(){throw Error('denied');}});
 const s=new RideEffectsSettings();s.toggle();assert.equal(s.mode,'gorn');
 } finally {if(descriptor)Object.defineProperty(globalThis,'localStorage',descriptor);else delete globalThis.localStorage;}
});
const {severedIndices,meshNodeBounds,poseBounds,crashPresentation}=await import('./render-support.js');
test('severing removes bridge triangles but preserves geometry within each connected limb',()=>{
 const nodes=[{parent:-1},{parent:0},{parent:1},{parent:0}];
 const group={vertices:[{node:0},{node:1},{node:2},{node:3},{node:1},{node:2}],indices:[0,1,2,1,2,4,0,3,0,2,4,5]};
 assert.deepEqual([...severedIndices(group,nodes,[])],group.indices);
 assert.deepEqual([...severedIndices(group,nodes,[1])],[1,2,4,0,3,0,2,4,5]);
});
test('crash shot stays tight, gives up a distant bike and clears terrain without dollying away',()=>{
 const original={position:[0,10,-15],yaw:.8,pitch:-.3,presentation:{focalLength:2.3}};
 for(const aspect of [.46,1.5,2.2])for(const gap of [0,5,10,16,22,100,1000]) {
  const subjects={rider:[0,3,0],bike:[gap,3,0]};
  const v=crashPresentation(original,subjects,aspect,()=>0),c=v.presentation.crash;
  assert(c.distance>=16&&c.distance<=32);assert(v.position.every(Number.isFinite));
  assert(Math.hypot(...v.position.map((x,i)=>x-c.target[i]))<=32.01);
  if(gap>=22){assert.deepEqual(c.target,subjects.rider);assert.equal(c.bikeWeight,0);}
  const hill=crashPresentation(original,subjects,aspect,()=>80);
  assert(hill.position[1]>=82);assert(hill.presentation.crash.distance<=32);
 }
 assert.deepEqual(original.position,[0,10,-15]);assert.equal(crashPresentation(original,null,1,()=>0),original);
});
test('crash bounds follow the actual rendered node transforms',()=>{
 const m=[1,0,0,0,0,1,0,0,0,0,1,0,10,20,30,1];
 const bounds=meshNodeBounds([{vertices:[{node:0,local_position:[-1,-2,-3]},{node:0,local_position:[1,2,3]}]}],2);
 assert.deepEqual(poseBounds(bounds,[m,m]),{min:[9,18,27],max:[11,22,33]});
});

// Asymmetric source coordinates catch a global mirror even when depth and
// ordinary driving smoke checks still pass. Reference: native +Z look/+Y up.
test('source camera puts +X on the right and preserves depth, pitch and clipping', () => {
  const dot=(a,b)=>a.reduce((sum,v,i)=>sum+v*b[i],0);
  for(const yaw of [0,.7,-1.8,Math.PI])for(const pitch of [0,-.45,.8]) {
    const camera={x:240,y:35,z:110,yaw,pitch},axes=cameraAxes(yaw,pitch);
    const nativeRight=[Math.cos(yaw),0,-Math.sin(yaw)];
    assert(dot(axes.right,nativeRight)>.99999);
    assert(Math.abs(dot(axes.right,axes.up))<1e-12);
    const m=sceneViewProjection(camera,16/9,1.2,1,1000);
    const point=(side,vertical,depth)=>[camera.x,camera.y,camera.z].map((v,i)=>v+nativeRight[i]*side+axes.up[i]*vertical+axes.forward[i]*depth);
    const project=p=>[0,1,2,3].map(row=>m[row]*p[0]+m[4+row]*p[1]+m[8+row]*p[2]+m[12+row]);
    const center=project(point(0,0,20)),right=project(point(4,0,20)),up=project(point(0,3,20));
    assert(center[3]>0&&Math.abs(center[0])<.0001&&Math.abs(center[1])<.0001);
    assert(right[0]/right[3]>.1,'Asymmetric landmark stays on original screen-right');
    assert(up[1]/up[3]>.1);
    const planes=viewPlanes(m);
    const tiny=p=>({min:p.map(v=>v-.01),max:p.map(v=>v+.01)});
    assert(!outsideView(tiny(point(4,0,20)),planes));
    assert(outsideView(tiny(point(0,0,-20)),planes));
  }
});
