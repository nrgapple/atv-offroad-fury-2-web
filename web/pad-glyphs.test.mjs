import test from 'node:test';
import assert from 'node:assert/strict';
import { bindingText, inputName, standardPad, controllerSummary } from './pad-glyphs.js';
import { GameplayBindings } from './bindings.js';

// Just enough DOM for the summary builder.
class Node {
  constructor(tag) { this.tag = tag; this.children = []; this.attributes = {}; this.dataset = {}; this.className = ''; this.textContent = ''; this.innerHTML = ''; }
  append(...nodes) { this.children.push(...nodes); }
  setAttribute(name, value) { this.attributes[name] = String(value); }
  find(predicate, out = []) { if (predicate(this)) out.push(this); for (const child of this.children) child.find?.(predicate, out); return out; }
}
globalThis.document = { createElement: tag => new Node(tag) };

test('standard layout names every button and stick direction', () => {
  assert.deepEqual([0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15].map(button => inputName({button})),
    ['Cross','Circle','Square','Triangle','L1','R1','L2','R2','SELECT','START','L3','R3','D-pad up','D-pad down','D-pad left','D-pad right']);
  assert.equal(inputName({axis:0,sign:-1}), 'Left stick left');
  assert.equal(inputName({axis:1,sign:1}), 'Left stick down');
  assert.equal(inputName({axis:3,sign:-1}), 'Right stick up');
  assert.equal(bindingText([{chord:[{button:0},{button:2}]}]), 'Cross + Square');
  assert.equal(bindingText([]), 'Unbound');
});

test('non-standard pads and unknown indices keep raw names', () => {
  assert.equal(standardPad({mapping:''}), false);
  assert.equal(standardPad({mapping:'standard'}), true);
  assert.equal(standardPad(null), true);
  assert.equal(bindingText([{button:0},{axis:0,sign:-1}], false), 'Button 0 / Axis 0 -');
  assert.equal(inputName({button:20}), 'Button 20');
  assert.equal(inputName({axis:5,sign:1}), 'Axis 5 +');
});

test('summary shows only the port\'s bound actions, pairing sticks and D-pad arms', () => {
  const rows = new GameplayBindings(null).rows('controller');
  const panel = controllerSummary(rows);
  const titles = panel.find(n => n.className === 'pad-summary-title').map(n => n.textContent);
  assert.deepEqual(titles, ['ATV Controls','Stunt Controls','Camera Controls']);
  const lines = panel.find(n => n.className === 'pad-summary-row').map(n => n.attributes['aria-label']);
  assert.deepEqual(lines, [
    'START: Pause Menu', 'Left stick / D-pad: Steering', 'Left stick / D-pad: Lean', 'Cross / R2: Accelerate', 'Square / L2: Brake',
    'Cross + Square: Reverse', 'L1 + R1: Reset ATV', 'L3: Motorcycle clutch', 'Circle / Triangle: Stunt', 'L1: Stunt combo',
    'R3: Change Camera', 'Right stick: Orbit Camera',
  ]);
  assert(!lines.some(line => /Look Back|Thrill|Zoom|Dolly|Pan Camera|Preload/.test(line)), 'no original-only or unbound items');
  // An unmatched direction keeps its own action name.
  const custom = rows.map(row => row.id === 'steerRight' ? { ...row, binding: [{button:5}] } : row);
  const customLines = controllerSummary(custom).find(n => n.className === 'pad-summary-row').map(n => n.attributes['aria-label']);
  assert(customLines.includes('Left stick left / D-pad left: Steer left') && customLines.includes('R1: Steer right'));
});
