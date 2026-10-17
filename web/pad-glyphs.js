// Controller binding names and glyphs in the style of the original Controller
// Settings page (PAGE17: button_x, button_l1, button_dpad, button_analog,
// symbol_plus and the translucent panel*). Glyphs are redrawn as vectors from
// colours sampled from those textures; no original texture is shipped here.
//
// Names follow the browser "standard" gamepad layout, which fixes the meaning
// of each button and axis index. A pad that reports another mapping keeps the
// raw "Button n" / "Axis n" names, since its indices name no known button.

const BUTTONS = [
  ['cross','Cross'], ['circle','Circle'], ['square','Square'], ['triangle','Triangle'],
  ['l1','L1'], ['r1','R1'], ['l2','L2'], ['r2','R2'], ['select','SELECT'], ['start','START'],
  ['l3','L3'], ['r3','R3'], ['dpad-up','D-pad up'], ['dpad-down','D-pad down'],
  ['dpad-left','D-pad left'], ['dpad-right','D-pad right'], ['home','Home'],
];
const AXES = [['left','x'], ['left','y'], ['right','x'], ['right','y']];
const DIRECTIONS = { x: ['left','right'], y: ['up','down'] };

/** Standard unless the pad explicitly reports a non-standard ('') mapping. */
export function standardPad(pad) { return pad?.mapping !== ''; }

function describe(atom, standard) {
  if ('button' in atom) {
    const known = standard && BUTTONS[atom.button];
    return known ? { kind: known[0], name: known[1] } : { kind: 'raw', name: `Button ${atom.button}`, short: `B${atom.button}` };
  }
  const known = standard && AXES[atom.axis];
  if (!known) return { kind: 'raw', name: `Axis ${atom.axis} ${atom.sign < 0 ? '-' : '+'}`, short: `A${atom.axis}${atom.sign < 0 ? '-' : '+'}` };
  const [stick, plane] = known, direction = DIRECTIONS[plane][atom.sign < 0 ? 0 : 1];
  return { kind: 'stick', stick, direction, name: `${stick === 'left' ? 'Left' : 'Right'} stick ${direction}` };
}

export function inputName(atom, standard = true) { return describe(atom, standard).name; }

export function bindingText(bindings, standard = true) {
  if (!bindings.length) return 'Unbound';
  return bindings.map(b => b.chord ? b.chord.map(a => inputName(a, standard)).join(' + ') : inputName(b, standard)).join(' / ');
}

// --- glyph drawing -------------------------------------------------------
const SLATE = '#4f5a68', RIM = '#07090b', SHOULDER = '#647c9e';
const shine = '<ellipse cx="13" cy="11" rx="7" ry="4.5" fill="#fff" opacity=".12"/>';
const face = symbol => `<circle cx="16" cy="16" r="13" fill="${SLATE}" stroke="${RIM}" stroke-width="2"/>${shine}${symbol}`;
const FACE = {
  cross: face('<path d="M10.5 10.5l11 11M21.5 10.5l-11 11" stroke="#9cbde4" stroke-width="2.4" stroke-linecap="round"/>'),
  circle: face('<circle cx="16" cy="16" r="6.4" fill="none" stroke="#d888b8" stroke-width="2.3"/>'),
  square: face('<rect x="10.4" y="10.4" width="11.2" height="11.2" fill="none" stroke="#b890c8" stroke-width="2.2"/>'),
  triangle: face('<path d="M16 9.6l6.6 11.6H9.4z" fill="none" stroke="#58c0c0" stroke-width="2.2" stroke-linejoin="round"/>'),
};
const shoulder = (label, trigger) => (trigger
  ? `<path d="M4 6h24v14c0 5-5 8-12 8S4 25 4 20z" fill="#14171b" stroke="${RIM}" stroke-width="1.5"/><path d="M6.5 8h19v11.5c0 3.6-4 6-9.5 6s-9.5-2.4-9.5-6z" fill="${SHOULDER}"/>`
  : `<rect x="3" y="7" width="26" height="19" rx="3" fill="#14171b" stroke="${RIM}" stroke-width="1.5"/><rect x="5.5" y="9" width="21" height="14.5" rx="2" fill="${SHOULDER}"/><rect x="6.5" y="9.8" width="19" height="3" rx="1.5" fill="#fff" opacity=".18"/>`)
  + `<text x="16" y="${trigger ? 21.5 : 20.6}" text-anchor="middle" font-size="12" font-weight="800" font-family="system-ui, sans-serif" fill="#fff" stroke="#111" stroke-width=".6" paint-order="stroke">${label}</text>`;
