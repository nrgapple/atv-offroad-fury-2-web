// Display → Graphics presets and the authored presentation layers they enable.
// Original is the recovered renderer exactly; Enhanced and High only add
// presentation. None of these settings reaches the Rust simulation.
export const GRAPHICS_PRESETS = ['auto', 'original', 'enhanced', 'high'];

// What each resolved preset turns on. `anisotropy` is the requested maximum;
// the renderer clamps it to the device limit and 0 never touches the sampler.
export function presetFeatures(preset) {
  if (preset === 'high') return { preset, interpolation: true, anisotropy: 16, contactShadows: true, dirt: true, wheelReprojection: true, tireTracks: { capacity: 3600, life: 75 }, shadowMap: 2048, fresnel: true, dust: 96, skySmooth: true };
  if (preset === 'enhanced') return { preset, interpolation: true, anisotropy: 4, contactShadows: true, dirt: true, wheelReprojection: true, tireTracks: { capacity: 600, life: 15 }, shadowMap: 0, fresnel: true, dust: 24, skySmooth: true };
  return { preset: 'original', interpolation: false, anisotropy: 0, contactShadows: false, dirt: false, wheelReprojection: false, tireTracks: null, shadowMap: 0, fresnel: false, dust: 0, skySmooth: false };
}

// Auto picks Enhanced on phones, tablets, touch-primary and low-power devices
// and High on desktops. Every probe is optional; missing APIs count as desktop.
export function detectGraphicsTier(env = globalThis) {
  const nav = env.navigator ?? {};
  const media = query => { try { return Boolean(env.matchMedia?.(query).matches); } catch { return false; } };
  const ua = String(nav.userAgent ?? '');
  const mobileUa = nav.userAgentData?.mobile === true || /Android|iPhone|iPad|iPod|Mobile|Silk|Kindle/i.test(ua);
  // iPadOS reports a desktop Macintosh user agent with multi-touch.
  const touchMac = /Macintosh/.test(ua) && (nav.maxTouchPoints ?? 0) > 1;
  const touchPrimary = media('(pointer: coarse)') && !media('(any-pointer: fine)');
  const lowPower = (Number.isFinite(nav.hardwareConcurrency) && nav.hardwareConcurrency > 0 && nav.hardwareConcurrency <= 4)
    || (Number.isFinite(nav.deviceMemory) && nav.deviceMemory > 0 && nav.deviceMemory <= 4) || nav.connection?.saveData === true;
  return mobileUa || touchMac || touchPrimary || lowPower ? 'enhanced' : 'high';
}

export class GraphicsSettings {
  constructor(storage, env = globalThis) {
    if (storage === undefined) {
      try { storage = globalThis.localStorage; } catch { storage = null; }
    }
    this.storage = storage; this.detected = detectGraphicsTier(env); this.preset = 'auto';
    try { const saved = storage?.getItem('atv-graphics'); if (GRAPHICS_PRESETS.includes(saved)) this.preset = saved; } catch { /* Session settings. */ }
  }
  get resolved() { return this.preset === 'auto' ? this.detected : this.preset; }
  get features() { return presetFeatures(this.resolved); }
  get label() { const name = p => p[0].toUpperCase() + p.slice(1); return this.preset === 'auto' ? `Auto (${name(this.detected)})` : name(this.preset); }
  cycle(direction = 1) {
    const at = GRAPHICS_PRESETS.indexOf(this.preset), n = GRAPHICS_PRESETS.length;
    this.preset = GRAPHICS_PRESETS[(at + (direction < 0 ? n - 1 : 1)) % n];
    try { this.storage?.setItem('atv-graphics', this.preset); } catch { /* Session settings. */ }
    this.onChange?.();
  }
}

// Controls → Vibration. The original offered a vibration option; this is an
// authored browser adaptation (controller dual rumble, Android vibration).
export class VibrationSettings {
  constructor(storage) {
    if (storage === undefined) {
      try { storage = globalThis.localStorage; } catch { storage = null; }
    }
    this.storage = storage; this.enabled = true;
    try { this.enabled = storage?.getItem('atv-vibration') !== 'off'; } catch { /* Session settings. */ }
  }
  toggle() {
    this.enabled = !this.enabled;
    try { this.storage?.setItem('atv-vibration', this.enabled ? 'on' : 'off'); } catch { /* Session settings. */ }
    this.onChange?.();
  }
}

