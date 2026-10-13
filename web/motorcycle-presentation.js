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