const DPAD_ARMS = { up: 'M12.5 3.5h7v9h-7z', down: 'M12.5 19.5h7v9h-7z', left: 'M3.5 12.5h9v7h-9z', right: 'M19.5 12.5h9v7h-9z' };
const dpad = lit => `<path d="M12 3h8v9h9v8h-9v9h-8v-9H3v-8h9z" fill="#68728a" stroke="${RIM}" stroke-width="1.6" stroke-linejoin="round"/>`
  + (lit ? `<path d="${DPAD_ARMS[lit]}" fill="#c4d6ef"/>` : '<circle cx="16" cy="16" r="2.4" fill="#4b5466"/>');
const STICK_ARROWS = { up: 'M16 1.5l4 4.5h-8z', down: 'M16 30.5l4-4.5h-8z', left: 'M1.5 16l4.5-4v8z', right: 'M30.5 16l-4.5-4v8z' };
// Top view of the original's analog stick: a ridged cap in a dark well.
const stick = (letter, direction) => `<circle cx="16" cy="16" r="12" fill="#1a1e24" stroke="${RIM}" stroke-width="1.5"/>`
  + `<circle cx="16" cy="16" r="8.6" fill="${SLATE}" stroke="#7d8da3" stroke-width="1"/>`
  + `<text x="16" y="20.2" text-anchor="middle" font-size="11" font-weight="800" font-family="system-ui, sans-serif" fill="#e6eef8">${letter}</text>`
  + (direction ? `<path d="${STICK_ARROWS[direction]}" fill="#c4d6ef" stroke="${RIM}" stroke-width=".8"/>` : '');
function svg(info) {
  switch (info.kind) {
    case 'cross': case 'circle': case 'square': case 'triangle': return FACE[info.kind];
    case 'l1': case 'r1': return shoulder(info.name, false);
    case 'l2': case 'r2': return shoulder(info.name, true);
    case 'start': return `<path d="M7 7.5l19 8.5-19 8.5z" fill="${SLATE}" stroke="${RIM}" stroke-width="1.6" stroke-linejoin="round"/>`;
    case 'select': return `<rect x="5" y="11" width="22" height="10" rx="5" fill="${SLATE}" stroke="${RIM}" stroke-width="1.6"/>`;
    case 'l3': case 'r3': return stick(info.name, null);
    case 'stick': return stick(info.stick === 'left' ? 'L' : 'R', info.direction);
    case 'stick-pair': return stick(info.stick === 'left' ? 'L' : 'R', null) + `<path d="${info.plane === 'x' ? STICK_ARROWS.left + STICK_ARROWS.right : STICK_ARROWS.up + STICK_ARROWS.down}" fill="#c4d6ef" stroke="${RIM}" stroke-width=".8"/>`;
    case 'dpad': return dpad(null);
    case 'home': return `<circle cx="16" cy="16" r="11" fill="${SLATE}" stroke="${RIM}" stroke-width="1.6"/><path d="M10 17l6-5.5 6 5.5v5h-12z" fill="#c4d6ef"/>`;
    default:
      if (info.kind.startsWith('dpad-')) return dpad(info.kind.slice(5));
      return `<rect x="1.5" y="7" width="29" height="18" rx="4" fill="#2b3139" stroke="${RIM}" stroke-width="1.5"/><text x="16" y="20" text-anchor="middle" font-size="${info.short.length > 3 ? 8.5 : 10}" font-weight="800" font-family="system-ui, sans-serif" fill="#e6eef8">${info.short}</text>`;
  }
}

function node(tag, className, text) {
  const element = document.createElement(tag); element.className = className;
  if (text !== undefined) element.textContent = text;
  return element;
}
function glyphNode(info) {
  const element = node('span', `pad-glyph pad-glyph-${info.kind}`);
  element.dataset.glyph = info.kind; element.title = info.name;
  element.innerHTML = `<svg viewBox="0 0 32 32" aria-hidden="true" focusable="false">${svg(info)}</svg>`;
  return element;
}
function chordNode(atoms, standard) {
  const group = node('span', 'pad-chord');
  atoms.forEach((atom, i) => {
    if (i) group.append(node('span', 'pad-plus', '+'));
    group.append(glyphNode(describe(atom, standard)));
  });
  return group;
}

/** Glyphs for one action's alternatives: chords joined by the original "+". */
export function bindingGlyphs(bindings, standard = true) {
  const row = node('span', 'pad-binding');
  row.setAttribute('aria-hidden', 'true');
  if (!bindings.length) { row.append(node('span', 'pad-unbound', 'Unbound')); return row; }
  bindings.forEach((binding, i) => {
    if (i) row.append(node('span', 'pad-or', '/'));
    row.append(chordNode(binding.chord ?? [binding], standard));
  });
  return row;
}

