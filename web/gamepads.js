// Which controller is the player using? Chrome on Windows often lists idle
// devices ahead of the real pad: Steam Input or DS4Windows virtual pads, and
// the HID interfaces of some keyboards, mice and wheels. Reading only the
// first listed pad then ignores the controller in the player's hands. Follow
// the pad with fresh input instead, as the workbench's "most recently used".
//
// The original's ControllerSelect page let each of four split-screen players
// claim a pad by pressing ×. This port has one rider per browser, so "Auto"
// plays that role (the pad you press is the one used) and Controller
// Settings can also pin one connected pad by name.

// Axes resting far from centre (idle triggers, hat switches, pedals) are
// measured from their first-seen value so they never count as activity.
const RESTING = .5, MOVED = .35;
export const CONTROLLER_STORAGE_KEY = 'atv2-controller-v1';

// Port enhancements; the original has no sensitivity settings (each quad's
// .phy fixes XAxisDeadZone = 0.0). 100% keeps the unmodified response; the
// 15% dead zone is the port's existing stick dead zone.
export const TUNING = Object.freeze({
  steering: { min: 50, max: 150, step: 5, value: 100 },
  lean: { min: 50, max: 150, step: 5, value: 100 },
  triggers: { min: 50, max: 150, step: 5, value: 100 },
  camera: { min: 50, max: 150, step: 5, value: 100 },
  deadzone: { min: 0, max: 40, step: 5, value: 15 },
});
const ACTION_GROUP = { steerLeft: 'steering', steerRight: 'steering', leanForward: 'lean', leanBack: 'lean', throttle: 'triggers', brake: 'triggers', cameraLeft: 'camera', cameraRight: 'camera' };

/** Readable pad name: Chrome's "(STANDARD GAMEPAD Vendor: …)" and Firefox's "054c-0ce6-" are dropped. */
export function padName(pad) {
  const name = String(pad?.id ?? '').replace(/\s*\((?:[^()]*(?:STANDARD GAMEPAD|Vendor:|XInput)[^()]*)\)\s*/gi, ' ').replace(/^[0-9a-f]{1,4}-[0-9a-f]{1,4}-/i, '').replace(/\s+/g, ' ').trim();
  return (name || `Controller ${(pad?.index ?? 0) + 1}`).slice(0, 48);
}

export class ControllerSettings {
  constructor(storage) {
    if (storage === undefined) { try { storage = globalThis.localStorage; } catch { storage = null; } }
    this.storage = storage; this.listeners = new Set(); this.selected = null;
    for (const [key, spec] of Object.entries(TUNING)) this[key] = spec.value;
    try {
      const saved = JSON.parse(storage?.getItem(CONTROLLER_STORAGE_KEY) ?? 'null');
      if (saved && typeof saved === 'object') {
        for (const key of Object.keys(TUNING)) if (Number.isFinite(saved[key])) this[key] = this.clamp(key, saved[key]);
        if (typeof saved.selected?.id === 'string' && saved.selected.id.length <= 256)
          this.selected = { id: saved.selected.id, index: Number.isInteger(saved.selected.index) ? saved.selected.index : 0 };
      }
    } catch { /* defaults */ }
  }
  clamp(key, value) { const { min, max, step } = TUNING[key]; return Math.min(max, Math.max(min, Math.round(value / step) * step)); }
  subscribe(listener) { this.listeners.add(listener); return () => this.listeners.delete(listener); }
  changed() {
    try { this.storage?.setItem(CONTROLLER_STORAGE_KEY, JSON.stringify({ selected: this.selected, ...Object.fromEntries(Object.keys(TUNING).map(key => [key, this[key]])) })); } catch { /* session only */ }
    for (const listener of this.listeners) listener();
  }
  set(key, value) { if (!TUNING[key] || !Number.isFinite(value)) return; const next = this.clamp(key, value); if (next !== this[key]) { this[key] = next; this.changed(); } }
  step(key, direction) { this.set(key, this[key] + direction * TUNING[key].step); }
  /** Restores the response settings; the chosen controller is kept. */
  reset() { for (const [key, spec] of Object.entries(TUNING)) this[key] = spec.value; this.changed(); }
  /** null selects Auto. */
  select(pad) { this.selected = pad ? { id: pad.id, index: pad.index } : null; this.changed(); }
  get zone() { return this.deadzone / 100; }
  /** Response curve for an analog action: full travel still reaches 1. */
  shape(id, value) {
    const group = ACTION_GROUP[id], sensitivity = group ? this[group] : 100;
    return value > 0 && value < 1 && sensitivity !== 100 ? value ** (100 / sensitivity) : value;
  }
}

export function createPadTracker(source = () => navigator.getGamepads?.() ?? [], settings = null) {
  const baselines = new Map();
  let current = null;
  const key = pad => `${pad.index}:${pad.id}`;
  const active = pad => {
    let base = baselines.get(key(pad));
    if (!base) baselines.set(key(pad), base = pad.axes.map(v => Number.isFinite(v) && Math.abs(v) > RESTING ? v : 0));
    return pad.buttons.some(b => b?.pressed) || pad.axes.some((v, i) => Number.isFinite(v) && Math.abs(v - (base[i] ?? 0)) > MOVED);
  };
  const connected = () => { try { return [...source()].filter(pad => pad && pad.connected !== false); } catch { return []; } };
  const pinned = pads => {
    const want = settings?.selected; if (!want) return null;
    const same = pads.filter(p => p.id === want.id);
    return same.find(p => p.index === want.index) ?? same[0] ?? null;
  };
  function activeGamepad() {
    const pads = connected();
    for (const id of baselines.keys()) if (!pads.some(pad => key(pad) === id)) baselines.delete(id);
    const live = pads.filter(active);
    // A pinned pad wins while connected; otherwise Auto follows fresh input.
    const pad = pinned(pads) ?? live.find(p => key(p) === current) ?? live[0] ?? pads.find(p => key(p) === current)
      ?? pads.find(p => p.mapping === 'standard') ?? pads[0] ?? null;
    current = pad && key(pad);
    return pad;
  }
  activeGamepad.connected = connected;
  activeGamepad.pinned = () => pinned(connected());
  return activeGamepad;
}

export const controllerSettings = new ControllerSettings();
export const activeGamepad = createPadTracker(undefined, controllerSettings);

/** Value for the Controller row: Auto (pad in use), a pinned pad, or a pinned pad that is absent. */
export function controllerLabel(settings = controllerSettings, tracker = activeGamepad) {
  if (settings.selected) return tracker.pinned() ? padName(settings.selected) : `${padName(settings.selected)} (not connected, using Auto)`;
  const pad = tracker();
  return pad ? `Auto (${padName(pad)})` : 'Auto (press a button)';
}

/** Steps Auto → each connected pad → Auto. */
export function cycleController(direction = 1, settings = controllerSettings, tracker = activeGamepad) {
  const options = [null, ...tracker.connected()], pinned = settings.selected && tracker.pinned();
  const at = pinned ? options.findIndex(p => p && p.id === pinned.id && p.index === pinned.index) : 0;
  settings.select(options[(at + direction + options.length) % options.length]);
}
