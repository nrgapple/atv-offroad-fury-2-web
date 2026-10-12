// Browser gestures adapt input axes only; the recovered Rust gates own jump force.
const clamp = value => Math.max(-1, Math.min(1, Number.isFinite(value) ? value : 0));
// Gesture mappings are browser adaptations of original modifier/direction inputs.
export class TouchInput {
  constructor() { this.generation = 0; this.airborne = false; this.oneHanded = false; this.clear(); }
  clear() { this.generation++; this.pointer = null; this.pendingSelection = null; this.sampled = false; this.releasePending = false; }
  owns(id) { return this.pointer?.id === id; }
  begin(id, x = 0, y = 0, airborne = this.airborne) {
    if (this.pointer) return false;
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
    if (this.oneHanded) return;
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
    if (this.pointer?.id !== id) return;
    if (cancelled) { this.clear(); return; }
    if (this.pointer.mode === 'trick') {
      const sent = this.pointer.selectionSent;
      this.sample(performance.now()+120);
      if (!sent) this.pendingSelection = this.pointer.selection;
    }
    this.releasePending = this.sampled && this.pointer.mode === 'drive';
    this.pointer = null; this.sampled = false;
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
    const steer = leaning ? clamp((pointer.currentX-pointer.x)/70) : 0;
    const lean = leaning ? clamp((pointer.currentY-pointer.y)/70) : 0;
    const drive = { generation: this.generation, held: driving, axis: driving ? pointer.axis : 0, grounded_only: false };
    if (this.oneHanded) drive.forward_only = true;
    let preload_axis = null;
    if (driving) { this.sampled = true; preload_axis = 1; }
    else if (this.releasePending) { this.releasePending = false; preload_axis = -1; }
    const selection = pointer?.selection ?? this.pendingSelection;
    if (pointer?.selection) pointer.selectionSent = true;
    this.pendingSelection = null;
    return { drive, preload_axis, steer, lean, stunt_modifiers: selection?.modifiers ?? 0,
      stunt_x: selection?.axes[0] ?? 0, stunt_y: selection?.axes[1] ?? 0 };
  }
}

// The left thumb always steers and leans, whether grounded or airborne.
export class TouchLeanIntent {
  constructor() { this.clear(); }
  clear() { this.pointer = null; }
  begin(id, x, y) {
    if (this.pointer) return false;
    this.pointer = { id, x, y, dx: 0, dy: 0 }; return true;
  }
  move(id, x, y) {
    if (this.pointer?.id !== id) return;
    this.pointer.dx = clamp((x - this.pointer.x) / 70);
    this.pointer.dy = clamp((y - this.pointer.y) / 70);
  }
  end(id) { if (this.pointer?.id === id) this.pointer = null; }
  get steer() { return this.pointer?.dx ?? 0; }
  get lean() { return this.pointer?.dy ?? 0; }
}

// Presentation reads held intent without sampling it or consuming a preload release.
export function oneHandedFeedback(input, frame, released = false) {
  const pointer = input.pointer;
  const steer = pointer ? clamp((pointer.currentX - pointer.x) / 70) : 0;
  const lean = pointer ? clamp((pointer.currentY - pointer.y) / 70) : 0;
  const percent = value => Math.round(Math.abs(value) * 100);
  const applied = frame?.applied_input;
  const current = applied?.touch_drive?.generation === input.generation && applied.touch_drive.held;
  const preload = frame?.preload_active ? 'PRELOAD ACTIVE' : frame?.preload_armed ? 'LIFT FOR PRELOAD' : null;
  return {
    held: Boolean(pointer), steer, lean,
    phase: input.airborne ? 'AIR · SAME CONTROLS' : 'GROUND',
    drive: pointer ? current && applied.brake > 0 ? 'BRAKE TO MOVE FORWARD' : 'THROTTLE + BRACE' : released ? 'COASTING' : 'TOUCH TO DRIVE',
    steering: percent(steer) ? `${steer < 0 ? 'LEFT' : 'RIGHT'} ${percent(steer)}%` : 'STRAIGHT',
    leaning: percent(lean) ? `${lean < 0 ? 'FORWARD' : 'BACK'} ${percent(lean)}%` : 'NEUTRAL',
    hint: pointer ? preload ?? 'Lift: coast + release brace' : 'Drag: ↔ steer · ↕ lean',
  };
}