// --- original-style summary panel ---------------------------------------
// Only actions the port really has. The original's Look Back, Thrill, Pan,
// Dolly and Zoom cameras and its four fixed presets are deliberately absent.
const GROUPS = [
  ['ATV Controls', [
    ['pause', 'Pause Menu'], [['steerLeft','steerRight'], 'Steering'], [['leanForward','leanBack'], 'Lean'],
    ['throttle', 'Accelerate'], ['brake', 'Brake'], ['reverse', 'Reverse'], ['reset', 'Reset ATV'],
    ['preloadHold', 'Preload'], ['clutch', 'Motorcycle clutch'],
  ]],
  ['Stunt Controls', [['stuntCircle', 'Stunt'], ['stuntTriangle', 'Stunt'], ['stuntL1', 'Stunt combo']]],
  ['Camera Controls', [['camera', 'Change Camera'], [['cameraLeft','cameraRight'], 'Orbit Camera']]],
];
const single = binding => !binding.chord;
// A matched pair (one stick axis both ways, or opposite D-pad arms) collapses
// to one glyph, as the original shows one D-pad and one stick for Steering.
function pairEntries(first, second, standard) {
  const entries = [], used = new Set();
  for (const a of first) {
    const match = single(a) && second.findIndex((b, j) => !used.has(j) && single(b) && (
      ('axis' in a && 'axis' in b && a.axis === b.axis && a.sign === -b.sign) ||
      (standard && 'button' in a && 'button' in b && [[14,15],[15,14],[12,13],[13,12]].some(([x,y]) => a.button === x && b.button === y))));
    if (match !== false && match >= 0) {
      used.add(match);
      if ('axis' in a) {
        const info = describe(a, standard);
        entries.push(info.kind === 'stick' ? { glyphs: [{ kind: 'stick-pair', stick: info.stick, plane: AXES[a.axis][1], name: `${info.stick === 'left' ? 'Left' : 'Right'} stick` }] } : { glyphs: [info, describe(second[match], standard)] });
      } else entries.push({ glyphs: [{ kind: 'dpad', name: 'D-pad' }] });
    } else entries.push({ suffix: 0, glyphs: (a.chord ?? [a]).map(x => describe(x, standard)) });
  }
  second.forEach((b, j) => { if (!used.has(j)) entries.push({ suffix: 1, glyphs: (b.chord ?? [b]).map(x => describe(x, standard)) }); });
  return entries;
}

/** Original-style panel built from the live controller profile. */
export function controllerSummary(rows, standard = true) {
  const byId = Object.fromEntries(rows.map(row => [row.id, row]));
  const panel = node('aside', 'pad-summary');
  panel.setAttribute('aria-label', 'Controller layout');
  for (const [title, items] of GROUPS) {
    // One line per label; its alternatives are joined by "/" (× / R2 Accelerate).
    const lines = new Map(), add = (glyphs, label) => { if (!lines.has(label)) lines.set(label, []); lines.get(label).push(glyphs); };
    for (const [ids, label] of items) {
      if (Array.isArray(ids)) {
        const [a, b] = ids.map(id => byId[id]);
        if (!a || !b) continue;
        for (const entry of pairEntries(a.binding, b.binding, standard)) add(entry.glyphs, entry.suffix === undefined ? label : (entry.suffix ? b : a).label);
      } else {
        const row = byId[ids];
        if (!row) continue;
        for (const binding of row.binding) add((binding.chord ?? [binding]).map(x => describe(x, standard)), label);
      }
    }
    if (!lines.size) continue;
    const section = node('section', 'pad-summary-group');
    section.append(node('h2', 'pad-summary-title', title));
    for (const [label, alternatives] of lines) {
      const line = node('p', 'pad-summary-row'), glyphs = node('span', 'pad-binding');
      glyphs.setAttribute('aria-hidden', 'true');
      alternatives.forEach((chord, i) => {
        if (i) glyphs.append(node('span', 'pad-or', '/'));
        const group = node('span', 'pad-chord');
        chord.forEach((info, j) => { if (j) group.append(node('span', 'pad-plus', '+')); group.append(glyphNode(info)); });
        glyphs.append(group);
      });
      line.append(glyphs, node('span', 'pad-summary-label', label));
      line.setAttribute('aria-label', `${alternatives.map(chord => chord.map(info => info.name).join(' + ')).join(' / ')}: ${label}`);
      section.append(line);
    }
    panel.append(section);
  }
  return panel;
}
