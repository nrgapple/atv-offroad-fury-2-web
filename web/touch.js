// Browser gestures adapt input axes only; the recovered Rust gates own jump force.
const clamp = value => Math.max(-1, Math.min(1, Number.isFinite(value) ? value : 0));
export const DEFAULT_STEERING_SENSITIVITY = 85;
export const normalizeSteeringSensitivity = value => Math.max(50, Math.min(150, Math.round((Number.isFinite(value) ? value : DEFAULT_STEERING_SENSITIVITY) / 5) * 5));
export function loadSteeringSensitivity(read = () => localStorage.getItem('atv-mobile-steering-sensitivity')) {
  try {
    const saved = read(), value = Number(saved);
    return saved !== null && saved !== '' && Number.isFinite(value) && value >= 50 && value <= 150 ? normalizeSteeringSensitivity(value) : DEFAULT_STEERING_SENSITIVITY;
  } catch { return DEFAULT_STEERING_SENSITIVITY; }
}
// Scale displacement BEFORE clipping so a larger drag can still reach full lock.
const steeringAxis = (dx, sensitivity) => clamp(dx / 70 * sensitivity / 100);
// Screen angles (y down). The eight inner sectors sit where the standard
// two-finger swipes point; the circle stunts sit in the outer band above,
// Cat Nac to the clockwise side. These reuse the ten existing touch stunts.
const deg = d => d * Math.PI / 180;
export const TRICK_WHEEL = [
  { name: 'Nac Nac', label: 'Nac Nac', modifiers: 1, axes: [0, -1], angle: deg(-90), span: deg(22.5) },
  { name: 'Point Back', label: 'Point Back', modifiers: 2, axes: [1, 0], angle: deg(-45), span: deg(22.5) },
  { name: 'Bar Hop', label: 'Bar Hop', modifiers: 1, axes: [1, 0], angle: 0, span: deg(22.5) },
  { name: 'Cliff Hanger', label: 'Cliff Hanger', modifiers: 2, axes: [0, 1], angle: deg(45), span: deg(22.5) },
  { name: 'No Footed Can Can', label: 'Can Can', modifiers: 1, axes: [0, 1], angle: deg(90), span: deg(22.5) },
  { name: 'Nothing', label: 'Nothing', modifiers: 2, axes: [-1, 0], angle: deg(135), span: deg(22.5) },
  { name: 'Suicide No Hander', label: 'No Hander', modifiers: 1, axes: [-1, 0], angle: deg(180), span: deg(22.5) },
  { name: 'Heel Clicker', label: 'Heel Clicker', modifiers: 2, axes: [0, -1], angle: deg(-135), span: deg(22.5) },
  { name: 'Cat Nac', label: 'Cat Nac', modifiers: 3, axes: [0, -1], angle: deg(-60), span: deg(22.5), outer: true },
  { name: 'Cordova', label: 'Cordova', modifiers: 3, axes: [0, 1], angle: deg(-120), span: deg(22.5), outer: true },
];
// The outer band starts here; its gaps (including straight up) fall
// through to the inner sector in that direction.
export const OUTER_DISTANCE = 132;
const INNER_TRICKS = TRICK_WHEEL.filter(t => !t.outer), OUTER_TRICKS = TRICK_WHEEL.filter(t => t.outer);
const sectorAt = (choices, angle) => choices.find(c => Math.abs(Math.atan2(Math.sin(angle - c.angle), Math.cos(angle - c.angle))) <= c.span) ?? null;
export function trickWheelSelection(dx, dy) {
  if (!Number.isFinite(dx) || !Number.isFinite(dy) || Math.hypot(dx, dy) < 28) return null;
  const angle = Math.atan2(dy, dx);
  return (Math.hypot(dx, dy) >= OUTER_DISTANCE && sectorAt(OUTER_TRICKS, angle)) || sectorAt(INNER_TRICKS, angle);
}
// Optional Extras flips come from the driving thumb. Pull it past the driving
// range, hold a beat, and lift: on the ramp the lift is also the jump, and in
// the first moments of air it still counts. Rust owns the timing window.
// Down matches leaning back; left/right spin that way. No front flip exists.
export const FLIPS = [
  { name: 'Backflip', flip: 'backflip', cue: 'LIFT → BACKFLIP' },
  { name: '360 Left', flip: 'left', cue: 'LIFT → 360 LEFT' },
  { name: '360 Right', flip: 'right', cue: 'LIFT → 360 RIGHT' },
];
// The flip zone lies beyond where lean (70px) and steering (full lock) already
// saturate, so ordinary hard steering or leaning never reaches it. It needs a
// near-straight pull (±30°) and FLIP_DWELL ms inside before a lift counts;
// sliding into it while lifting does not. A thumb that rolls back out in the
// last FLIP_GRACE ms before lifting still keeps the armed flip.
export const FLIP_REACH = 105, FLIP_DWELL = 220, FLIP_GRACE = 100;
const FLIP_CONE = Math.PI / 6;
export function flipZone(dx, dy, sensitivity = 100) {
  if (!Number.isFinite(dx) || !Number.isFinite(dy)) return null;
  const distance = Math.hypot(dx, dy), angle = Math.atan2(dy, dx);
  const toward = target => Math.abs(Math.atan2(Math.sin(angle - target), Math.cos(angle - target))) <= FLIP_CONE;
  if (distance >= FLIP_REACH && toward(Math.PI / 2)) return FLIPS[0];
  const fullLock = 70 * 100 / Math.max(1, sensitivity);
  if (distance < Math.max(FLIP_REACH, fullLock * 1.3)) return null;
  return toward(Math.PI) ? FLIPS[1] : toward(0) ? FLIPS[2] : null;
}
// Held pull time in source stick-hold frames (60 = one second = full strength).
const holdFrames = (since, now) => Math.max(0, Math.min(60, (now - since) * .06));
// Pointer flip bookkeeping shared by the one-handed thumb and the two-finger pad.
const trackFlip = (pointer, flip, now) => {
  if (flip === pointer.flip) return;
  if (pointer.flip && now - pointer.flipSince >= FLIP_DWELL) pointer.lastArmed = { flip: pointer.flip, since: pointer.flipSince, at: now };
  pointer.flip = flip; pointer.flipSince = now;
};
export const flipArmed = (pointer, now = performance.now()) => Boolean(pointer?.flip && now - pointer.flipSince >= FLIP_DWELL);
const flipLaunch = (pointer, now) => {
  if (flipArmed(pointer, now)) return { flip: pointer.flip.flip, hold: holdFrames(pointer.flipSince, now) };
  const last = pointer?.lastArmed;
  return last && now - last.at <= FLIP_GRACE ? { flip: last.flip.flip, hold: holdFrames(last.since, last.at) } : null;
};
// Two-finger mode: a long straight swipe down/left/right in the air flips.
export const FLIP_SWIPE = 120;
// Gesture mappings are browser adaptations of original modifier/direction inputs.
export class TouchInput {
  constructor({ radial = false } = {}) { this.radial = radial; this.generation = 0; this.airborne = false; this.oneHanded = false; this.flips = false; this.steeringSensitivity = 100; this.clear(); }
  clear() { this.generation++; this.pointer = null; this.secondary = null; this.pendingSelection = null; this.pendingFlip = null; this.sampled = false; this.releasePending = false; }
  owns(id) { return this.pointer?.id === id || this.secondary?.owns(id) === true; }
  begin(id, x = 0, y = 0, airborne = this.airborne) {
    if (this.pointer) {
      if (!this.oneHanded || this.owns(id) || this.secondary?.pointer) return false;
      this.secondary = new TouchInput({ radial: true }); this.secondary.flips = this.flips;
      return this.secondary.begin(id, x, y, airborne);
    }
    this.secondary = null;
    this.airborne = airborne;
    this.pointer = { id, x, y, currentX: x, currentY: y, axis: 1,
      mode: airborne && !this.oneHanded ? (this.radial ? 'radial' : 'trick') : 'drive', points: [[x,y]], lastMove: Infinity, selection: null };
    this.sampled = false; this.releasePending = false;
    return true;
  }
  setOneHanded(enabled) {
    if (this.oneHanded === Boolean(enabled)) return;
    this.oneHanded = Boolean(enabled); this.clear();
  }
  setAirborne(airborne) {
    if (airborne === this.airborne) return;
    this.airborne = airborne;
    // One-handed controls do not change at takeoff or landing.
    if (this.oneHanded) { this.secondary?.setAirborne(airborne); return; }
    const pointer = this.pointer;
    if (pointer) {
      pointer.mode = airborne ? (this.radial ? 'radial' : 'trick') : 'drive';
      pointer.x = pointer.currentX; pointer.y = pointer.currentY;
      if (!airborne) pointer.axis = 1;
      pointer.points = [[pointer.x, pointer.y]]; pointer.selection = null; pointer.lastMove = Infinity;
    }
    // Cancel remembered brace without manufacturing a preload release at takeoff.
    this.sampled = false; this.releasePending = false; this.pendingSelection = null; this.generation++;
  }
  move(id, x, y, now = performance.now()) {
    if (this.secondary?.owns(id)) { this.secondary.move(id, x, y, now); return; }
    const pointer = this.pointer;
    if (pointer?.id !== id) return;
    pointer.currentX = x; pointer.currentY = y;
    if (pointer.mode === 'radial') {
      // Preview is reversible. No modifier is submitted until pointer release.
      const [startX, startY] = pointer.points[0];
      pointer.selection = Math.hypot(x - startX, y - startY) < 8 ? null : trickWheelSelection(x - pointer.x, y - pointer.y);
      return;
    }
    if (pointer.mode === 'drive') {
      if (this.oneHanded) {
        trackFlip(pointer, flipZone(x - pointer.x, y - pointer.y, this.steeringSensitivity), now);
      }
      const back = y - pointer.y;
      pointer.axis = this.oneHanded ? 1 : back > 8 ? -clamp((back - 8) / 62) : 1;
      return;
    }
    if (pointer.mode !== 'trick' || pointer.selection) return;
    const previous = pointer.points.at(-1);
    if (Math.hypot(x-previous[0], y-previous[1]) < 3) return;
    pointer.points.push([x,y]); pointer.lastMove = now;
    if (pointer.points.length > 256) pointer.points.splice(1,1);
    // Closed loops need substantial size and a full turn, not a hooked swipe.
    const points = pointer.points;
    if (points.length < 8) return;
    const xs = points.map(p => p[0]), ys = points.map(p => p[1]);
    const width = Math.max(...xs)-Math.min(...xs), height = Math.max(...ys)-Math.min(...ys);
    if (Math.min(width,height) < 44 || Math.max(width,height)/Math.min(width,height) > 2.5) return;
    let turn = 0;
    for (let i=2;i<points.length;i++) {
      const a = Math.atan2(points[i-1][1]-points[i-2][1], points[i-1][0]-points[i-2][0]);
      const b = Math.atan2(points[i][1]-points[i-1][1], points[i][0]-points[i-1][0]);
      turn += Math.atan2(Math.sin(b-a), Math.cos(b-a));
    }
    if (Math.abs(turn) >= 5.2 && Math.hypot(x-pointer.x,y-pointer.y) <= Math.min(width,height)*.45)
      pointer.selection = { modifiers: 3, axes: [0, turn > 0 ? -1 : 1] };
  }
  end(id, cancelled = false, now = performance.now()) {
    if (this.secondary?.owns(id)) { this.secondary.end(id, cancelled); return; }
    if (this.pointer?.id !== id) return;
    if (cancelled) { this.clear(); return; }
    // Lifting the pulled driving thumb launches a flip (Rust checks timing).
    if (this.flips && this.oneHanded) this.pendingFlip = flipLaunch(this.pointer, now);
    if (this.pointer.mode === 'radial') this.pendingSelection = this.pointer.selection;
    if (this.pointer.mode === 'trick') {
      const sent = this.pointer.selectionSent;
      this.sample(performance.now()+120);
      if (!sent) this.pendingSelection = this.pointer.selection;
    }
    this.releasePending = this.sampled && this.pointer.mode === 'drive';
    this.pointer = null; this.secondary = null; this.sampled = false;
  }
  sample(now = performance.now()) {
    const pointer = this.pointer;
    if (pointer?.mode === 'trick' && !pointer.selection && now-pointer.lastMove >= 120) {
      const dx = pointer.currentX-pointer.x, dy = pointer.currentY-pointer.y;
      const distance = Math.hypot(dx,dy);
      const length = pointer.points.slice(1).reduce((sum,p,i) => sum+Math.hypot(p[0]-pointer.points[i][0],p[1]-pointer.points[i][1]),0);
      if (distance >= 36 && distance / length >= .985) {
        const diagonal = Math.min(Math.abs(dx),Math.abs(dy)) > Math.max(Math.abs(dx),Math.abs(dy))*.5;
        const flip = this.flips && !diagonal && distance >= FLIP_SWIPE && !(Math.abs(dy) >= Math.abs(dx) && dy < 0)
          ? (Math.abs(dy) >= Math.abs(dx) ? 'backflip' : dx < 0 ? 'left' : 'right') : null;
        const axes = diagonal ? (dy < 0 ? (dx < 0 ? [0,-1] : [1,0]) : (dx < 0 ? [-1,0] : [0,1]))
          : Math.abs(dy) >= Math.abs(dx) ? [0,Math.sign(dy)] : [Math.sign(dx),0];
        pointer.selection = flip ? FLIPS.find(c => c.flip === flip) : { modifiers: diagonal ? 2 : 1, axes };
      }
    }
    const driving = pointer?.mode === 'drive';
    const leaning = this.oneHanded && pointer && ['drive','lean'].includes(pointer.mode);
    const steer = leaning ? steeringAxis(pointer.currentX-pointer.x, this.steeringSensitivity) : 0;
    const lean = leaning ? clamp((pointer.currentY-pointer.y)/70) : 0;
    const drive = { generation: this.generation, held: driving, axis: driving ? pointer.axis : 0, grounded_only: false };
    if (this.oneHanded) {
      drive.forward_only = true;
      // Rust resolves ground contact on EACH fixed step, including batch landing.
      drive.reverse_held = Boolean(pointer && this.secondary?.pointer);
    }
    let preload_axis = null;
    if (driving) { this.sampled = true; preload_axis = 1; }
    else if (this.releasePending) { this.releasePending = false; preload_axis = -1; }
    const secondary = this.oneHanded && pointer && this.airborne ? this.secondary?.sample(now) : null;
    const selection = secondary?.flip_request ? { flip: secondary.flip_request }
      : secondary?.stunt_modifiers ? { modifiers: secondary.stunt_modifiers, axes: [secondary.stunt_x, secondary.stunt_y] }
      : (pointer?.mode === 'trick' ? pointer.selection : null) ?? this.pendingSelection;
    if (pointer?.mode === 'trick' && pointer.selection) pointer.selectionSent = true;
    this.pendingSelection = null;
    const launch = this.pendingFlip; this.pendingFlip = null;
    return { drive, preload_axis, steer, lean, stunt_modifiers: selection?.modifiers ?? 0,
      stunt_x: selection?.axes?.[0] ?? 0, stunt_y: selection?.axes?.[1] ?? 0,
      flip_request: launch?.flip ?? selection?.flip ?? null, flip_launch: Boolean(launch), flip_hold: launch?.hold ?? 0 };
  }
}

