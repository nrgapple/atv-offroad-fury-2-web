// Browser-independent input math. Proposed mappings, pending original-game verification.
export const defaultMapping = Object.freeze({ steer: { axis: 0 }, lean: { axis: 1 }, cameraX: { axis: 2 }, cameraY: { axis: 3 }, throttle: { button: 7 }, brake: { button: 6 }, preload: { button: 4 }, trick: { button: 5 }, jump: { button: 0 }, lookBack: { button: 1 }, reset: { button: 3 }, pause: { button: 9 } });
export const clamp = (v, low = -1, high = 1) => Math.min(high, Math.max(low, Number.isFinite(v) ? v : 0));
export function deadzone(value, zone = .15) { const v = clamp(value); return Math.abs(v) <= zone ? 0 : Math.sign(v) * (Math.abs(v) - zone) / (1 - zone); }
export function stick(x, y, radius = 64) { const length = Math.hypot(x, y); const scale = Math.max(radius, length); return { x: x / scale, y: y / scale }; }
export function gesture(dx, dy, threshold = 36) {
  if (Math.hypot(dx, dy) < threshold) return null;
  const directions = ['right', 'down-right', 'down', 'down-left', 'left', 'up-left', 'up', 'up-right'];
  return directions[(Math.round(Math.atan2(dy, dx) / (Math.PI / 4)) + 8) % 8];
}
export function validMapping(value) {
  if (!value || typeof value !== 'object') return false;
  return Object.keys(defaultMapping).every(key => {
    const entry = value[key]; const type = 'axis' in defaultMapping[key] ? 'axis' : 'button';
    return entry && Number.isInteger(entry[type]) && entry[type] >= 0 && entry[type] < 64;
  });
}
export function fromGamepad(pad, mapping = defaultMapping, zone = .15) {
  const state = {};
  for (const [action, binding] of Object.entries(mapping)) {
    state[action] = 'axis' in binding ? deadzone(pad?.axes[binding.axis] ?? 0, zone) : clamp(pad?.buttons[binding.button]?.value ?? 0, 0, 1);
  }
  return state;
}
export function combine(...states) {
  const result = fromGamepad(null);
  for (const state of states) for (const key of Object.keys(result)) {
    if (Math.abs(state[key] ?? 0) > Math.abs(result[key])) result[key] = clamp(state[key]);
  }
  return result;
}
