// Authored keyboard controls card. It reads the live bindings so remapped keys
// show correctly, opens on a player's first keyboard ride, and toggles with H.
import { gameplayBindings } from './bindings.js';

export const SEEN_KEY = 'atv2-keyboard-help-seen-v1';
const NAMES = { ArrowUp: '↑', ArrowDown: '↓', ArrowLeft: '←', ArrowRight: '→', ShiftLeft: 'Shift', ShiftRight: 'R Shift', ControlLeft: 'Ctrl', ControlRight: 'R Ctrl', AltLeft: 'Alt', AltRight: 'R Alt', Escape: 'Esc', Backspace: '⌫', Enter: 'Enter', Space: 'Space', Tab: 'Tab', Slash: '/', Period: '.', Comma: ',', Semicolon: ';', Quote: "'", BracketLeft: '[', BracketRight: ']', Minus: '-', Equal: '=', Backquote: '`' };
export const keyName = code => NAMES[code] ?? code.replace(/^Key/, '').replace(/^Digit/, '').replace(/^Numpad/, 'Num ');

// Group the rows players need first; tricks explain the hold-then-tap rule.
const SECTIONS = [
  ['Drive', [['throttle', 'Gas'], ['brake', 'Brake · hold to reverse'], [['steerLeft', 'steerRight'], 'Steer']]],
  ['Rider', [['preloadHold', 'Hold, release to jump'], ['leanBack', 'Lean back · wheelie'], ['leanForward', 'Lean forward']]],
  ['Tricks', [[['stuntCircle', 'stuntTriangle'], 'In the air: hold, then tap a direction'], ['stuntL1', 'Add for combo tricks · alone + back/side: flip (Extras)']]],
  ['Other', [['reset', 'Reset'], ['camera', 'Camera'], ['pause', 'Pause']]],
];

// Returns alternative key groups. Paired rows (Steer, Tricks) group the n-th
// key of each action together, so Steer reads "A D / ← →".
export function keysFor(rows, ids) {
  const byId = new Map(rows.map(row => [row.id, row.binding]));
  const keys = [ids].flat().map(id => (byId.get(id) ?? []).map(b => (b.chord ?? [b]).map(a => keyName(a.key)).join('+')));
  const columns = Math.max(0, ...keys.map(k => k.length));
  return Array.from({ length: columns }, (_, i) => keys.map(k => k[i]).filter(Boolean));
}

export class KeyboardHelp {
  constructor({ storage, root = document.body, toolbar = document.getElementById('toolbar'), bindings = gameplayBindings } = {}) {
    if (storage === undefined) { try { storage = globalThis.localStorage; } catch { storage = null; } }
    this.storage = storage; this.bindings = bindings; this.riding = false; this.open = false;
    this.card = document.createElement('section');
    this.card.id = 'keyboard-help'; this.card.hidden = true; this.card.setAttribute('aria-label', 'Keyboard controls');
    root.append(this.card);
    if (toolbar) {
      this.button = document.createElement('button');
      Object.assign(this.button, { id: 'keyboard-help-toggle', type: 'button', className: 'atv-control', disabled: true });
      this.button.setAttribute('aria-label', 'Keyboard controls (H)');
      this.button.setAttribute('aria-expanded', 'false');
      this.button.innerHTML = '<svg viewBox="0 0 20 20" aria-hidden="true"><path d="M2 5h16v10H2zm2 2v2h2V7zm3 0v2h2V7zm3 0v2h2V7zm3 0v2h3V7zM4 10v2h2v-2zm3 0v2h6v-2zm7 0v2h2v-2z"/></svg><span>Keys</span>';
      // Keep keyboard focus on the game canvas after a mouse click.
      this.button.addEventListener('pointerdown', event => event.preventDefault());
      this.button.addEventListener('click', () => { this.toggle(); document.getElementById('game')?.focus({ preventScroll: true }); });
      toolbar.append(this.button);
    }
    bindings.subscribe(() => { if (this.open) this.render(); });
    window.addEventListener('keydown', event => {
      if (!this.riding || event.repeat || event.code !== 'KeyH' || event.ctrlKey || event.metaKey || event.altKey) return;
      if (/INPUT|SELECT|TEXTAREA/.test(event.target?.tagName ?? '') || bindings.usesKey('KeyH')) return;
      event.preventDefault(); this.toggle();
    });
  }
  seen() { try { return this.storage?.getItem(SEEN_KEY) === '1'; } catch { return false; } }
  markSeen() { try { this.storage?.setItem(SEEN_KEY, '1'); } catch { /* Private mode: shows again next visit. */ } }
  render() {
    const rows = this.bindings.rows('keyboard');
    const sections = SECTIONS.map(([title, entries]) => {
      const items = entries.map(([ids, text]) => {
        const keys = keysFor(rows, ids);
        if (!keys.length) return '';
        return `<li><span class="keys">${keys.map(group => group.map(k => `<kbd>${escape(k)}</kbd>`).join('')).join('<i>/</i>')}</span><span>${text}</span></li>`;
      }).join('');
      return `<div><h3>${title}</h3><ul>${items}</ul></div>`;
    }).join('');
    this.card.innerHTML = `<header><h2>Keyboard controls</h2><p>Press <kbd>H</kbd> to show or hide · remap in Pause → Settings → Controls</p></header><div class="keyboard-help-grid">${sections}</div>`;
  }
  show() { clearTimeout(this.timer); this.render(); this.open = true; this.card.hidden = false; this.button?.setAttribute('aria-expanded', 'true'); this.markSeen(); }
  hide() { clearTimeout(this.timer); this.open = false; this.card.hidden = true; this.button?.setAttribute('aria-expanded', 'false'); }
  toggle() { if (this.open) this.hide(); else this.show(); }
  // Called when a ride starts or the menu opens. Touch-driving devices keep
  // their own on-screen controls, so the card never auto-opens there.
  setRiding(riding, touch = document.body.classList.contains('touch-driving') || globalThis.matchMedia?.('(hover: none) and (pointer: coarse)').matches) {
    this.riding = riding;
    if (this.button) this.button.disabled = !riding;
    if (!riding) { this.hide(); return; }
    // First keyboard ride: open the card, then get out of the way.
    if (!touch && !this.seen()) { this.show(); this.timer = setTimeout(() => this.hide(), 15000); }
  }
}
const escape = text => text.replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);