export function mountTouchControls({ active, focus, surface, parent = document.body }) {
  const input = new TouchInput();
  try { input.setOneHanded(localStorage.getItem('atv-mobile-one-handed') === 'true'); } catch { /* Session-only settings when storage is unavailable. */ }
  let touched = false, airborne = false, released = false, lastFrame = null;
  const root = document.createElement('div'); root.id = 'touch-controls'; root.setAttribute('aria-hidden', 'true');
  root.innerHTML = `<div class="touch-rider-marker"><b class="touch-axis forward">FWD</b><b class="touch-axis back">BACK</b><b class="touch-axis left">L</b><b class="touch-axis right">R</b><em class="touch-direction"></em><i></i><span>PRELOAD</span></div>
    <div class="touch-one-hand-readout"><div class="touch-mode">ONE FINGER <span class="touch-phase">GROUND</span></div><strong class="touch-drive-status">TOUCH TO DRIVE</strong><div class="touch-command-row"><span>STEER <b class="touch-steer-status">STRAIGHT</b></span><div class="touch-axis-meter steer"><i></i></div></div><div class="touch-command-row"><span>LEAN <b class="touch-lean-status">NEUTRAL</b></span><div class="touch-axis-meter lean"><i></i></div></div><small class="touch-hint">Drag: ↔ steer · ↕ lean</small></div>`;
  parent.append(root);
  const marker = root.firstElementChild, label = marker.querySelector('span');
  const readout = root.querySelector('.touch-one-hand-readout');
  const fields = Object.fromEntries(['phase', 'drive-status', 'steer-status', 'lean-status', 'hint'].map(name => [name, readout.querySelector(`.touch-${name}`)]));
  let presentationKey = null;
  const meters = { steer: readout.querySelector('.touch-axis-meter.steer i'), lean: readout.querySelector('.touch-axis-meter.lean i') };
  const present = () => {
    root.dataset.oneHanded = String(input.oneHanded);
    if (!input.oneHanded) return;
    const state = oneHandedFeedback(input, lastFrame, released);
    const key = [state.steer, state.lean, state.phase, state.drive, state.hint].join('|');
    if (key === presentationKey) return;
    presentationKey = key;
    root.dataset.phase = input.airborne ? 'air' : 'ground';
    root.dataset.drive = state.held ? 'held' : released ? 'coast' : 'idle';
    fields.phase.textContent = state.phase;
    fields['drive-status'].textContent = state.drive;
    fields['steer-status'].textContent = state.steering;
    fields['lean-status'].textContent = state.leaning;
    fields.hint.textContent = state.hint;
    marker.style.setProperty('--steer', `${state.steer * 34}px`);
    marker.style.setProperty('--lean', `${state.lean * 34}px`);
    marker.style.setProperty('--angle', `${Math.atan2(state.lean, state.steer)}rad`);
    marker.style.setProperty('--length', `${Math.hypot(state.steer, state.lean) * 34}px`);
    for (const [axis, value] of [['steer', state.steer], ['lean', state.lean]]) {
      const meter = meters[axis];
      meter.style.left = `${50 + Math.min(value, 0) * 50}%`;
      meter.style.width = `${Math.abs(value) * 50}%`;
    }
  };
  const coarse = window.matchMedia('(any-pointer: coarse)');
  const refresh = () => {
    root.hidden = !active();
    document.body.classList.toggle('touch-driving', !root.hidden && (coarse.matches || touched));
    present();
  };
  const clear = () => { input.clear(); released = false; lastFrame = null; root.classList.remove('held'); root.dataset.preload = 'idle'; refresh(); };
  const down = event => {
    if (event.pointerType !== 'touch' || (!input.oneHanded && event.clientX < innerWidth / 2)) return;
    focus?.();
    if (!active()) return;
    event.preventDefault(); event.stopImmediatePropagation(); touched = true;
    if (!input.begin(event.pointerId, event.clientX, event.clientY, airborne)) return;
    surface.setPointerCapture(event.pointerId); root.classList.add('held'); released = false;
    if (input.pointer?.id === event.pointerId) {
      // Keep the visual compass and labels on screen without moving the input origin.
      marker.style.left = `${input.oneHanded ? Math.max(62, Math.min(innerWidth - 62, event.clientX)) : event.clientX}px`;
      marker.style.top = `${input.oneHanded ? Math.max(82, Math.min(innerHeight - 164, event.clientY)) : event.clientY}px`;
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
    event.preventDefault(); event.stopImmediatePropagation(); released = event.type === 'pointerup'; input.end(event.pointerId, !released); root.classList.toggle('held', Boolean(input.pointer)); present();
  };
  surface.addEventListener('pointerdown', down); surface.addEventListener('pointermove', move);
  for (const type of ['pointerup', 'pointercancel', 'lostpointercapture']) surface.addEventListener(type, end);
  const suppress = event => { if (!document.querySelector('#frontend')?.contains(event.target)) event.preventDefault(); };
  // Scoped to the game document. The separately loaded dev log remains selectable.
  document.addEventListener('selectstart', suppress); document.addEventListener('contextmenu', suppress);
  window.addEventListener('blur', clear); window.addEventListener('resize', clear); document.addEventListener('visibilitychange', clear);
  refresh();
  return {
    clear, refresh,
    get oneHanded() { return input.oneHanded; },
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
      root.dataset.preload = state; present(); label.textContent = airborne && !input.oneHanded ? (frame.stunt?.name ?? 'SWIPE / CIRCLE') : state === 'armed' ? 'RELEASE' : state === 'active' ? 'PRELOADED' : 'BRACED';
    },
    sample() { refresh(); if (!active()) clear(); return input.sample(); },
    get engaged() { return touched; },
    get generation() { return input.generation; },
    destroy() {
      clear(); root.remove(); document.body.classList.remove('touch-driving');
      surface.removeEventListener('pointerdown', down); surface.removeEventListener('pointermove', move);
      for (const type of ['pointerup', 'pointercancel', 'lostpointercapture']) surface.removeEventListener(type, end);
      document.removeEventListener('selectstart', suppress); document.removeEventListener('contextmenu', suppress);
      window.removeEventListener('blur', clear); window.removeEventListener('resize', clear); document.removeEventListener('visibilitychange', clear);
    },
  };
}
