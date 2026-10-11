// Browser gestures adapt input axes only; the recovered Rust gates own jump force.
const clamp = value => Math.max(-1, Math.min(1, Number.isFinite(value) ? value : 0));
export class TouchInput {
  constructor() { this.clear(); }
  clear() { this.pointer = null; this.sampled = false; this.releasePending = false; }
  begin(id, x = 0, y = 0, airborne = false) {
    if (this.pointer) return false;
    this.pointer = { id, x, y, lean: 0, stunt: airborne, axes: null }; this.sampled = false; this.releasePending = false;
    return true;
  }
  move(id, x, y) {
    if (this.pointer?.id !== id) return;
    const pointer = this.pointer, dx = x - pointer.x, dy = y - pointer.y;
    if (pointer.stunt) {
      // One airborne swipe is a held Circle+direction command. It cannot turn
      // into a preload release or keep selecting different stunts mid-gesture.
      if (!pointer.axes && Math.hypot(dx, dy) >= 36) pointer.axes = Math.abs(dy) >= Math.abs(dx) ? [0, Math.sign(dy)] : [Math.sign(dx), 0];
    } else pointer.lean = clamp(dy / 70);
  }
  end(id, cancelled = false) {
    if (this.pointer?.id !== id) return;
    this.releasePending = !cancelled && this.sampled && !this.pointer.stunt; this.pointer = null; this.sampled = false;
  }
  sample() {
    if (this.pointer) {
      this.sampled = true;
      if (this.pointer.stunt) return { lean: 0, preload_axis: null, stunt_modifiers: this.pointer.axes ? 1 : 0, stunt_x: this.pointer.axes?.[0] ?? 0, stunt_y: this.pointer.axes?.[1] ?? 0 };
      return { lean: this.pointer.lean, preload_axis: 1 };
    }
    if (this.releasePending) { this.releasePending = false; return { lean: 0, preload_axis: -1 }; }
    return { lean: 0, preload_axis: null };
  }
}

// Browser convenience, not recovered controller behavior. Direction changes
// brake first and select the other gear only near standstill (1 ft/s).
export class TouchDrive {
  constructor() { this.clear(); }
  clear() { this.reverse = false; this.engaged = false; this.ground = { throttle: 0, brake: 0, reverse: false }; }
  sample(axis, speed) {
    axis = clamp(axis); speed = Number.isFinite(speed) ? speed : 0;
    const strength = Math.abs(axis);
    if (strength <= .08) this.ground = { throttle: 0, brake: 0, reverse: this.reverse };
    else {
      this.engaged = true;
      const wantedReverse = axis < 0;
      if ((wantedReverse && speed > 1) || (!wantedReverse && speed < -1)) {
        this.reverse = speed < 0;
        this.ground = { throttle: 0, brake: strength, reverse: this.reverse };
      } else {
        this.reverse = wantedReverse;
        this.ground = { throttle: strength, brake: 0, reverse: this.reverse };
      }
    }
    return { ...this.ground };
  }
  // Keep the accelerator/brake held at takeoff while the same thumb now controls
  // the original airborne pitch axis. No velocity or angular impulse is added.
  airSample(lean) { return { ...this.ground, lean: clamp(lean) }; }

}

export function mountTouchControls({ active, focus, surface, parent = document.body }) {
  const input = new TouchInput(); let touched = false, airborne = false;
  const root = document.createElement('div'); root.id = 'touch-controls'; root.setAttribute('aria-hidden', 'true');
  root.innerHTML = '<div class="touch-rider-marker"><i></i><span>PRELOAD</span></div>';
  parent.append(root);
  const marker = root.firstElementChild, label = marker.querySelector('span');
  const coarse = window.matchMedia('(any-pointer: coarse)');
  const refresh = () => {
    root.hidden = !active();
    document.body.classList.toggle('touch-driving', !root.hidden && (coarse.matches || touched));
  };
  const clear = () => { input.clear(); root.classList.remove('held'); root.dataset.preload = 'idle'; refresh(); };
  const down = event => {
    if (event.pointerType !== 'touch' || event.clientX < innerWidth / 2 || !active()) return;
    event.preventDefault(); event.stopImmediatePropagation(); touched = true;
    if (!input.begin(event.pointerId, event.clientX, event.clientY, airborne)) return;
    focus?.(); surface.setPointerCapture(event.pointerId); root.classList.add('held');
    marker.style.left = `${event.clientX}px`; marker.style.top = `${event.clientY}px`; marker.style.setProperty('--lean', '0px');
    label.textContent = airborne ? 'SWIPE' : 'PRELOAD'; refresh();
  };
  const move = event => {
    if (input.pointer?.id !== event.pointerId) return;
    event.preventDefault(); event.stopImmediatePropagation(); input.move(event.pointerId, event.clientX, event.clientY);
    marker.style.setProperty('--lean', `${input.pointer.lean * 30}px`);
  };
  const end = event => {
    if (input.pointer?.id !== event.pointerId) return;
    event.preventDefault(); event.stopImmediatePropagation(); input.end(event.pointerId, event.type !== 'pointerup'); root.classList.remove('held');
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
    feedback(frame) {
      airborne = Boolean(frame.grounded?.every(contact => !contact));
      const state = frame.preload_active ? 'active' : frame.preload_armed ? 'armed' : frame.preload_available ? 'available' : 'idle';
      root.dataset.preload = state; label.textContent = input.pointer?.stunt ? (frame.stunt?.name ?? 'SWIPE') : state === 'armed' ? 'RELEASE' : state === 'active' ? 'PRELOADED' : 'PRELOAD';
    },
    sample() { refresh(); if (!active()) clear(); return input.sample(); },
    get engaged() { return touched; },
    destroy() {
      clear(); root.remove(); document.body.classList.remove('touch-driving');
      surface.removeEventListener('pointerdown', down); surface.removeEventListener('pointermove', move);
      for (const type of ['pointerup', 'pointercancel', 'lostpointercapture']) surface.removeEventListener(type, end);
      document.removeEventListener('selectstart', suppress); document.removeEventListener('contextmenu', suppress);
      window.removeEventListener('blur', clear); window.removeEventListener('resize', clear); document.removeEventListener('visibilitychange', clear);
    },
  };
}
