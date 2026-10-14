// Conservative presentation-only bounds. Never used for contacts or gameplay.
export function vertexBounds(vertices, stride = 6) {
  const min = [Infinity, Infinity, Infinity], max = [-Infinity, -Infinity, -Infinity];
  for (let i = 0; i < vertices.length; i += stride) for (let a = 0; a < 3; a++) {
    min[a] = Math.min(min[a], vertices[i + a]); max[a] = Math.max(max[a], vertices[i + a]);
  }
  return [...min, ...max].every(Number.isFinite) ? { min, max } : null;
}
export function worldBounds(bounds, matrix) {
  if (!bounds) return null;
  const min = [Infinity, Infinity, Infinity], max = [-Infinity, -Infinity, -Infinity];
  for (let corner = 0; corner < 8; corner++) {
    const p = [0, 1, 2].map(a => (corner & (1 << a)) ? bounds.max[a] : bounds.min[a]);
    for (let a = 0; a < 3; a++) {
      const v = matrix[a] * p[0] + matrix[4 + a] * p[1] + matrix[8 + a] * p[2] + matrix[12 + a];
      min[a] = Math.min(min[a], v); max[a] = Math.max(max[a], v);
    }
  }
  return [...min, ...max].every(Number.isFinite) ? { min, max } : null;
}
export function viewPlanes(matrix) {
  const planes = [];
  for (const row of [0, 1, 2]) for (const sign of [1, -1]) {
    const p = [0, 1, 2, 3].map(c => matrix[c * 4 + 3] + sign * matrix[c * 4 + row]);
    const length = Math.hypot(p[0], p[1], p[2]);
    if (!(length > 0) || !p.every(Number.isFinite)) return [];
    planes.push(p.map(v => v / length));
  }
  return planes;
}
export function outsideView(bounds, planes) {
  if (!bounds) return false;
  for (const p of planes) {
    // Most-positive box corner. A box intersecting any clip plane stays drawn.
    let distance = p[3], magnitude = Math.abs(p[3]);
    for (let a = 0; a < 3; a++) {
      const v = p[a] * (p[a] >= 0 ? bounds.max[a] : bounds.min[a]);
      distance += v; magnitude += Math.abs(v);
    }
    // Allow for float32 GPU transforms at the original large world coordinates.
    if (distance < -(0.05 + magnitude * 1e-6)) return true;
  }
  return false;
}

// Requested default chase adaptation. The recovered camera frame stays intact.
export class UphillSightline {
  constructor() { this.lift = 0; this.pullback = 0; this.airTime = 0; this.resetCount = null; }
  update(actor, camera, dt, height) {
    if (!actor || camera?.preset !== 0 || camera.mode === 'wreck' || actor.wreck?.phase !== 'Riding') {
      this.lift = 0; this.pullback = 0; this.airTime = 0; return;
    }
    if (this.resetCount !== actor.reset_count) { this.lift = 0; this.pullback = 0; this.airTime = 0; this.resetCount = actor.reset_count; }
    const step = Number.isFinite(dt) ? Math.max(0, dt) : 0;
    let desired = this.lift;
    if (actor.speed < -1) {
      desired = 0;
    } else if (actor.grounded.some(Boolean)) {
      this.airTime = 0;
      const [w, x, y, z] = actor.orientation;
      // Right-axis heading survives even a wheelie beyond vertical. Projecting
      // chassis forward instead reverses heading as its pitch crosses 90 degrees.
      const rx = 1 - 2 * (y*y + z*z), rz = 2 * (x*z - w*y);
      const horizontal = Math.hypot(rx, rz);
      const [ox, , oz] = actor.origin;
      const ground = height(ox, oz);
      const grades = horizontal > .2 ? [8, 20, 36].map(span =>
        Math.atan2(height(ox - rz/horizontal*span, oz + rx/horizontal*span) - ground, span)) : [0, 0, 0];
      const grade = grades.every(Number.isFinite) ? grades.sort((a,b) => a-b)[1] : 0;
      // Smooth onset at gentle grades avoids threshold chatter. Cap the orbit
      // and use a modest dolly rather than a large pitch-only turn away from ATV.
      const amount = Math.max(0, Math.min(1, (grade - .10) / .75));
      desired = .52 * amount * amount * (3 - 2 * amount);
    } else {
      this.airTime += step;
      // Carry takeoff framing through short contact gaps; ignore aerial flips.
      if (this.airTime > .35) desired = 0;
    }
    const blend = 1 - Math.exp(-step / (desired > this.lift ? .45 : .80));
    this.lift += (desired - this.lift) * blend;
    this.pullback = .24 * this.lift / .52;
  }
}

