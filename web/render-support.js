// Original camera looks along +Z with +X on screen-right (0013be20).
// WebGL depth uses -Z; convert depth without reflecting the source world in X.
export function cameraAxes(yaw, pitch) {
  return {
    forward: [Math.sin(yaw)*Math.cos(pitch), Math.sin(pitch), Math.cos(yaw)*Math.cos(pitch)],
    right: [Math.cos(yaw), 0, -Math.sin(yaw)],
    up: [-Math.sin(yaw)*Math.sin(pitch), Math.cos(pitch), -Math.cos(yaw)*Math.sin(pitch)],
  };
}
export function sceneViewProjection(camera, aspect, focalLength, near, far) {
  const {x,y,z,yaw,pitch}=camera, {forward,right,up}=cameraAxes(yaw,pitch);
  const dot=v=>v[0]*x+v[1]*y+v[2]*z;
  const view=new Float32Array([right[0],up[0],-forward[0],0,right[1],up[1],-forward[1],0,right[2],up[2],-forward[2],0,-dot(right),-dot(up),dot(forward),1]);
  const f=focalLength;
  const projection=new Float32Array([f/aspect,0,0,0,0,f,0,0,0,0,(far+near)/(near-far),-1,0,0,2*far*near/(near-far),0]);
  const result=new Float32Array(16);
  for(let column=0;column<4;column++)for(let row=0;row<4;row++)for(let k=0;k<4;k++)result[column*4+row]+=projection[k*4+row]*view[column*4+k];
  return result;
}

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

// Optional chase adaptation (Chase camera: Assisted). The recovered camera frame
// stays intact; a null actor (option Off) clears it to the unmodified rig.
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
// Without a saved choice, crashes and the chase camera use the recovered original.
export class RideEffectsSettings {
  constructor(storage, reducedMotion = false) {
    if (storage === undefined) {
      try { storage = globalThis.localStorage; } catch { storage = null; }
    }
    this.storage = storage; this.mode = 'original'; this.camera = 'tight'; this.chase = 'original'; this.speed = reducedMotion ? 'off' : 'normal';
    try {
      const speed = storage?.getItem('atv-speed-rush');
      if (['off','low','normal','high'].includes(speed)) this.speed = speed;
      this.camera = storage?.getItem('atv-crash-camera') === 'original' ? 'original' : 'tight';
      this.chase = storage?.getItem('atv-chase-camera') === 'assisted' ? 'assisted' : 'original';
      const mode = storage?.getItem('atv-crash-mode');
      this.mode = ['original','crazy','gorn'].includes(mode) ? mode : storage?.getItem('atv-funny-crashes') === 'true' ? 'crazy' : 'original';
    } catch { /* Session settings. */ }
  }
  toggleSpeed() {
    this.speed = ['off','low','normal','high'][(['off','low','normal','high'].indexOf(this.speed)+1)%4];
    try { this.storage?.setItem('atv-speed-rush',this.speed); } catch { /* Session settings. */ }
    this.onSpeedChange?.();
  }
  toggleCamera() {
    this.camera = this.camera === 'tight' ? 'original' : 'tight';
    try { this.storage?.setItem('atv-crash-camera',this.camera); } catch { /* Session settings. */ }
    this.onCameraChange?.();
  }
  get chaseAssist() { return this.chase === 'assisted'; }
  toggleChase() {
    this.chase = this.chaseAssist ? 'original' : 'assisted';
    try { this.storage?.setItem('atv-chase-camera',this.chase); } catch { /* Session settings. */ }
    this.onChaseChange?.();
  }
  get enabled() { return this.mode !== 'original'; }
  toggle() {
    this.mode = ['original','crazy','gorn'][(['original','crazy','gorn'].indexOf(this.mode)+1)%3];
    try { this.storage?.setItem('atv-crash-mode',this.mode); this.storage?.setItem('atv-funny-crashes',String(this.enabled)); } catch { /* Session settings. */ }
    this.onChange?.();
  }
}

