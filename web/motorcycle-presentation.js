// Presentation only. The Rust 60 Hz state, controls and crash effects stay intact.
const mix = (a, b, t) => a.map((v, i) => v + (b[i] - v) * t);
const normal = f => f?.vehicle_kind === 'motorcycle' && f.wreck?.phase === 'Riding' && !f.stunt && !f.rider_world_matrix;
const compatible = (a, b, ca, cb) => normal(a) && normal(b) && a.reset_count === b.reset_count && b.motorcycle_tick > a.motorcycle_tick && ca?.preset === cb?.preset && ca && cb && a.quad_node_matrices.length === b.quad_node_matrices.length && a.rider_node_matrices.length === b.rider_node_matrices.length;

export class MotorcyclePresentation {
  reset() { this.segment = null; }
  accept(previous, current, previousCamera, currentCamera, now, continuous = true) {
    this.reset();
    if (!continuous || !compatible(previous, current, previousCamera, currentCamera)) return;
    const ticks = current.motorcycle_tick - previous.motorcycle_tick;
    // Do not animate a missed/resumed interval or a reset across the map.
    if (ticks > 4) return;
    this.segment = { previous, current, previousCamera, currentCamera, now, duration: ticks * 1000 / 60 };
  }
  sample(frame, camera, now, active = true) {
    const s = this.segment;
    if (!active) this.reset();
    if (!active || !s || frame !== s.current) return { frame, camera, alpha: 1, interpolated: false };
    const alpha = Math.max(0, Math.min(1, (now - s.now) / s.duration));
    let q = s.current.orientation;
    if (q.reduce((sum, v, i) => sum + v * s.previous.orientation[i], 0) < 0) q = q.map(v => -v);
    const orientation = mix(s.previous.orientation, q, alpha);
    const length = Math.hypot(...orientation);
    // Both rigs use the same affine blend, preserving their shared grip/peg
    // locations through the whole interval rather than smoothing hands alone.
    const matrices = key => s.previous[key].map((m, i) => mix(m, s.current[key][i], alpha));
    return {
      frame: { ...frame, origin: mix(s.previous.origin, frame.origin, alpha), orientation: orientation.map(v => v / length), quad_node_matrices: matrices('quad_node_matrices'), rider_node_matrices: matrices('rider_node_matrices') },
      camera: { ...camera, position: mix(s.previousCamera.position, camera.position, alpha), target: mix(s.previousCamera.target, camera.target, alpha) },
      alpha, interpolated: alpha < 1,
    };
  }
}