// The left thumb always steers and leans, whether grounded or airborne.
export class TouchLeanIntent {
  constructor(sensitivity = () => 100) { this.sensitivity = sensitivity; this.clear(); }
  clear() { this.pointer = null; }
  begin(id, x, y) {
    if (this.pointer) return false;
    this.pointer = { id, x, y, dx: 0, dy: 0 }; return true;
  }
  move(id, x, y, now = performance.now()) {
    if (this.pointer?.id !== id) return;
    this.pointer.dx = x - this.pointer.x;
    this.pointer.dy = clamp((y - this.pointer.y) / 70);
    trackFlip(this.pointer, flipZone(this.pointer.dx, y - this.pointer.y, this.sensitivity()), now);
  }
  /** Two-finger mode: the left pad's held full pull when the drive thumb lifts. */
  launchFlip(now = performance.now()) { return this.pointer ? flipLaunch(this.pointer, now) : null; }
  end(id) { if (this.pointer?.id === id) this.pointer = null; }
  get steer() { return steeringAxis(this.pointer?.dx ?? 0, this.sensitivity()); }
  get lean() { return this.pointer?.dy ?? 0; }
}

// Only an explicitly saved Off overrides the one-finger default.
export function loadOneHandedPreference(read = () => localStorage.getItem('atv-mobile-one-handed')) {
  try { return read() !== 'false'; } catch { return true; }
}

