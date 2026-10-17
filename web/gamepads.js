// Which controller is the player using? Chrome on Windows often lists idle
// devices ahead of the real pad: Steam Input or DS4Windows virtual pads, and
// the HID interfaces of some keyboards, mice and wheels. Reading only the
// first listed pad then ignores the controller in the player's hands. Follow
// the pad with fresh input instead, as the workbench's "most recently used".

// Axes resting far from centre (idle triggers, hat switches, pedals) are
// measured from their first-seen value so they never count as activity.
const RESTING = .5, MOVED = .35;

export function createPadTracker(source = () => navigator.getGamepads?.() ?? []) {
  const baselines = new Map();
  let current = null;
  const key = pad => `${pad.index}:${pad.id}`;
  const active = pad => {
    let base = baselines.get(key(pad));
    if (!base) baselines.set(key(pad), base = pad.axes.map(v => Number.isFinite(v) && Math.abs(v) > RESTING ? v : 0));
    return pad.buttons.some(b => b?.pressed) || pad.axes.some((v, i) => Number.isFinite(v) && Math.abs(v - (base[i] ?? 0)) > MOVED);
  };
  return function activeGamepad() {
    let pads;
    try { pads = [...source()].filter(pad => pad && pad.connected !== false); } catch { return null; }
    for (const id of baselines.keys()) if (!pads.some(pad => key(pad) === id)) baselines.delete(id);
    const live = pads.filter(active);
    const pad = live.find(p => key(p) === current) ?? live[0] ?? pads.find(p => key(p) === current)
      ?? pads.find(p => p.mapping === 'standard') ?? pads[0] ?? null;
    current = pad && key(pad);
    return pad;
  };
}

export const activeGamepad = createPadTracker();