// ImpactEvents severities (contact speed or landing sink rate, ft/s) to a
// 0..1 rumble strength. Gentle hops stay silent.
export function rumbleStrength(severity) {
  if (!(severity >= 18)) return 0;
  return Math.min(1, .2 + (severity - 18) / 55);
}

// Pulses the active controller (Gamepad vibrationActuator dual-rumble) and,
// on touch devices that support it, navigator.vibrate. iOS has no vibrate.
export class Rumble {
  constructor({ pad = () => null, vibrate = pattern => globalThis.navigator?.vibrate?.(pattern), touch = () => false, now = () => performance.now() } = {}) {
    Object.assign(this, { pad, vibrate, touch, now, until: 0, strength: 0, pulses: 0, last: null });
  }
  pulse(strength) {
    if (!(strength > 0)) return false;
    const now = this.now();
    // Bouncing inside one impact cannot chain pulses; a stronger hit can.
    if (now < this.until && strength <= this.strength + .15) return false;
    const duration = Math.round(70 + 230 * strength);
    this.until = now + duration; this.strength = strength;
    let sent = false;
    const actuator = this.pad()?.vibrationActuator;
    if (actuator?.playEffect) {
      try { void actuator.playEffect('dual-rumble', { startDelay: 0, duration, strongMagnitude: strength, weakMagnitude: Math.min(1, .25 + strength * .75) })?.catch?.(() => {}); sent = true; } catch { /* Unsupported effect. */ }
    }
    if (!sent && this.touch() && strength >= .3) { try { sent = Boolean(this.vibrate(duration)); } catch { /* Not allowed. */ } }
    if (sent) { this.pulses++; this.last = { strength, duration }; }
    return sent;
  }
  // One simulation frame: an ImpactEvents severity and the wreck transition.
  accept(previous, frame, severity, enabled) {
    if (!enabled || !frame) return false;
    const wrecked = previous && previous.reset_count === frame.reset_count && previous.wreck?.phase === 'Riding' && frame.wreck?.phase && frame.wreck.phase !== 'Riding';
    const strength = Math.max(rumbleStrength(severity), wrecked ? Math.max(.7, rumbleStrength(frame.crash_impact_speed ?? 0)) : 0);
    return this.pulse(strength);
  }
}

// Original ATV2 terrain zones carry DirtAnimChangeTime/DirtAnimChangeAmount.
// The source TOY documents them as: above 10 mph, every ChangeTime seconds the
// vehicle gains (or, if negative, loses) ChangeAmount of 255 dirt steps. Zones
// without them change nothing. The native consumer is not instruction-verified.
export function parseDirtZones(text) {
  const zones = Array.from({ length: 8 }, () => ({ time: 0, amount: 0, tireTrack: false, dust: false }));
  let zone = -1;
  for (const raw of String(text ?? '').split(/\r?\n/)) {
    const line = raw.split(';')[0].trim();
    const section = /^\[\s*TerrainZone(\d+)\s*\]$/i.exec(line);
    if (line.startsWith('[')) { zone = section && +section[1] >= 1 && +section[1] <= 8 ? +section[1] - 1 : -1; continue; }
    const flag = zone >= 0 && /^(GenerateTireTrack|GenerateDust)\s*=\s*([TF])/i.exec(line);
    if (flag) { zones[zone][flag[1].toLowerCase() === 'generatetiretrack' ? 'tireTrack' : 'dust'] = flag[2].toUpperCase() === 'T'; continue; }
    const field = zone >= 0 && /^(DirtAnimChangeTime|DirtAnimChangeAmount)\s*=\s*(-?[\d.]+)\s*$/i.exec(line);
    if (!field) continue;
    const value = Number(field[2]);
    if (!Number.isFinite(value)) continue;
    if (field[1].toLowerCase() === 'dirtanimchangetime') zones[zone].time = Math.max(0, value);
    else zones[zone].amount = value;
  }
  return zones;
}