// Read intent without sampling it or consuming a preload release.
export function oneHandedFeedback(input, frame, now = performance.now()) {
  const pointer = input.pointer;
  const applied = frame?.applied_input;
  const current = applied?.touch_drive?.generation === input.generation && applied.touch_drive.held;
  // Only promise a flip when lifting now would start one.
  const canLaunch = Boolean(input.flips && input.oneHanded && pointer && (frame?.preload_armed || frame?.preload_active || frame?.flip?.launch_window));
  const flip = canLaunch && flipArmed(pointer, now) ? pointer.flip : null;
  return {
    // Compass arming feedback: which flip zone the thumb is in and how far through the dwell.
    flipZone: canLaunch ? pointer.flip?.flip ?? null : null,
    flipProgress: canLaunch && pointer.flip ? Math.min(1, (now - pointer.flipSince) / FLIP_DWELL) : 0,
    flipsReady: canLaunch,
    steer: pointer ? steeringAxis(pointer.currentX - pointer.x, input.steeringSensitivity) : 0,
    lean: pointer ? clamp((pointer.currentY - pointer.y) / 70) : 0,
    cue: !pointer ? '' : flip ? flip.cue : input.secondary?.pointer && !input.airborne ? (current && applied.brake > 0 ? 'BRAKE → REVERSE' : 'REVERSE · BRACE') : frame?.preload_active ? 'PRELOADED' : frame?.preload_armed ? 'LIFT FOR PRELOAD' : current && applied.brake > 0 ? 'BRAKE · BRACE' : 'DRIVE · BRACE',
  };
}

