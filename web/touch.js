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
// Gesture mappings are browser adaptations of original modifier/direction inputs.
export class TouchInput {
  constructor() { this.generation = 0; this.airborne = false; this.oneHanded = false; this.steeringSensitivity = 100; this.clear(); }
  clear() { this.generation++; this.pointer = null; this.secondary = null; this.pendingSelection = null; this.sampled = false; this.releasePending = false; }
  owns(id) { return this.pointer?.id === id || this.secondary?.owns(id) === true; }
  begin(id, x = 0, y = 0, airborne = this.airborne) {
    if (this.pointer) {
      if (!this.oneHanded || this.owns(id) || this.secondary?.pointer) return false;
      // Reuse the established gesture recognizer with a separate origin/history.
      this.secondary = new TouchInput();
      return this.secondary.begin(id, x, y, airborne);
    }
    this.secondary = null;
    this.airborne = airborne;
    this.pointer = { id, x, y, currentX: x, currentY: y, axis: 1,
      mode: airborne && !this.oneHanded ? 'trick' : 'drive', points: [[x,y]], lastMove: Infinity, selection: null };
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
      pointer.mode = airborne ? 'trick' : 'drive';
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
    if (pointer.mode === 'drive') {
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
  end(id, cancelled = false) {
    if (this.secondary?.owns(id)) { this.secondary.end(id, cancelled); return; }
    if (this.pointer?.id !== id) return;
    if (cancelled) { this.clear(); return; }
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
        const axes = diagonal ? (dy < 0 ? (dx < 0 ? [0,-1] : [1,0]) : (dx < 0 ? [-1,0] : [0,1]))
          : Math.abs(dy) >= Math.abs(dx) ? [0,Math.sign(dy)] : [Math.sign(dx),0];
        pointer.selection = { modifiers: diagonal ? 2 : 1, axes };
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
    const selection = secondary?.stunt_modifiers ? { modifiers: secondary.stunt_modifiers, axes: [secondary.stunt_x, secondary.stunt_y] } : pointer?.selection ?? this.pendingSelection;
    if (pointer?.selection) pointer.selectionSent = true;
    this.pendingSelection = null;
    return { drive, preload_axis, steer, lean, stunt_modifiers: selection?.modifiers ?? 0,
      stunt_x: selection?.axes[0] ?? 0, stunt_y: selection?.axes[1] ?? 0 };
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
  move(id, x, y) {
    if (this.pointer?.id !== id) return;
    this.pointer.dx = x - this.pointer.x;
    this.pointer.dy = clamp((y - this.pointer.y) / 70);
  }
  end(id) { if (this.pointer?.id === id) this.pointer = null; }
  get steer() { return steeringAxis(this.pointer?.dx ?? 0, this.sensitivity()); }
  get lean() { return this.pointer?.dy ?? 0; }
}

// Only an explicitly saved Off overrides the one-finger default.
export function loadOneHandedPreference(read = () => localStorage.getItem('atv-mobile-one-handed')) {
  try { return read() !== 'false'; } catch { return true; }
}

// Read intent without sampling it or consuming a preload release.
export function oneHandedFeedback(input, frame) {
  const pointer = input.pointer;
  const applied = frame?.applied_input;
  const current = applied?.touch_drive?.generation === input.generation && applied.touch_drive.held;
  return {
    steer: pointer ? steeringAxis(pointer.currentX - pointer.x, input.steeringSensitivity) : 0,
    lean: pointer ? clamp((pointer.currentY - pointer.y) / 70) : 0,
    cue: !pointer ? '' : input.secondary?.pointer ? (input.airborne ? frame?.stunt?.name ?? 'SWIPE / CIRCLE' : current && applied.brake > 0 ? 'BRAKE → REVERSE' : 'REVERSE · BRACE') : frame?.preload_active ? 'PRELOADED' : frame?.preload_armed ? 'LIFT FOR PRELOAD' : current && applied.brake > 0 ? 'BRAKE · BRACE' : 'DRIVE · BRACE',
  };
}

export function mountTouchControls({ active, focus, surface, parent = document.body, utilities = document.querySelector('#toolbar') }) {
  const input = new TouchInput();
  input.setOneHanded(loadOneHandedPreference());
  input.steeringSensitivity = loadSteeringSensitivity();
  let touched = false, airborne = false, lastFrame = null;
  const utilityPointers = new Map();
  const root = document.createElement('div'); root.id = 'touch-controls'; root.setAttribute('aria-hidden', 'true');
  root.innerHTML = '<div class="touch-rider-marker"><b class="touch-axis forward">FWD</b><b class="touch-axis back">BACK</b><b class="touch-axis left">L</b><b class="touch-axis right">R</b><em class="touch-direction"></em><i></i><span>PRELOAD</span></div>';
  parent.append(root);
  const marker = root.firstElementChild, label = marker.querySelector('span');
  let presentationKey = null;
  const present = () => {
    root.dataset.oneHanded = String(input.oneHanded);
    if (!input.oneHanded) return;
    const state = oneHandedFeedback(input, lastFrame);
    const key = [state.steer, state.lean, state.cue].join('|');
    if (key === presentationKey) return;
    presentationKey = key;
    label.textContent = state.cue;
    marker.style.setProperty('--steer', `${state.steer * 34}px`);
    marker.style.setProperty('--lean', `${state.lean * 34}px`);
    marker.style.setProperty('--angle', `${Math.atan2(state.lean, state.steer)}rad`);
    marker.style.setProperty('--length', `${Math.hypot(state.steer, state.lean) * 34}px`);
  };
  const coarse = window.matchMedia('(any-pointer: coarse)');
  const refresh = () => {
    root.hidden = !active();
    document.body.classList.toggle('touch-driving', !root.hidden && (coarse.matches || touched));
    present();
  };
  const clear = () => { input.clear(); utilityPointers.clear(); lastFrame = null; root.classList.remove('held'); root.dataset.preload = 'idle'; refresh(); };
  const down = event => {
    if (event.pointerType !== 'touch' || (!input.oneHanded && event.clientX < innerWidth / 2)) return;
    focus?.();
    if (!active()) return;
    event.preventDefault(); event.stopImmediatePropagation(); touched = true;
    if (!input.begin(event.pointerId, event.clientX, event.clientY, airborne)) return;
    surface.setPointerCapture(event.pointerId); root.classList.add('held');
    if (input.pointer?.id === event.pointerId) {
      // Keep the visual compass and labels on screen without moving the input origin.
      marker.style.left = `${input.oneHanded ? Math.max(74, Math.min(innerWidth - 74, event.clientX)) : event.clientX}px`;
      marker.style.top = `${input.oneHanded ? Math.max(82, Math.min(innerHeight - 92, event.clientY)) : event.clientY}px`;
      marker.style.setProperty('--drive', '-30px');
    }
    label.textContent = airborne && !input.oneHanded ? 'SWIPE / CIRCLE' : input.oneHanded ? 'BRACED' : 'PRELOAD'; refresh();
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
    event.preventDefault(); event.stopImmediatePropagation(); input.end(event.pointerId, event.type !== 'pointerup'); root.classList.toggle('held', Boolean(input.pointer)); present();
  };
  // WebKit's loupe can ignore CSS selection suppression. Cancel native touch
  // gestures on the active game surface as well; Pointer Events still own input.
  // Keep menu/toolbar taps and the separate selectable dev log outside this gate.
  const suppressNativeTouch = event => { if (active() && event.cancelable) event.preventDefault(); };
  surface.addEventListener('touchstart', suppressNativeTouch, { passive: false });
  surface.addEventListener('touchmove', suppressNativeTouch, { passive: false });
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
      input.setAirborne(airborne);
      const state = frame.preload_active ? 'active' : frame.preload_armed ? 'armed' : frame.preload_available ? 'available' : 'idle';
      root.dataset.preload = state;
      if (input.oneHanded) present();
      else label.textContent = airborne ? (frame.stunt?.name ?? 'SWIPE / CIRCLE') : state === 'armed' ? 'RELEASE' : state === 'active' ? 'PRELOADED' : 'BRACED';
    },
    sample() { refresh(); if (!active()) clear(); return input.sample(); },
    get engaged() { return touched; },
    get generation() { return input.generation; },
    destroy() {
      clear(); root.remove(); document.body.classList.remove('touch-driving');
      surface.removeEventListener('touchstart', suppressNativeTouch); surface.removeEventListener('touchmove', suppressNativeTouch);
      surface.removeEventListener('pointerdown', down); surface.removeEventListener('pointermove', move);
      for (const type of ['pointerup', 'pointercancel', 'lostpointercapture']) surface.removeEventListener(type, end);
      utilities?.removeEventListener('touchstart', utilityNativeTouch); utilities?.removeEventListener('touchmove', utilityNativeTouch);
      utilities?.removeEventListener('pointerdown', utilityDown); utilities?.removeEventListener('pointermove', utilityMove);
      for (const type of ['pointerup', 'pointercancel', 'lostpointercapture']) utilities?.removeEventListener(type, utilityEnd);
      document.removeEventListener('selectstart', suppress); document.removeEventListener('contextmenu', suppress);
      window.removeEventListener('blur', clear); window.removeEventListener('resize', clear); document.removeEventListener('visibilitychange', clear);
    },
  };
}
