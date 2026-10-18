// Alpha "hype" presentation (Settings → Alpha). Authored, not original: it
// reads delivered 60 Hz frames and shapes time, camera, sound and light around
// big air. It never feeds anything back into the recovered vehicle solver;
// slow motion only delays when the next fixed 1/60 s step is requested.
const STEP = 1 / 60, MPH = 5280 / 3600;
const clamp01 = v => Math.max(0, Math.min(1, v));
const smooth = (edge0, edge1, v) => { const t = clamp01((v - edge0) / (edge1 - edge0)); return t * t * (3 - 2 * t); };
const approach = (value, target, dt, tc) => value + (target - value) * (1 - Math.exp(-dt / tc));

// A big jump needs a real launch: the quad must climb RISE ft above its
// takeoff height and reach BIG_AIR ft above the terrain (FULL_AIR saturates).
// Rolling off a hillside never climbs, however far the ground falls away.
export const BIG_AIR = 12, FULL_AIR = 30, RISE = [4, 10], APEX_MIN = 14, APEX_RISE = 8, MIN_AIR_TIME = .45;
// Slow motion (real seconds): ease in, hold, ease out to full speed.
export const SLOWMO = { scale: .3, easeIn: .12, hold: .55, easeOut: .3 };

export class HypeDirector {
  constructor() { this.reset(); }
  reset() {
    this.previous = null; this.airTime = 0; this.clearance = 0; this.peak = 0; this.speed = 0; this.takeoff = 0; this.rise = 0;
    this.apexFired = false; this.apexAge = Infinity; this.apexes = 0; this.landings = 0; this.landing = null;
    this.riding = false; this.grounded = true;
    this.air = 0; this.blur = 0; this.muffle = 0; this.wind = 0; this.slam = 0; this.dip = 0; this.dipVelocity = 0;
    this.slow = null; this.timeScale = 1; this.slowmos = 0;
    this.seenApexes = 0; this.seenLandings = 0;
  }
  // Exactly one call per accepted 60 Hz simulation frame.
  step(frame, height) {
    const old = this.previous; this.previous = frame;
    if (!frame?.origin || !old || old.reset_count !== frame.reset_count || Math.hypot(...frame.origin.map((v, i) => v - old.origin[i])) > 40) {
      this.airTime = 0; this.peak = 0; this.rise = 0; this.apexFired = false; this.apexAge = Infinity; this.grounded = true; this.riding = false; return;
    }
    this.riding = frame.wreck?.phase === 'Riding';
    this.grounded = Boolean(frame.grounded?.some(Boolean));
    const ground = height?.(frame.origin[0], frame.origin[2]);
    this.clearance = Number.isFinite(ground) ? frame.origin[1] - ground : 0;
    this.speed = Math.abs(frame.speed_magnitude ?? 0);
    const vy = frame.linear_velocity?.[1] ?? 0, oldVy = old.linear_velocity?.[1] ?? 0;
    if (this.grounded) {
      if (!old.grounded?.some(Boolean) && this.airTime >= .3 && this.riding) {
        this.landing = { severity: Math.max(0, -oldVy), airTime: this.airTime, peak: this.peak, position: [...frame.origin] };
        this.landings++;
      }
      this.airTime = 0; this.peak = 0; this.rise = 0; this.apexFired = false; this.apexAge = Infinity;
      return;
    }
    if (!this.airTime) this.takeoff = old.origin[1];
    this.airTime += STEP; this.peak = Math.max(this.peak, this.clearance); this.rise = Math.max(this.rise, frame.origin[1] - this.takeoff);
    if (this.apexFired) this.apexAge += STEP;
    if (!this.apexFired && this.riding && oldVy > 0 && vy <= 0 && this.clearance >= APEX_MIN && this.rise >= APEX_RISE && this.airTime > .25) {
      this.apexFired = true; this.apexAge = 0; this.apexes++;
    }
  }
  // Big-air amount (0..1) for the frame being shown: the highest point of the
  // jump so far, held until touchdown so the framing never sags mid-fall.
  get airTarget() { return this.big; }
  get big() { return this.riding && !this.grounded && this.airTime >= MIN_AIR_TIME ? smooth(BIG_AIR, FULL_AIR, this.peak) * smooth(RISE[0], RISE[1], this.rise) : 0; }
  // Once per rendered frame with real seconds. Options select enabled effects.
  update(dt, { slowmo = false } = {}) {
    dt = Math.max(0, Math.min(.1, dt));
    const apex = this.apexes !== this.seenApexes, landed = this.landings !== this.seenLandings;
    this.seenApexes = this.apexes; this.seenLandings = this.landings;
    // Slow motion: one per jump, at its apex; touchdown or a wreck ends it early.
    if (apex && slowmo && !this.slow) { this.slow = { t: 0 }; this.slowmos++; }
    if (this.slow) {
      const s = this.slow, { scale, easeIn, hold, easeOut } = SLOWMO;
      if (!slowmo || landed || !this.riding || (this.clearance < 4 && s.t < easeIn + hold)) s.t = Math.max(s.t, easeIn + hold);
      s.t += dt;
      const out = s.t - easeIn - hold;
      this.timeScale = s.t < easeIn ? 1 - (1 - scale) * smooth(0, easeIn, s.t)
        : out < 0 ? scale : scale + (1 - scale) * smooth(0, easeOut, out);
      if (out >= easeOut) { this.slow = null; this.timeScale = 1; }
    } else this.timeScale = 1;
    const target = this.airTarget;
    this.air = approach(this.air, target, dt, target > this.air ? .4 : .22);
    // Dark edge blur builds through big air, peaks around the apex and clears
    // quickly at touchdown.
    const big = this.big;
    const blurTarget = big * (.75 + .25 * (this.apexAge < .9 ? 1 : 0));
    this.blur = approach(this.blur, blurTarget, dt, blurTarget > this.blur ? .3 : .12);
    // Landing: camera compression spring and the music slamming back in.
    if (landed && this.landing) {
      const hit = Math.min(70, this.landing.severity) * (.4 + .6 * smooth(2, FULL_AIR, this.landing.peak));
      this.dipVelocity -= hit * .42;
      this.slam = .35;
    }
    const k = 130, c = 15;
    this.dipVelocity += (-k * this.dip - c * this.dipVelocity) * dt; this.dip += this.dipVelocity * dt;
    this.dip = Math.max(-3, Math.min(1.5, this.dip));
    this.slam = Math.max(0, this.slam - dt);
    const fast = .5 * smooth(40 * MPH, 72 * MPH, this.speed);
    const muffleTarget = this.slam > 0 ? 0 : Math.max(fast, .85 * this.air, this.slow ? 1 - this.timeScale : 0);
    this.muffle = approach(this.muffle, muffleTarget, dt, this.slam > 0 ? .03 : muffleTarget > this.muffle ? .3 : .45);
    // Wind rush rises with speed and big air; it cuts with the landing slam.
    const windTarget = this.slam > 0 ? 0 : Math.max(2 * fast, this.air);
    this.wind = approach(this.wind, windTarget, dt, this.slam > 0 ? .05 : .35);
    return { landed: landed ? this.landing : null, apex };
  }
  // Big-air chase framing: close in behind the rider so the flight feels
  // personal, widen slightly; landings compress the camera and spring back.
  camera(frame, height) {
    const a = this.air, dip = this.dip;
    if (a < .001 && Math.abs(dip) < .001) return frame;
    // Close in towards the aim point; a steep falling view also flattens out.
    const offset = frame.target.map((v, i) => v - frame.position[i]), pull = 1 - .4 * a;
    const position = [frame.target[0] - offset[0] * pull, frame.target[1] - offset[1] * (1 - .6 * a), frame.target[2] - offset[2] * pull];
    const floor = height?.(position[0], position[2]);
    if (Number.isFinite(floor)) position[1] = Math.max(position[1], floor + 2);
    const target = [...frame.target];
    position[1] += dip; target[1] += dip * .55;
    return { ...frame, position, target, fov: frame.fov * (1 + .1 * a) };
  }
  snapshot() {
    return { airTime: this.airTime, clearance: this.clearance, peak: this.peak, air: this.air, blur: this.blur, muffle: this.muffle, wind: this.wind,
      rise: this.rise, dip: this.dip, timeScale: this.timeScale, slowmos: this.slowmos, apexes: this.apexes, landings: this.landings, landing: this.landing };
  }
}

// Heavy mud: the original zone dirt rule runs four times faster, dusty zones
// without a dirt rule still soil the quad, and hard landings splat extra mud.
export function heavyMudZones(zones) {
  return zones.map(z => z.time > 0 && z.amount ? { ...z, time: z.time / 4, amount: z.amount * (z.amount > 0 ? 1 : 2) }
    : z.dust ? { ...z, time: .2, amount: 1 } : z);
}