// First-run coaching: compass axis labels and the idle hint show for the
// first COACH_RIDES page sessions, wheel instructions for the first
// COACH_WHEEL wheel opens. Storage denial keeps coaching for this session.
const COACH_RIDES = 3, COACH_WHEEL = 5;
const coachCount = key => { try { return Number(localStorage.getItem(key)) || 0; } catch { return 0; } };
const saveCoachCount = (key, value) => { try { localStorage.setItem(key, String(value)); } catch { /* Session-only count. */ } };
const SVG_NS = 'http://www.w3.org/2000/svg';
// Compass flip-zone arcs: flip and centre angle (clockwise from three o'clock), ±30° wide.
const FLIP_ARCS = [['backflip', 90], ['left', 180], ['right', 0]];
// Wheel geometry in CSS pixels from its centre; the outer band starts at OUTER_DISTANCE.
const WHEEL_RADIUS = 156, HUB = 28, INNER_EDGE = 126, INNER_LABEL = 80, OUTER_LABEL = 143, SIDE_MARGIN = 160;
const sectorPath = ({ angle, span }, inner, outer, gap) => {
  const a0 = angle - span + gap, a1 = angle + span - gap;
  const at = (r, a) => `${(r * Math.cos(a)).toFixed(2)} ${(r * Math.sin(a)).toFixed(2)}`;
  return `M${at(inner, a0)}L${at(outer, a0)}A${outer} ${outer} 0 0 1 ${at(outer, a1)}L${at(inner, a1)}A${inner} ${inner} 0 0 0 ${at(inner, a0)}Z`;
};

