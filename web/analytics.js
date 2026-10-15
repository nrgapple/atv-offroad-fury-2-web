import { ENVIRONMENTS } from './launch.js';
// Anonymous usage counters for Cloudflare Workers Analytics Engine. Events
// carry only the enumerated fields below: no cookies, identifiers, player
// names, room codes, URLs or free text. The Worker re-validates every event
// with normalizeEvent and adds only a coarse country and the release.
export const ENDPOINT = '/api/events';
export const MAX_EVENT_BYTES = 1024;
export const EVENT_NAMES = Object.freeze(['session_start', 'ride_start', 'ride_failed', 'ride_time', 'race_finish']);
const TEXT = {
  device: ['touch', 'desktop'], display: ['app', 'browser'], entry: ['link', 'menu'],
  mode: ['single', 'multiplayer'], activity: ['freestyle', 'race'], map: ENVIRONMENTS, vehicle: ['atv', 'motorcycle'],
};
const NUMBER = { quad: [1, 46], seconds: [0, 86400], place: [1, 20], field: [1, 20], laps: [1, 5], cpu: [0, 4], players: [1, 16] };
// Analytics Engine column order. Keep stable: queries address blobN/doubleN.
// blob1 name, blob2 country, blob3 release, then BLOB_FIELDS; double1.. DOUBLE_FIELDS.
export const BLOB_FIELDS = Object.freeze(['device', 'display', 'entry', 'mode', 'activity', 'map', 'vehicle']);
export const DOUBLE_FIELDS = Object.freeze(['quad', 'seconds', 'place', 'field', 'laps', 'cpu', 'players']);

export function normalizeEvent(raw) {
  if (!raw || typeof raw !== 'object' || Array.isArray(raw) || !EVENT_NAMES.includes(raw.name)) return null;
  const event = { name: raw.name };
  for (const [key, allowed] of Object.entries(TEXT)) if (allowed.includes(raw[key])) event[key] = raw[key];
  for (const [key, [min, max]] of Object.entries(NUMBER)) {
    const value = raw[key];
    if (typeof value === 'number' && Number.isFinite(value) && value >= min && value <= max) event[key] = key === 'seconds' ? Math.round(value * 100) / 100 : Math.round(value);
  }
  return event;
}

export function dataPoint(event, { country = '', release = '' } = {}) {
  return {
    indexes: [event.name],
    blobs: [event.name, /^[A-Z0-9]{2}$/.test(country) ? country : '', /^[a-f0-9]{7,40}$/.test(release) ? release.slice(0, 12) : '', ...BLOB_FIELDS.map(key => event[key] ?? '')],
    doubles: DOUBLE_FIELDS.map(key => event[key] ?? 0),
  };
}

// Automated browsers, local servers and visitors asking not to be tracked send nothing.
export function analyticsAllowed(scope = globalThis) {
  const nav = scope.navigator, host = scope.location?.hostname ?? '';
  if (!nav || nav.webdriver || nav.doNotTrack === '1' || nav.globalPrivacyControl === true) return false;
  return scope.location?.protocol === 'https:' && !/^(localhost|127\.|\[::1\]|.*\.local$)/.test(host);
}

export function deviceContext(scope = globalThis) {
  const media = query => { try { return Boolean(scope.matchMedia?.(query).matches); } catch { return false; } };
  return {
    device: media('(pointer: coarse)') ? 'touch' : 'desktop',
    display: media('(display-mode: standalone)') || scope.navigator?.standalone === true ? 'app' : 'browser',
  };
}

function beacon(event) {
  const body = JSON.stringify(event);
  try {
    if (globalThis.navigator?.sendBeacon?.(ENDPOINT, new Blob([body], { type: 'text/plain' }))) return;
  } catch { /* Fall back to a keepalive request. */ }
  try { void fetch(ENDPOINT, { method: 'POST', body, keepalive: true, credentials: 'omit' }).catch(() => {}); } catch { /* Analytics never affects play. */ }
}

export class UsageAnalytics {
  constructor({ send = beacon, clock = () => performance.now(), enabled = analyticsAllowed(), context = enabled ? deviceContext() : {} } = {}) {
    Object.assign(this, { send, clock, enabled, base: context });
    this.ride = null; this.active = false; this.visible = globalThis.document?.hidden !== true; this.since = null; this.seconds = 0;
  }
  track(name, fields = {}) {
    if (!this.enabled) return;
    const event = normalizeEvent({ ...this.base, ...fields, name });
    if (event) this.send(event);
  }
  // details (load seconds) describe only the start event, not later riding time.
  rideStarted(ride, details = {}) { this.report(1); this.ride = ride; this.since = null; this.seconds = 0; this.accrue(); this.track('ride_start', { ...ride, ...details }); }
  rideEnded() { this.report(1); this.ride = null; this.since = null; this.seconds = 0; }
  rideFailed(ride) { this.rideEnded(); this.track('ride_failed', ride); }
  // Riding time accrues only while a ride is open, visible and not in a menu.
  setRiding(active) { this.accrue(); this.active = active; this.accrue(); }
  setVisible(visible) { this.accrue(); this.visible = visible; this.accrue(); }
  accrue() {
    const now = this.clock();
    if (this.since != null) this.seconds += (now - this.since) / 1000;
    this.since = this.ride && this.active && this.visible ? now : null;
  }
  // Sends accumulated riding time as a delta; partial reports sum per ride.
  report(minimum = 1) {
    this.accrue();
    if (!this.ride || this.seconds < minimum) return;
    this.track('ride_time', { ...this.ride, seconds: Math.min(this.seconds, NUMBER.seconds[1]) });
    this.seconds = 0;
  }
}
