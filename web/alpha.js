// Settings → Alpha: experimental authored toggles the user tries and rates.
// Every entry is off by default and removing one removes its menu row. Only
// Arcade flips changes the simulation (authored air rotation while it is on).
export const ALPHA_FEATURES = [
  { id: 'air-slowmo', label: 'Air slow-mo' },
  { id: 'big-air-camera', label: 'Big-air camera' },
  { id: 'music-muffle', label: 'Speed & air muffle' },
  { id: 'air-blur', label: 'Air dark blur' },
  { id: 'landing-burst', label: 'Landing burst' },
  { id: 'heavy-mud', label: 'Heavy mud' },
  { id: 'arcade-flips', label: 'Arcade flips' },
];

const KEY = 'atv-alpha';

export class AlphaSettings {
  constructor(storage, features = ALPHA_FEATURES) {
    if (storage === undefined) {
      try { storage = globalThis.localStorage; } catch { storage = null; }
    }
    this.storage = storage; this.features = features; this.on = new Set();
    try {
      const saved = JSON.parse(storage?.getItem(KEY) ?? '[]');
      if (Array.isArray(saved)) for (const id of saved) if (features.some(f => f.id === id)) this.on.add(id);
    } catch { /* Session settings. */ }
  }
  enabled(id) { return this.on.has(id); }
  toggle(id) {
    if (!this.features.some(f => f.id === id)) return;
    if (this.on.has(id)) this.on.delete(id); else this.on.add(id);
    try { this.storage?.setItem(KEY, JSON.stringify([...this.on])); } catch { /* Session settings. */ }
    this.onChange?.(id, this.on.has(id));
  }
  // Menu request rows: { id, label, value } with an On/Off value.
  rows() { return this.features.map(f => ({ id: f.id, label: f.label, value: this.on.has(f.id) ? 'On' : 'Off' })); }
}
