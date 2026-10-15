import test from 'node:test';
import assert from 'node:assert/strict';
import { GameplayBindings } from './bindings.js';
import { keyName, keysFor } from './keyboard-help.js';

test('controls card names keys players recognise', () => {
  assert.deepEqual(['KeyW', 'ArrowUp', 'ShiftLeft', 'Space', 'Escape', 'Digit3', 'Backspace'].map(keyName), ['W', '↑', 'Shift', 'Space', 'Esc', '3', '⌫']);
});
test('controls card follows live remaps and pairs steering keys', () => {
  const bindings = new GameplayBindings(null), rows = () => bindings.rows('keyboard');
  assert.deepEqual(keysFor(rows(), 'throttle'), [['W'], ['↑']]);
  assert.deepEqual(keysFor(rows(), ['steerLeft', 'steerRight']), [['A', 'D'], ['←', '→']]);
  assert.deepEqual(keysFor(rows(), ['stuntCircle', 'stuntTriangle']), [['J', 'K']]);
  assert.deepEqual(keysFor(rows(), 'reverse'), [], 'Unbound rows are omitted');
  bindings.setBinding('keyboard', 'throttle', [{ key: 'KeyT' }, { chord: [{ key: 'KeyG' }, { key: 'KeyH' }] }]);
  assert.deepEqual(keysFor(rows(), 'throttle'), [['T'], ['G+H']]);
});