// Rotate both eye and aim about rider height, retaining its screen placement.
// The terrain floor can reduce the orbit on steep ground; it takes precedence.
export function chasePresentation(frame, actor, aspect, sightline, height) {
  const portrait = Math.max(0, Math.min(1, (1 - aspect) / .5));
  const portraitDistanceScale = 1 + .35 * portrait;
  const uphillDistanceScale = 1 + sightline.pullback;
  const distanceScale = portraitDistanceScale * uphillDistanceScale;
  const offset = frame.target.map((value, i) => value - frame.position[i]);
  const horizontal = Math.hypot(offset[0], offset[2]);
  const yaw = Math.atan2(offset[0], offset[2]);
  const nativePitch = Math.atan2(offset[1], horizontal);
  const position = frame.position.map((value, i) => value - offset[i] * (distanceScale - 1));
  let target = [...frame.target];
  if (sightline.lift > 0 && actor && horizontal > 0) {
    const pivot = [actor.origin[0], actor.origin[1] + 3, actor.origin[2]];
    const dx = offset[0] / horizontal, dz = offset[2] / horizontal;
    const c = Math.cos(sightline.lift), s = Math.sin(sightline.lift);
    const orbit = point => {
      const along = (point[0]-pivot[0])*dx + (point[2]-pivot[2])*dz;
      const up = point[1]-pivot[1];
      const shift = along*c - up*s - along;
      return [point[0]+dx*shift, pivot[1]+up*c+along*s, point[2]+dz*shift];
    };
    position.splice(0, 3, ...orbit(position));
    target = orbit(target);
    position[1] = Math.max(position[1], height(position[0], position[2]) + 2);
  }
  const view = target.map((value,i) => value-position[i]);
  const hillPitch = Math.atan2(view[1], Math.hypot(view[0],view[2]));
  const pitchLift = .12 * portrait;
  const focalLength = (4 / 3) / Math.tan(frame.fov * Math.PI / 360) * (1 - .12 * portrait);
  return { position, yaw, pitch: hillPitch + pitchLift, presentation: {
    aspect, portrait, distanceScale, portraitDistanceScale, uphillDistanceScale,
    pitchLift, uphillLift: hillPitch-nativePitch, requestedHillOrbit: sightline.lift, focalLength,
  } };
}

// Saved authored options, including migration of the former boolean crash switch.
export class RideEffectsSettings {
  constructor(storage, reducedMotion = false) {
    if (storage === undefined) {
      try { storage = globalThis.localStorage; } catch { storage = null; }
    }
    this.storage = storage; this.mode = 'crazy'; this.shake = !reducedMotion;
    try {
      const mode = storage?.getItem('atv-crash-mode');
      this.mode = ['original','crazy','gorn'].includes(mode) ? mode : storage?.getItem('atv-funny-crashes') === 'false' ? 'original' : 'crazy';
      const shake = storage?.getItem('atv-camera-shake');
      if (shake === 'true' || shake === 'false') this.shake = shake === 'true';
    } catch { /* Session settings. */ }
  }
  get enabled() { return this.mode !== 'original'; }
  toggle() {
    this.mode = ['original','crazy','gorn'][(['original','crazy','gorn'].indexOf(this.mode)+1)%3];
    try { this.storage?.setItem('atv-crash-mode',this.mode); this.storage?.setItem('atv-funny-crashes',String(this.enabled)); } catch { /* Session settings. */ }
    this.onChange?.();
  }
  toggleShake() {
    this.shake = !this.shake;
    try { this.storage?.setItem('atv-camera-shake',String(this.shake)); } catch { /* Session settings. */ }
    this.onShakeChange?.();
  }
}