const MPH = 5280 / 3600, STEP = 1 / 60;
// Authored adaptation around the source zone rule: one shared level for the
// quad and rider, original dirt overlays, water washing and reset clearing.
export class DirtBuildup {
  constructor(zones = parseDirtZones('')) { this.zones = this.sourceZones = zones; this.rate = 1; this.reset(); }
  // A player Reset (or a new ride) cleans the quad; wreck recovery keeps it.
  reset() { this.level = 0; this.timer = 0; this.zone = -1; }
  get amount() { return this.level / 255; }
  // Alpha Heavy mud: faster soiling, dusty zones soil too, landings splat.
  setHeavy(heavy, zones) { this.heavy = heavy; this.rate = heavy ? 3 : 1; this.zones = heavy ? zones(this.sourceZones) : this.sourceZones; }
  splat(steps) { this.level = Math.max(0, Math.min(255, this.level + steps)); }
  // Exactly one call per accepted 60 Hz simulation frame.
  step(frame, dt = STEP) {
    if (!frame) return this.amount;
    const effects = frame.wheel_effects, grounded = frame.grounded ?? [];
    const submerged = effects ? effects.centers.filter((c, i) => effects.water_heights[i] > c[1] - effects.radii[i] * .5).length : 0;
    if (submerged) {
      // Authored wash: a fully submerged quad comes clean in about four seconds.
      this.level = Math.max(0, this.level - 255 * .25 * dt * submerged / 4); this.timer = 0;
      return this.amount;
    }
    const speed = Math.abs(frame.speed_magnitude ?? frame.speed ?? 0);
    const wheels = grounded.map((g, i) => g ? effects?.surfaces?.[i] : undefined).filter(z => Number.isInteger(z) && z >= 0 && z < 8);
    if (speed <= 10 * MPH || !wheels.length) return this.amount;
    // The zone under most grounded wheels; MUD scenery counts as heavy dirt.
    const counts = new Map(); for (const z of wheels) counts.set(z, (counts.get(z) ?? 0) + 1);
    const zone = [...counts].sort((a, b) => b[1] - a[1] || a[0] - b[0])[0][0];
    const mud = effects?.mud?.some((m, i) => m && grounded[i]);
    let { time, amount } = this.zones[zone] ?? { time: 0, amount: 0 };
    if (mud) { time = time > 0 ? Math.min(time, .1) : .1; amount = Math.max(amount, 1); }
    if (zone !== this.zone) { this.zone = zone; this.timer = 0; }
    if (!(time > 0) || !amount) return this.amount;
    this.timer += dt * this.rate;
    while (this.timer >= time - 1e-6) { this.timer -= time; this.level = Math.max(0, Math.min(255, this.level + amount)); }
    return this.amount;
  }
}

// Enhanced/High visual wheel re-projection. The recovered order draws wheels
// from the compression stored before integration (one frame stale), so landing
// tires can sit about 0.2 ft inside the visible ground. This returns raised
// presentation compressions for wheels whose bottom is below the visible
// terrain at their centre, or null when nothing changes. The renderer clamps
// each value to the quad's suspension travel; the simulation never sees it.
export const MAX_WHEEL_LIFT = .5;
export function wheelReprojection(frame, height) {
  const effects = frame?.wheel_effects, compression = frame?.wheel_compression;
  if (!effects || !Array.isArray(compression) || compression.length !== 4 || frame.wreck?.phase !== 'Riding' || frame.funny_crash) return null;
  const [, x, , z] = frame.orientation ?? [1, 0, 0, 0];
  const up = Math.max(.5, 1 - 2 * (x * x + z * z));
  let changed = false;
  const result = compression.map((c, i) => {
    const center = effects.centers?.[i], radius = effects.radii?.[i];
    if (!center || !Number.isFinite(radius)) return c;
    const ground = height(center[0], center[2]), depth = ground - (center[1] - radius);
    if (!(depth > .01)) return c;
    changed = true;
    return c + Math.min(depth / up, MAX_WHEEL_LIFT);
  });
  return changed ? result : null;
}
