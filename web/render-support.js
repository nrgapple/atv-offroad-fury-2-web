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
