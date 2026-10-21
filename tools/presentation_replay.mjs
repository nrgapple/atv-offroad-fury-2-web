// Replays a Rust engine presentation log (Riding::present_log: delivered
// states, batch timings and every sampled frame) through the JavaScript
// game's presenters and reports how far the Rust presented poses are from
// theirs. Shared by tools/compare_presentation.mjs (native) and
// tools/test_engine_interpolation.mjs (browser build).
import { AtvPresentation, MotorcyclePresentation } from '../web/motorcycle-presentation.js';

export function replay(log) {
  const bike = new MotorcyclePresentation(), atv = new AtvPresentation();
  let delivered = null;
  const worst = { origin: 0, orientation: 0, camera: 0, matrix: 0, cpu: 0 };
  let samples = 0, interpolated = 0, mismatched = 0;
  const alphas = [];
  const d = (a, b) => Math.hypot(...a.map((v, i) => v - b[i]));
  for (const event of log.events) {
    if (event.deliver) { delivered = event.deliver; bike.reset(); atv.reset(); continue; }
    if (event.accept) {
      const { now, ticks, continuous, frame } = event.accept;
      if (delivered) {
        bike.accept(delivered.frame, frame.frame, delivered.camera, frame.camera, now, continuous, 1);
        if (log.interpolation) atv.accept(delivered.frame, frame.frame, delivered.camera, frame.camera, now, ticks, continuous, 1);
        else atv.reset();
      } else { bike.reset(); atv.reset(); }
      delivered = frame;
      continue;
    }
    const s = event.sample;
    let p = bike.sample(delivered.frame, delivered.camera, s.now, s.active);
    if (!p.interpolated && log.interpolation) p = atv.sample(delivered.frame, delivered.camera, s.now, s.active);
    samples++;
    if (p.interpolated !== s.interpolated) { mismatched++; continue; }
    if (!p.interpolated) continue;
    interpolated++;
    alphas.push(+p.alpha.toFixed(3));
    const f = p.frame;
    worst.origin = Math.max(worst.origin, d(f.origin, s.origin));
    worst.orientation = Math.max(worst.orientation, d(f.orientation, s.orientation));
    worst.camera = Math.max(worst.camera, d(p.camera.position, s.camera[0]), d(p.camera.target, s.camera[1]));
    if (s.quad) worst.matrix = Math.max(worst.matrix, d(f.quad_node_matrices[0], s.quad));
    if (s.rider) worst.matrix = Math.max(worst.matrix, d(f.rider_node_matrices[0], s.rider));
    if (s.rider_world && f.rider_world_matrix) worst.matrix = Math.max(worst.matrix, d(f.rider_world_matrix, s.rider_world));
    const cpu = f.race?.opponents?.[0];
    if (cpu && s.cpu) worst.cpu = Math.max(worst.cpu, d(cpu.origin, s.cpu[0]), cpu.physical && s.cpu[1] ? d(cpu.physical.orientation, s.cpu[1]) : 0);
  }
  return { samples, interpolated, mismatched, worst, alphaPattern: alphas.slice(10, 22) };
}