// Enhanced/High ATV interpolation between delivered 60 Hz frames for the
// player quad, attached or detached rider, Crazy/Gorn presentation poses, CPU
// racers and the chase camera. Presentation only: inputs, physics, snapshots
// and Rust state are untouched, and anything discontinuous snaps.
const lerpInto = (a, b, t) => a.map((v, i) => v + (b[i] - v) * t);
const slerpish = (a, b, t) => {
  if (a.reduce((sum, v, i) => sum + v * b[i], 0) < 0) b = b.map(v => -v);
  const q = lerpInto(a, b, t), length = Math.hypot(...q);
  return q.map(v => v / length);
};
const sameShape = (a, b) => Array.isArray(a) && Array.isArray(b) && a.length === b.length && a.every((m, i) => m?.length === b[i]?.length);
const blendMatrices = (a, b, t) => a.map((m, i) => lerpInto(m, b[i], t));
// A respawned, recovered or teleported actor never slides across the map.
const near = (a, b, limit) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]) <= limit;
function atvCompatible(a, b) {
  return a && b && a !== b && a.vehicle_kind !== 'motorcycle' && b.vehicle_kind !== 'motorcycle' && a.reset_count === b.reset_count
    && near(a.origin, b.origin, 30) && sameShape(a.quad_node_matrices, b.quad_node_matrices) && sameShape(a.rider_node_matrices, b.rider_node_matrices)
    && Boolean(a.rider_world_matrix) === Boolean(b.rider_world_matrix) && Boolean(a.funny_crash) === Boolean(b.funny_crash);
}
function blendCrash(a, b, t) {
  if (!a || !b) return b;
  const out = { ...b };
  for (const key of ['quad_node_matrices', 'rider_node_matrices']) {
    if (!sameShape(a[key], b[key])) return b;
    out[key] = blendMatrices(a[key], b[key], t);
  }
  return out;
}
function blendOpponents(a, b, t) {
  if (!Array.isArray(a) || !Array.isArray(b) || a.length !== b.length) return b;
  return b.map((cpu, i) => {
    const old = a[i];
    if (!old || !near(old.origin, cpu.origin, 30) || Boolean(old.physical) !== Boolean(cpu.physical)) return cpu;
    const out = { ...cpu, origin: lerpInto(old.origin, cpu.origin, t) };
    if (Number.isFinite(old.yaw) && Number.isFinite(cpu.yaw)) { let d = cpu.yaw - old.yaw; d -= Math.round(d / (2 * Math.PI)) * 2 * Math.PI; out.yaw = old.yaw + d * t; }
    if (cpu.physical) {
      const p = cpu.physical, q = old.physical;
      if (!sameShape(q.quad_matrices, p.quad_matrices) || !sameShape(q.rider_matrices, p.rider_matrices) || Boolean(q.rider_world_matrix) !== Boolean(p.rider_world_matrix)) return cpu;
      out.physical = { ...p, orientation: slerpish(q.orientation, p.orientation, t), quad_matrices: blendMatrices(q.quad_matrices, p.quad_matrices, t), rider_matrices: blendMatrices(q.rider_matrices, p.rider_matrices, t),
        rider_world_matrix: p.rider_world_matrix ? lerpInto(q.rider_world_matrix, p.rider_world_matrix, t) : p.rider_world_matrix };
    }
    return out;
  });
}
export class AtvPresentation {
  reset() { this.segment = null; }
  // `ticks` is the number of 60 Hz steps between the two frames.
  accept(previous, current, previousCamera, currentCamera, now, ticks, continuous = true) {
    this.reset();
    if (!continuous || !(ticks >= 1 && ticks <= 4) || !atvCompatible(previous, current) || !previousCamera || !currentCamera || previousCamera.preset !== currentCamera.preset || previousCamera.mode !== currentCamera.mode) return;
    this.segment = { previous, current, previousCamera, currentCamera, now, duration: ticks * 1000 / 60 };
  }
  sample(frame, camera, now, active = true) {
    const s = this.segment;
    if (!active) this.reset();
    if (!active || !s || frame !== s.current) return { frame, camera, alpha: 1, interpolated: false };
    const t = Math.max(0, Math.min(1, (now - s.now) / s.duration));
    if (t >= 1) return { frame, camera, alpha: 1, interpolated: false };
    const a = s.previous, b = frame;
    const shown = { ...b, origin: lerpInto(a.origin, b.origin, t), orientation: slerpish(a.orientation, b.orientation, t),
      quad_node_matrices: blendMatrices(a.quad_node_matrices, b.quad_node_matrices, t), rider_node_matrices: blendMatrices(a.rider_node_matrices, b.rider_node_matrices, t),
      rider_world_matrix: b.rider_world_matrix ? lerpInto(a.rider_world_matrix, b.rider_world_matrix, t) : b.rider_world_matrix,
      funny_crash: blendCrash(a.funny_crash, b.funny_crash, t) };
    if (a.race && b.race) shown.race = { ...b.race, opponents: blendOpponents(a.race.opponents, b.race.opponents, t) };
    return { frame: shown, camera: { ...camera, position: lerpInto(s.previousCamera.position, camera.position, t), target: lerpInto(s.previousCamera.target, camera.target, t) }, alpha: t, interpolated: true };
  }
}
