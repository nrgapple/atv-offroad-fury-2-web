// Browser gestures adapt input axes only; the recovered Rust gates own jump force.
const clamp = value => Math.max(-1, Math.min(1, Number.isFinite(value) ? value : 0));
export class TouchInput {
  constructor() { this.generation = 0; this.clear(); }
  clear() { this.generation++; this.pointer = null; this.stunt = null; this.sampled = false; this.releasePending = false; }
  owns(id) { return this.pointer?.id === id || this.stunt?.id === id; }
  begin(id, x = 0, y = 0, airborne = false) {
    if (this.owns(id)) return false;
    if (this.pointer) {
      // A second right finger selects an original stunt independently of the
      // primary thumb's throttle. Grounded extra fingers cannot steal drive.
      if (!airborne || this.stunt) return false;
      this.stunt = { id, x, y, axes: null }; return true;
    }
    this.pointer = { id, x, y, axis: 0 }; this.sampled = false; this.releasePending = false;
    return true;
  }
  move(id, x, y) {
    if (this.pointer?.id === id) this.pointer.axis = clamp((this.pointer.y - y) / 70);
    else if (this.stunt?.id === id) {
      const pointer = this.stunt, dx = x - pointer.x, dy = y - pointer.y;
      if (!pointer.axes && Math.hypot(dx, dy) >= 36) pointer.axes = Math.abs(dy) >= Math.abs(dx) ? [0, Math.sign(dy)] : [Math.sign(dx), 0];
    }
  }
  end(id, cancelled = false) {
    if (this.stunt?.id === id) { this.stunt = null; return; }
    if (this.pointer?.id !== id) return;
    if (cancelled) { this.clear(); return; }
    this.releasePending = this.sampled; this.pointer = null; this.sampled = false;
  }
  sample() {
    const drive = { generation: this.generation, held: Boolean(this.pointer), axis: this.pointer?.axis ?? 0 };
    let preload_axis = null;
    if (this.pointer) { this.sampled = true; preload_axis = 1; }
    else if (this.releasePending) { this.releasePending = false; preload_axis = -1; }
    return { drive, preload_axis, stunt_modifiers: this.stunt?.axes ? 1 : 0, stunt_x: this.stunt?.axes?.[0] ?? 0, stunt_y: this.stunt?.axes?.[1] ?? 0 };
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
    if (event.pointerType !== 'touch' || event.clientX < innerWidth / 2) return;
    focus?.();
    if (!active()) return;
    event.preventDefault(); event.stopImmediatePropagation(); touched = true;
    if (!input.begin(event.pointerId, event.clientX, event.clientY, airborne)) return;
    surface.setPointerCapture(event.pointerId); root.classList.add('held');
    if (input.pointer?.id === event.pointerId) { marker.style.left = `${event.clientX}px`; marker.style.top = `${event.clientY}px`; marker.style.setProperty('--drive', '0px'); }
    label.textContent = input.stunt ? 'SWIPE' : 'PRELOAD'; refresh();
  };
  const move = event => {
    if (!input.owns(event.pointerId)) return;
    event.preventDefault(); event.stopImmediatePropagation(); input.move(event.pointerId, event.clientX, event.clientY);
    marker.style.setProperty('--drive', `${-(input.pointer?.axis ?? 0) * 30}px`);
  };
  const end = event => {
    if (!input.owns(event.pointerId)) return;
    event.preventDefault(); event.stopImmediatePropagation(); input.end(event.pointerId, event.type !== 'pointerup'); root.classList.toggle('held', Boolean(input.pointer || input.stunt));
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
      root.dataset.preload = state; label.textContent = input.stunt ? (frame.stunt?.name ?? 'SWIPE') : state === 'armed' ? 'RELEASE' : state === 'active' ? 'PRELOADED' : 'BRACED';
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