// Smooth deterministic gradient noise: independent channels, no per-frame random jumps.
function impactNoise(x, seed) {
  const cell = Math.floor(x), t = x-cell;
  const gradient = n => {
    let h = Math.imul(n ^ seed, 0x45d9f3b); h = Math.imul(h ^ (h >>> 16), 0x45d9f3b);
    return ((h ^ (h >>> 16)) >>> 0) / 0xffffffff * 2 - 1;
  };
  const fade = t*t*t*(t*(t*6-15)+10);
  return 2*((1-fade)*gradient(cell)*t + fade*gradient(cell+1)*(t-1));
}
// Event-only rotational impulses. GDC camera trauma squared with finite decay;
// ordinary speed, engine RPM and suspension contacts never create shake.
export class VehicleShake {
  constructor(clock=()=>performance.now()/1000) { this.clock=clock; this.reset(); }
  reset() {
    this.impulseAt=0; this.cooldownUntil=0;
    this.previous=null; this.energy=0; this.roughness=0; this.time=0;
    this.remaining=0; this.duration=0; this.strength=0; this.cooldown=0;
    this.airTime=0; this.crashSeverity=0; this.lastSeverity=0; this.impacts=0;
    this.offset=[0,0,0,0];
  }
  quiet() { this.energy=0; this.remaining=0; this.duration=0; this.offset.fill(0); }
  accept(actor, enabled=true) {
    const old=this.previous; this.previous=actor;
    const now=this.clock();
    if (!actor || !enabled || !old || old.reset_count!==actor.reset_count || Math.hypot(...actor.origin.map((v,i)=>v-old.origin[i]))>40) {
      this.quiet(); this.airTime=0; this.crashSeverity=0; this.cooldown=0; this.cooldownUntil=0; return 0;
    }
    const riding=actor.wreck?.phase==='Riding', grounded=actor.grounded?.some(Boolean);
    if (riding && old.wreck?.phase!=='Riding') this.crashSeverity=0;
    const airTime=this.airTime;
    this.airTime=grounded?0:this.airTime+1/60;
    const contact=actor.crash_impact_speed??0;
    const body=(actor.chassis_contact_count??0)+(actor.obstacle_contact_count??0)+(actor.dynamic_contact_count??0)>0;
    const delta=Math.hypot(...actor.linear_velocity.map((v,i)=>v-old.linear_velocity[i]));
    const landing=grounded && !old.grounded?.some(Boolean) && airTime>=.18 && old.linear_velocity[1]<-12;
    const rider=Math.hypot(...(actor.rider_wreck?.sampled_impact_delta??[0,0,0]));
    const severity=Math.max(body && delta>=5 && contact>=30 ? contact : 0, landing ? Math.max(contact,-old.linear_velocity[1]) : 0, rider>=40 ? rider : 0);
    // A crash gets one impulse and at most a meaningful severity upgrade. Its
    // repeated bouncing cannot keep the camera moving throughout recovery.
    const upgrade=severity>=50 && this.crashSeverity<50;
    if (severity && (now>=this.cooldownUntil || upgrade) && (riding || this.crashSeverity===0 || upgrade)) {
      this.strength=Math.min(1,Math.max(.2,(severity-12)/65));
      this.duration=severity>=50?.48:.26; this.remaining=this.duration; this.impulseAt=now;
      this.energy=this.strength; this.cooldown=.30; this.cooldownUntil=now+.30; this.lastSeverity=severity;
      if (!riding || body) this.crashSeverity=Math.max(this.crashSeverity,severity);
      this.impacts++; return severity;
    }
    return 0;
  }
  update(actor, dt, enabled=true, active=true) {
    const now=this.clock();
    this.cooldown=Math.max(0,this.cooldownUntil-now);
    if (!enabled || !active) { this.quiet(); return; }
    // Physics/control dt is capped by the game loop. Impulse age must use the
    // event's real clock, including a slow frame and a fresh late-delivered hit.
    this.time=now; this.remaining=Math.max(0,this.duration-(now-this.impulseAt));
    if (!this.remaining) { this.quiet(); return; }
    const attack=Math.min(1,(this.duration-this.remaining)/.012);
    this.energy=this.strength*this.remaining/this.duration;
    const amplitude=this.energy*this.energy*attack;
    this.offset[0]=impactNoise(this.time*19,17)*amplitude*.025;
    this.offset[1]=impactNoise(this.time*22,83)*amplitude*.035;
  }
  apply(view) { return {...view,yaw:view.yaw+this.offset[0],pitch:view.pitch+this.offset[1]}; }
}