export function mountTouchControls({ active, focus, surface, parent = document.body, utilities = document.querySelector('#toolbar'), flips = () => false }) {
  const input = new TouchInput();
  input.setOneHanded(loadOneHandedPreference());
  input.steeringSensitivity = loadSteeringSensitivity();
  let touched = false, airborne = false, lastFrame = null;
  let rides = coachCount('atv-touch-coach-rides'), wheelOpens = coachCount('atv-touch-coach-wheel'), rodeThisSession = false;
  const utilityPointers = new Map();
  const root = document.createElement('div'); root.id = 'touch-controls'; root.setAttribute('aria-hidden', 'true');
  root.innerHTML = '<div class="touch-rider-marker"><svg class="touch-ring" viewBox="0 0 96 96"><defs><linearGradient id="touch-ring-metal" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#f4f4f4"/><stop offset=".45" stop-color="#9a9a9a"/><stop offset=".55" stop-color="#5c5c5c"/><stop offset="1" stop-color="#c9c9c9"/></linearGradient></defs><circle class="touch-ring-shadow" cx="48" cy="48" r="46.5"/><circle class="touch-ring-rim" cx="48" cy="48" r="44"/><circle class="touch-ring-throttle" cx="48" cy="48" r="44" pathLength="100"/><circle class="touch-ring-preload" cx="48" cy="48" r="38.5" pathLength="100"/>' + FLIP_ARCS.map(([flip, angle]) => `<circle class="touch-flip-zone" data-flip="${flip}" cx="48" cy="48" r="55" pathLength="100" transform="rotate(${angle - 30} 48 48)"/>`).join('') + '<circle class="touch-flip-fill" cx="48" cy="48" r="55" pathLength="100"/></svg><b class="touch-axis forward">FWD</b><b class="touch-axis back">BACK</b><b class="touch-axis left">L</b><b class="touch-axis right">R</b><em class="touch-direction"></em><i></i><span>PRELOAD</span></div>' +
    '<div class="touch-idle-hint"><i></i><b></b><small></small></div>';
  parent.append(root);
  const marker = root.firstElementChild, label = marker.querySelector('span');
  const throttleArc = marker.querySelector('.touch-ring-throttle'), flipFill = marker.querySelector('.touch-flip-fill');
  let flipBuzzed = null;
  const hint = root.querySelector('.touch-idle-hint');
  const wheel = document.createElement('div'); wheel.className = 'touch-trick-wheel'; wheel.hidden = true;
  wheel.innerHTML = `<svg class="touch-trick-sectors" viewBox="${-WHEEL_RADIUS} ${-WHEEL_RADIUS} ${WHEEL_RADIUS * 2} ${WHEEL_RADIUS * 2}"><defs><radialGradient id="touch-trick-gold" cx="0" cy="0" r="${WHEEL_RADIUS}" gradientUnits="userSpaceOnUse"><stop offset=".2" stop-color="#fff7b8"/><stop offset=".55" stop-color="#e5d360"/><stop offset="1" stop-color="#a88604"/></radialGradient></defs><circle class="touch-trick-hub" r="${HUB - 2}"/></svg><span class="touch-trick-name"></span><div class="touch-trick-center"></div><span class="touch-trick-cue"></span>`;
  const sectors = wheel.firstElementChild;
  // Sector shapes precede labels so the selection reads by position before text.
  const addSector = (choice, inner, outer, kind) => {
    const path = document.createElementNS(SVG_NS, 'path');
    path.setAttribute('d', sectorPath(choice, inner, outer, kind === 'inner' ? .035 : .025));
    path.setAttribute('class', `touch-trick-sector ${kind}`);
    sectors.insertBefore(path, sectors.querySelector('.touch-trick-hub')); return path;
  };
  const addLabel = (choice, radius, className) => {
    const element = document.createElement('b'); element.className = className; element.textContent = choice.label;
    element.style.left = `${WHEEL_RADIUS + Math.cos(choice.angle) * radius}px`; element.style.top = `${WHEEL_RADIUS + Math.sin(choice.angle) * radius}px`;
    wheel.append(element); return element;
  };
  const trickSectors = TRICK_WHEEL.map(trick => trick.outer ? addSector(trick, OUTER_DISTANCE + 2, WHEEL_RADIUS, 'outer') : addSector(trick, HUB, INNER_EDGE, 'inner'));
  const choices = TRICK_WHEEL.map(trick => addLabel(trick, trick.outer ? OUTER_LABEL : INNER_LABEL, `touch-trick-choice${trick.outer ? ' outer' : ''}`));
  root.append(wheel);
  const wheelName = wheel.querySelector('.touch-trick-name'), wheelCenter = wheel.querySelector('.touch-trick-center'), wheelCue = wheel.querySelector('.touch-trick-cue');
  let wheelKey = null;
  const placeWheel = () => {
    const pointer = input.secondary?.pointer;
    if (pointer?.mode !== 'radial') return;
    // The displayed origin also owns selection, making every label reachable
    // near an edge. A stationary press never selects despite this adjustment.
    // The top margin keeps the trick name above the wheel; the bottom the cue.
    pointer.x = innerWidth < SIDE_MARGIN * 2 ? innerWidth / 2 : Math.max(SIDE_MARGIN, Math.min(innerWidth - SIDE_MARGIN, pointer.currentX));
    pointer.y = Math.max(WHEEL_RADIUS + 26, Math.min(innerHeight - WHEEL_RADIUS - 16, pointer.currentY));
  };
  const presentWheel = () => {
    const pointer = input.secondary?.pointer;
    const visible = Boolean(input.oneHanded && input.pointer && pointer?.mode === 'radial');
    if (visible && wheel.hidden) saveCoachCount('atv-touch-coach-wheel', ++wheelOpens);
    wheel.hidden = !visible;
    root.dataset.wheel = String(visible);
    if (!visible) { wheelKey = null; return; }
    const index = TRICK_WHEEL.indexOf(pointer.selection);
    const coach = wheelOpens <= COACH_WHEEL;
    const key = [pointer.x, pointer.y, index, coach].join('|');
    if (key === wheelKey) return;
    wheelKey = key;
    wheel.style.left = `${pointer.x}px`; wheel.style.top = `${pointer.y}px`;
    wheel.dataset.selected = String(index >= 0);
    wheel.dataset.coach = String(coach);
    trickSectors.forEach((sector, i) => sector.classList.toggle('selected', i === index));
    choices.forEach((choice, i) => choice.classList.toggle('selected', i === index));
    wheelName.textContent = pointer.selection?.name ?? '';
    wheelCenter.textContent = coach && !pointer.selection ? 'TRICKS' : '';
    wheelCue.textContent = !coach ? '' : pointer.selection ? 'LIFT TO PERFORM · CENTER CANCELS' : 'DRAG TO A TRICK · LIFT TO PERFORM';
  };
  let presentationKey = null;
  const present = () => {
    root.dataset.oneHanded = String(input.oneHanded);
    root.dataset.coach = String(rides <= COACH_RIDES);
    root.dataset.hint = String(!rodeThisSession && rides < COACH_RIDES);
    root.dataset.drive = input.oneHanded && input.secondary?.pointer && !input.airborne ? 'reverse' : 'forward';
    presentWheel();
    // The red throttle arc fills like a gauge with the held drive axis.
    const throttle = input.pointer?.mode === 'drive' ? Math.max(0, input.pointer.axis) : 0;
    throttleArc.style.strokeDasharray = `${(root.dataset.drive === 'reverse' ? 1 : throttle) * 42} 100`;
    if (!input.oneHanded) return;
    const state = oneHandedFeedback(input, lastFrame);
    // The release burst keeps naming achieved preload after the thumb lifts.
    const cue = state.cue || (lastFrame?.preload_active ? 'PRELOADED' : '');
    const armed = FLIPS.some(flip => flip.cue === cue);
    // A short buzz (where supported) confirms the flip armed; lifting now flips.
    if (armed && flipBuzzed !== state.flipZone) { flipBuzzed = state.flipZone; try { navigator.vibrate?.(12); } catch { /* Unsupported. */ } }
    if (!armed) flipBuzzed = null;
    const progress = Math.round(state.flipProgress * 20) / 20;
    const key = [state.steer, state.lean, cue, state.flipsReady, state.flipZone, progress].join('|');
    if (key === presentationKey) return;
    presentationKey = key;
    label.textContent = cue;
    marker.dataset.flip = String(armed);
    // Zone arcs appear only when a lift could flip; the entered arc fills over the dwell.
    marker.dataset.flipsReady = String(state.flipsReady);
    marker.dataset.flipZone = state.flipZone ?? '';
    const zone = FLIP_ARCS.find(([flip]) => flip === state.flipZone);
    if (zone) flipFill.setAttribute('transform', `rotate(${zone[1] - 30} 48 48)`);
    flipFill.style.strokeDasharray = `${progress * 100 / 6} 100`;
    marker.style.setProperty('--steer', `${state.steer * 34}px`);
    marker.style.setProperty('--lean', `${state.lean * 34}px`);
    marker.style.setProperty('--angle', `${Math.atan2(state.lean, state.steer)}rad`);
    marker.style.setProperty('--length', `${Math.hypot(state.steer, state.lean) * 34}px`);
  };
  const airLabel = () => input.flips ? 'SWIPE · LONG SWIPE FLIPS' : 'SWIPE / CIRCLE';
  const coarse = window.matchMedia('(any-pointer: coarse)');
  let hintMode = null;
  const refresh = () => {
    input.flips = Boolean(flips());
    if (input.secondary) input.secondary.flips = input.flips;
    root.hidden = !active();
    document.body.classList.toggle('touch-driving', !root.hidden && (coarse.matches || touched));
    if (hintMode !== input.oneHanded) {
      hintMode = input.oneHanded;
      hint.querySelector('b').textContent = input.oneHanded ? 'HOLD ANYWHERE TO RIDE' : 'HOLD RIGHT SIDE TO RIDE';
      hint.querySelector('small').textContent = input.oneHanded ? 'DRAG TO STEER & LEAN · LIFT ON RAMPS TO JUMP' : 'LEFT THUMB STEERS & LEANS · LIFT ON RAMPS TO JUMP';
    }
    present();
  };
  const clear = () => { input.clear(); utilityPointers.clear(); lastFrame = null; root.classList.remove('held'); root.dataset.preload = 'idle'; refresh(); };
  const down = event => {
    if (event.pointerType !== 'touch' || (!input.oneHanded && event.clientX < innerWidth / 2)) return;
    focus?.();
    if (!active()) return;
    event.preventDefault(); event.stopImmediatePropagation(); touched = true;
    if (!input.begin(event.pointerId, event.clientX, event.clientY, airborne)) return;
    if (input.secondary?.owns(event.pointerId)) placeWheel();
    surface.setPointerCapture(event.pointerId); root.classList.add('held');
    if (input.pointer?.id === event.pointerId) {
      if (!rodeThisSession) { rodeThisSession = true; saveCoachCount('atv-touch-coach-rides', ++rides); }
      // Keep the visual compass and its cue (above the ring, clear of the
      // thumb) on screen without moving the input origin.
      const [side, top, bottom] = input.oneHanded ? [74, 82, 60] : [72, 64, 40];
      marker.style.left = `${Math.max(side, Math.min(innerWidth - side, event.clientX))}px`;
      marker.style.top = `${Math.max(top, Math.min(innerHeight - bottom, event.clientY))}px`;
      marker.style.setProperty('--drive', '-30px');
    }
    if (!input.oneHanded) label.textContent = airborne ? airLabel() : 'PRELOAD';
    refresh();
  };
  const move = event => {
    if (!input.owns(event.pointerId)) return;
    event.preventDefault(); event.stopImmediatePropagation();
    const events = event.getCoalescedEvents?.() ?? [];
    for (const point of events.length ? events : [event]) input.move(event.pointerId, point.clientX, point.clientY, point.timeStamp);
    marker.style.setProperty('--drive', `${-(input.pointer?.axis ?? 0) * 30}px`); present();
  };
  const end = event => {
    if (!input.owns(event.pointerId)) return;
    if (event.type === 'pointerup' && input.secondary?.pointer?.mode === 'radial' && input.secondary.owns(event.pointerId)) input.move(event.pointerId, event.clientX, event.clientY);
    event.preventDefault(); event.stopImmediatePropagation(); input.end(event.pointerId, event.type !== 'pointerup', event.timeStamp); root.classList.toggle('held', Boolean(input.pointer)); present();
  };
  // WebKit's loupe can ignore CSS selection suppression. Cancel native touch
  // gestures on the active game surface as well; Pointer Events still own input.
  // Keep menu/toolbar taps and the separate selectable dev log outside this gate.
  const suppressNativeTouch = event => { if (active() && event.cancelable) event.preventDefault(); };
  surface.addEventListener('touchstart', suppressNativeTouch, { passive: false });
  surface.addEventListener('touchmove', suppressNativeTouch, { passive: false });
  const gestureTypes = ['gesturestart', 'gesturechange', 'gestureend'];
  for (const type of gestureTypes) surface.addEventListener(type, suppressNativeTouch, { passive: false });
  surface.addEventListener('pointerdown', down); surface.addEventListener('pointermove', move);
  for (const type of ['pointerup', 'pointercancel', 'lostpointercapture']) surface.addEventListener(type, end);
  // A second finger does not produce a compatibility click while a riding
  // touch is held. Activate utility buttons from their own touch release,
  // independently of the captured riding pointer and its native-touch gate.
  const utilityButton = target => target?.closest?.('button:not(:disabled)');
  const utilityDown = event => {
    const button = utilityButton(event.target);
    if (event.pointerType !== 'touch' || !button || !utilities.contains(button)) return;
    event.preventDefault(); event.stopPropagation();
    utilityPointers.set(event.pointerId, { button, x: event.clientX, y: event.clientY, moved: false });
    button.setPointerCapture(event.pointerId);
  };
  const utilityMove = event => {
    const pointer = utilityPointers.get(event.pointerId);
    if (!pointer) return;
    if (Math.hypot(event.clientX - pointer.x, event.clientY - pointer.y) > 16) pointer.moved = true;
    event.preventDefault(); event.stopPropagation();
  };
  const utilityEnd = event => {
    const pointer = utilityPointers.get(event.pointerId);
    if (!pointer) return;
    utilityPointers.delete(event.pointerId);
    event.preventDefault(); event.stopPropagation();
    const hit = utilityButton(document.elementFromPoint(event.clientX, event.clientY));
    const nearby = Math.hypot(event.clientX - pointer.x, event.clientY - pointer.y) <= 16;
    if (event.type === 'pointerup' && !pointer.moved && nearby && hit === pointer.button && !pointer.button.disabled) pointer.button.click();
  };
  // Cancel native touch on utilities too, so a primary finger cannot activate
  // once on pointer release and again through a synthesized mouse click.
  const utilityNativeTouch = event => { if (utilityButton(event.target) && event.cancelable) event.preventDefault(); };
  utilities?.addEventListener('touchstart', utilityNativeTouch, { passive: false });
  utilities?.addEventListener('touchmove', utilityNativeTouch, { passive: false });
  for (const type of gestureTypes) utilities?.addEventListener(type, suppressNativeTouch, { passive: false });
  utilities?.addEventListener('pointerdown', utilityDown);
  utilities?.addEventListener('pointermove', utilityMove);
  for (const type of ['pointerup', 'pointercancel', 'lostpointercapture']) utilities?.addEventListener(type, utilityEnd);
  const suppress = event => { if (!document.querySelector('#frontend')?.contains(event.target)) event.preventDefault(); };
  // Scoped to the game document. The separately loaded dev log remains selectable.
  document.addEventListener('selectstart', suppress); document.addEventListener('contextmenu', suppress);
  window.addEventListener('blur', clear); window.addEventListener('resize', clear); document.addEventListener('visibilitychange', clear);
  refresh();
  return {
    clear, refresh,
    get oneHanded() { return input.oneHanded; },
    get steeringSensitivity() { return input.steeringSensitivity; },
    setSteeringSensitivity(value) {
      input.steeringSensitivity = normalizeSteeringSensitivity(value);
      try { localStorage.setItem('atv-mobile-steering-sensitivity', String(input.steeringSensitivity)); } catch { /* Session-only setting. */ }
      present();
    },
    toggleOneHanded() {
      input.setOneHanded(!input.oneHanded);
      try { localStorage.setItem('atv-mobile-one-handed', String(input.oneHanded)); } catch { /* Session-only setting. */ }
      clear();
    },
    feedback(frame) {
      lastFrame = frame;
      airborne = Boolean(frame.grounded?.length && frame.grounded.every(contact => !contact));
      const wasAirborne = input.airborne;
      input.setAirborne(airborne);
      if (airborne !== wasAirborne) placeWheel();
      const state = frame.preload_active ? 'active' : frame.preload_armed ? 'armed' : frame.preload_available ? 'available' : 'idle';
      root.dataset.preload = state;
      if (input.oneHanded) present();
      else label.textContent = airborne ? (frame.stunt?.name ?? airLabel()) : state === 'armed' ? 'RELEASE' : state === 'active' ? 'PRELOADED' : 'BRACED';
    },
    sample() { refresh(); if (!active()) clear(); return input.sample(); },
    get engaged() { return touched; },
    get generation() { return input.generation; },
    destroy() {
      clear(); root.remove(); document.body.classList.remove('touch-driving');
      surface.removeEventListener('touchstart', suppressNativeTouch); surface.removeEventListener('touchmove', suppressNativeTouch);
      for (const type of gestureTypes) surface.removeEventListener(type, suppressNativeTouch);
      surface.removeEventListener('pointerdown', down); surface.removeEventListener('pointermove', move);
      for (const type of ['pointerup', 'pointercancel', 'lostpointercapture']) surface.removeEventListener(type, end);
      utilities?.removeEventListener('touchstart', utilityNativeTouch); utilities?.removeEventListener('touchmove', utilityNativeTouch);
      for (const type of gestureTypes) utilities?.removeEventListener(type, suppressNativeTouch);
      utilities?.removeEventListener('pointerdown', utilityDown); utilities?.removeEventListener('pointermove', utilityMove);
      for (const type of ['pointerup', 'pointercancel', 'lostpointercapture']) utilities?.removeEventListener(type, utilityEnd);
      document.removeEventListener('selectstart', suppress); document.removeEventListener('contextmenu', suppress);
      window.removeEventListener('blur', clear); window.removeEventListener('resize', clear); document.removeEventListener('visibilitychange', clear);
    },
  };
}
