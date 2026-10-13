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

// Read-only vehicle response; never alters the recovered camera or physics.
export class VehicleShake {
  constructor() { this.reset(); }
  reset() { this.previous = null; this.energy = 0; this.roughness = 0; this.time = 0; this.cooldown = 0; this.offset = [0,0,0,0]; this.impacts = 0; }
  accept(actor, enabled = true) {
    const old = this.previous;
    this.previous = actor;
    if (!actor || !enabled || !old || old.reset_count !== actor.reset_count || Math.hypot(...actor.origin.map((v,i)=>v-old.origin[i])) > 40) {
      this.energy = 0; this.roughness = 0; this.offset = [0,0,0,0]; return 0;
    }
    const contact = Number.isFinite(actor.crash_impact_speed) ? actor.crash_impact_speed : 0;
    const rider = Math.hypot(...(actor.rider_wreck?.sampled_impact_delta ?? [0,0,0]));
    const severity = Math.max(contact, rider);
    const acceleration = Math.hypot(...(actor.linear_velocity ?? [0,0,0]).map((v,i)=>v-(old.linear_velocity?.[i] ?? v)));
    const grounded = actor.grounded?.some(Boolean);
    const desired = grounded && actor.wreck?.phase === 'Riding' ? Math.min(1, acceleration / 8) : 0;
    this.roughness += (desired - this.roughness) * .22;
    // One impulse per separated impact, rather than replaying every contact frame.
    const previousImpact = Math.max(old.crash_impact_speed ?? 0,Math.hypot(...(old.rider_wreck?.sampled_impact_delta ?? [0,0,0])));
    if (severity > 16 && this.cooldown <= 0 && (previousImpact <= 16 || severity > previousImpact*1.35)) {
      this.energy = Math.min(1, Math.max(this.energy, (severity-12)/55));
      this.cooldown = .25; this.impacts++;
      return severity;
    }
    return 0;
  }
  update(actor, dt, enabled = true, active = true) {
    this.cooldown = Math.max(0,this.cooldown-(Number.isFinite(dt) ? Math.max(0,Math.min(.05,dt)) : 0));
    if (!enabled || !active) { this.energy=0; this.roughness=0; this.offset=[0,0,0,0]; return; }
    dt = Number.isFinite(dt) ? Math.max(0,Math.min(.05,dt)) : 0;
    this.time += dt;
    this.energy *= Math.exp(-dt*5.2);
    const riding = actor?.wreck?.phase === 'Riding', grounded = actor?.grounded?.some(Boolean);
    const speed = Math.min(1,Math.abs(actor?.speed ?? 0)/70);
    const engine = riding && grounded && speed > .015 ? .0008*Math.min(1,(actor.rpm ?? 0)/8000) : 0;
    const vibration = riding && grounded ? .0025*speed + .007*this.roughness : 0;
    const strength = vibration+engine;
    const t = this.time, e = this.energy;
    this.offset = [
      Math.sin(t*83)*strength + Math.sin(t*41)*e*.055,
      Math.sin(t*107+.7)*strength*.7 + Math.sin(t*47+.4)*e*.045,
      Math.sin(t*67)*strength*2 + Math.sin(t*35)*e*.28,
      e*.12,
    ];
  }
  apply(view, height) {
    const [yaw,pitch,up,dolly] = this.offset;
    const position = [...view.position];
    position[0] -= Math.sin(view.yaw)*dolly;
    position[2] -= Math.cos(view.yaw)*dolly;
    position[1] += up;
    if (up || dolly) position[1]=Math.max(position[1],height(position[0],position[2])+1);
    return {...view,position,yaw:view.yaw+yaw,pitch:view.pitch+pitch};
  }
}