// Cut topology at severed joints. A triangle must belong to one connected
// component; otherwise skinning it would bridge two independently moving limbs.
export function severedIndices(group, nodes, severed) {
  const cuts=new Set(severed);
  const components=nodes.map((_,i)=>{
    for(let depth=0;depth<nodes.length;depth++) {
      if(cuts.has(i))return i+1;
      const parent=nodes[i]?.parent;
      if(!(parent>=0))return 0;
      i=parent;
    }
    return 0;
  });
  const indices=[];
  for(let i=0;i<group.indices.length;i+=3) {
    const a=group.indices[i],b=group.indices[i+1],c=group.indices[i+2];
    const component=components[group.vertices[a].node];
    if(component===components[group.vertices[b].node] && component===components[group.vertices[c].node])indices.push(a,b,c);
  }
  return new Uint16Array(indices);
}

export function poseBounds(bounds, matrices, world=null) {
  const min=[Infinity,Infinity,Infinity],max=[-Infinity,-Infinity,-Infinity];
  for(let i=0;i<bounds.length;i++) {
    const b=worldBounds(bounds[i],matrices[i]);
    const transformed=world ? worldBounds(b,world) : b;
    if(transformed)for(let a=0;a<3;a++){min[a]=Math.min(min[a],transformed.min[a]);max[a]=Math.max(max[a],transformed.max[a]);}
  }
  return min.every(Number.isFinite)?{min,max}:null;
}

// Fit BOTH visible actors, using posed mesh bounds rather than the native
// rider's recovery animation. Safety margin also covers the small impact shake.
export function crashPresentation(view, bounds, aspect, height) {
  if(!bounds)return view;
  const target=bounds.min.map((v,i)=>(v+bounds.max[i])/2);
  const pitch=Math.max(-.55,Math.min(-.22,view.pitch)),yaw=view.yaw;
  const forward=[Math.sin(yaw)*Math.cos(pitch),Math.sin(pitch),Math.cos(yaw)*Math.cos(pitch)];
  const right=[-Math.cos(yaw),0,Math.sin(yaw)];
  const up=[-Math.sin(yaw)*Math.sin(pitch),Math.cos(pitch),-Math.cos(yaw)*Math.sin(pitch)];
  const f=view.presentation.focalLength;
  let distance=24;
  for(let corner=0;corner<8;corner++) {
    const p=bounds.min.map((v,i)=>((corner&(1<<i))?bounds.max[i]:v)-target[i]);
    const dot=v=>v.reduce((sum,x,i)=>sum+x*p[i],0),depth=dot(forward);
    distance=Math.max(distance,Math.abs(dot(right))*f/aspect/.76-depth,Math.abs(dot(up))*f/.76-depth,2-depth);
  }
  let position=target.map((v,i)=>v-forward[i]*distance);
  // Climb by pulling back along the same sightline, so terrain clearance never
  // changes the fitted orientation or drops either actor outside the frustum.
  for(let i=0;i<8;i++) {
    const deficit=height(position[0],position[2])+2-position[1];
    if(deficit<=0)break;
    distance+=deficit / -forward[1] + 1;
    position=target.map((v,i)=>v-forward[i]*distance);
  }
  return {...view,position,yaw,pitch,presentation:{...view.presentation,crash:{target,bounds,distance}}};
}

export function meshNodeBounds(groups, count) {
  const bounds=Array.from({length:count},()=>null);
  for(const group of groups)for(const v of group.vertices) {
    const b=bounds[v.node]??={min:[Infinity,Infinity,Infinity],max:[-Infinity,-Infinity,-Infinity]};
    for(let a=0;a<3;a++){b.min[a]=Math.min(b.min[a],v.local_position[a]);b.max[a]=Math.max(b.max[a],v.local_position[a]);}
  }
  return bounds;
}