// Impact audio event detection only. It never modifies the camera.
export class ImpactEvents {
  constructor(clock=()=>performance.now()/1000) { this.clock=clock; this.reset(); }
  reset() { this.previous=null; this.cooldownUntil=0; this.airTime=0; this.crashSeverity=0; this.impacts=0; }
  accept(actor, enabled=true) {
    const old=this.previous; this.previous=actor;
    const now=this.clock();
    if (!actor || !enabled || !old || old.reset_count!==actor.reset_count || Math.hypot(...actor.origin.map((v,i)=>v-old.origin[i]))>40) {
      this.airTime=0; this.crashSeverity=0; this.cooldownUntil=0; return 0;
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
      this.cooldownUntil=now+.30;
      if (!riding || body) this.crashSeverity=Math.max(this.crashSeverity,severity);
      this.impacts++; return severity;
    }
    return 0;
  }
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

// Only the central body participates: detached wheels/limbs must never zoom the
// camera out. Model node names come from the decoded runtime mesh.
export function crashSubjects(vehicle, frame, quadPose, actorWorld) {
  const central = (bounds, nodes, pattern, matrices, world) => poseBounds(
    bounds.map((b,i)=>pattern.test(nodes[i]?.name ?? '') ? b : null), matrices, world);
  const riderWorld=frame.funny_crash?.rider_node_matrices.length ? null : frame.rider_world_matrix??actorWorld;
  const torso=central(vehicle.rider.boundsByNode,vehicle.rider.mesh.nodes,/torso|pelvis/i,vehicle.rider.pose.world_matrices,riderWorld);
  const bike=central(vehicle.boundsByNode,vehicle.nodes,/chassis|body|frame/i,quadPose,actorWorld);
  const center=b=>b.min.map((v,i)=>(v+b.max[i])/2);
  return { rider:torso ? center(torso) : (riderWorld ? [riderWorld[12],riderWorld[13]+2,riderWorld[14]] : frame.origin.map((v,i)=>v+(i===1?2:0))), bike:bike ? center(bike) : [...frame.origin] };
}

// Tight authored crash shot. Nearby centers share the frame; as they separate,
// smoothly drop the bike and stay with the rider. Terrain raises the eye instead
// of causing an unbounded backwards dolly.
export function crashPresentation(view, subjects, aspect, height) {
  if(!subjects)return view;
  const {rider,bike}=subjects;
  const separation=Math.hypot(...bike.map((v,i)=>v-rider[i]));
  const t=Math.max(0,Math.min(1,(separation-8)/14));
  const bikeWeight=.5*(1-t*t*(3-2*t));
  const target=rider.map((v,i)=>v+(bike[i]-v)*bikeWeight);
  const pitch=Math.max(-.55,Math.min(-.22,view.pitch)),yaw=view.yaw;
  const {forward,right,up}=cameraAxes(yaw,pitch);
  const f=view.presentation.focalLength;
  let distance=16;
  const bounds={min:target.map(v=>v-2.5),max:target.map(v=>v+2.5)};
  for(const [point,weight] of [[rider,1],[bike,bikeWeight*2]]) {
    const p=point.map((v,i)=>(v-target[i])*weight);
    const dot=v=>v.reduce((sum,x,i)=>sum+x*p[i],0),depth=dot(forward);
    distance=Math.max(distance,(Math.abs(dot(right))+2.5)*f/aspect/.8-depth,(Math.abs(dot(up))+2.5)*f/.8-depth);
    for(let i=0;i<3;i++){bounds.min[i]=Math.min(bounds.min[i],target[i]+p[i]-2.5);bounds.max[i]=Math.max(bounds.max[i],target[i]+p[i]+2.5);}
  }
  distance=Math.min(32,distance);
  const position=target.map((v,i)=>v-forward[i]*distance);
  position[1]=Math.max(position[1],height(position[0],position[2])+2);
  const aimPitch=Math.atan2(target[1]-position[1],Math.hypot(target[0]-position[0],target[2]-position[2]));
  return {...view,position,yaw,pitch:aimPitch,presentation:{...view.presentation,crash:{target,bounds,distance,separation,bikeWeight,rider,bike}}};
}

export function meshNodeBounds(groups, count) {
  const bounds=Array.from({length:count},()=>null);
  for(const group of groups)for(const v of group.vertices) {
    const b=bounds[v.node]??={min:[Infinity,Infinity,Infinity],max:[-Infinity,-Infinity,-Infinity]};
    for(let a=0;a<3;a++){b.min[a]=Math.min(b.min[a],v.local_position[a]);b.max[a]=Math.max(b.max[a],v.local_position[a]);}
  }
  return bounds;
}
