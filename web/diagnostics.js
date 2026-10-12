// Optional browser instrumentation. Values come from the same live snapshot
// used by gameplay verification; this module never changes simulation state.
export const DIAGNOSTICS_KEY = 'atv.diagnostics.v1';
export const MODES = ['off', 'performance', 'debug'];
const number = (value, places = 1) => Number.isFinite(value) ? value.toFixed(places) : '—';
const vector = value => value?.map(v => number(v, 2)).join(' / ') ?? '—';

export class FrameStats {
  constructor() { this.previous = null; this.intervals = []; this.costs = []; }
  sample(time, cpuMs, active) {
    if (!active) { this.previous = null; return; }
    if (this.previous !== null) {
      const interval = time - this.previous;
      if (interval > 0 && interval < 1000) {
        this.intervals.push(interval);
        if (this.intervals.length > 120) this.intervals.shift();
      }
    }
    this.previous = time;
    if (Number.isFinite(cpuMs) && cpuMs >= 0) {
      this.costs.push(cpuMs);
      if (this.costs.length > 120) this.costs.shift();
    }
  }
  snapshot() {
    const mean = values => values.length ? values.reduce((a, b) => a + b, 0) / values.length : null;
    const frameMs = mean(this.intervals), sorted = [...this.intervals].sort((a, b) => a - b);
    return { fps: frameMs ? 1000 / frameMs : null, frameMs,
      p95Ms: sorted.length ? sorted[Math.ceil(sorted.length * .95) - 1] : null,
      renderCpuMs: mean(this.costs), samples: this.intervals.length };
  }
}

export class DiagnosticsOverlay {
  constructor({ snapshot, parent = document.body }) {
    this.read = snapshot; this.mode = 'off'; this.stats = new FrameStats(); this.updated = -Infinity;
    try { const saved = localStorage.getItem(DIAGNOSTICS_KEY); if (MODES.includes(saved)) this.mode = saved; } catch { /* Private storage may be unavailable. */ }
    this.element = document.createElement('pre'); this.element.id = 'diagnostics-overlay';
    this.element.setAttribute('aria-label', 'Performance and gameplay diagnostics');
    parent.append(this.element); this.sync();
    window.addEventListener('keydown', event => {
      if (event.code !== 'F3' || event.repeat || event.defaultPrevented || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey
          || event.target?.closest?.('input, textarea, [contenteditable=true]')) return;
      event.preventDefault(); event.stopImmediatePropagation(); this.cycle();
    });
  }
  get label() { return ({ off: 'Off', performance: 'Performance', debug: 'Debug' })[this.mode]; }
  cycle() {
    this.mode = MODES[(MODES.indexOf(this.mode) + 1) % MODES.length];
    try { localStorage.setItem(DIAGNOSTICS_KEY, this.mode); } catch { /* In-memory control still works. */ }
    this.updated = -Infinity; this.sync(); this.present(performance.now());
  }
  sync() { this.element.hidden = this.mode === 'off'; this.element.dataset.mode = this.mode; }
  sample(time, cpuMs, active) {
    // Disabled instrumentation does not collect/sort timing history.
    if (this.mode === 'off') { this.stats.previous = null; return; }
    this.stats.sample(time, cpuMs, active); this.present(time);
  }
  snapshot() { return { mode: this.mode, ...this.stats.snapshot() }; }
  present(time) {
    if (this.mode === 'off' || time - this.updated < 250) return;
    this.updated = time;
    const s = this.read(), f = s.frame, timing = this.stats.snapshot();
    const state = s.paused ? 'PAUSED' : !s.focused ? 'UNFOCUSED' : s.ready ? 'LIVE' : 'LOADING';
    const lines = [
      `${this.mode === 'debug' ? 'DEBUG' : 'PERFORMANCE'} · ${state} · F3`,
      `FPS ${number(timing.fps)}  Frame ${number(timing.frameMs)} ms`,
      `Frame p95 ${number(timing.p95Ms)} ms  CPU draw ${number(timing.renderCpuMs)} ms`,
      `WASM batch ${number(s.physicsBatchMs, 2)} ms / ${s.physicsBatchSteps ?? 0} steps`,
      `Worker round trip ${number(s.physicsRoundTripMs, 2)} ms`,
      `Draws ${s.drawCalls ?? 0}  Triangles ${Math.round(s.drawTriangles ?? 0).toLocaleString('en-US')}`,
      `Resolution ${s.resolution?.join(' × ') ?? '—'}  WASM ${number(s.wasmBytes / 1048576)} MiB`,
    ];
    if (this.mode === 'debug') lines.push(
      `Map ${s.source ?? '—'}  Step ${s.simulationSteps ?? 0}`,
      `Position ft ${vector(f?.origin)}`,
      `Velocity ft/s ${vector(f?.linear_velocity)}`,
      `Contact FL FR RL RR ${f?.grounded?.map(v => v ? '1' : '0').join(' ') ?? '—'}`,
      `Suspension ft ${vector(f?.wheel_compression)}`,
      `Stops ${f?.suspension_stop_contacts ?? 0}  Body ${f?.chassis_contact_count ?? 0}  Props ${f?.obstacle_contact_count ?? 0}`,
      `Scene cache ${f?.scene_query?.enabled ? f.scene_query.cached_nodes : 'off'}  Props ${f?.scene_query?.selected_static ?? 0}  Plants ${f?.scene_query?.selected_vegetation ?? 0}`,
      `Wreck ${f?.wreck?.phase ?? '—'}  Resets ${f?.reset_count ?? 0}`,
      `Boundary ${f?.boundary_launch?.active ? 'launch' : 'ready'} ${number(f?.boundary_launch?.elapsed ?? 0, 2)} s`,
      `Water ft ${s.water ? number(f?.water_height, 2) : 'none'}  Rider contacts ${f?.rider_wreck?.water_contact_count ?? 0}`,
      `Input T ${number(s.input?.throttle, 2)} B ${number(s.input?.brake, 2)} S ${number(s.input?.steer, 2)} L ${number(s.input?.lean, 2)}`,
      `Reverse ${s.input?.reverse ? 'on' : 'off'}  Rider ${s.riderMode ?? '—'}`,
      `Vegetation ${s.vegetationVisible ?? 0}/${s.vegetationPlacements ?? 0}  Batches ${s.vegetationDraws ?? 0}`,
      `Terrain updates ${s.terrainDetailUpdates ?? 0}  GL ${s.glError ?? 0}`,
    );
    if (this.mode === 'debug' && f?.rider_wreck) {
      const query = f.rider_wreck.scene_query;
      lines.push(`Rider cache ${query?.cached ? query.cached_nodes : 'off'}  Props ${query?.selected_static ?? 0}  Proxy gaps ${query?.unsupported_vegetation_proxies ?? 0}`);
    }
    this.element.textContent = lines.join('\n');
  }
}
