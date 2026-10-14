import assert from 'node:assert/strict';
import test from 'node:test';
import { projectedGrid, projectedGridSize } from './water.js';

test('native-derived water grid keeps the dense count on the viewport depth axis', () => {
  assert.deepEqual(projectedGridSize(640, 448), { columns: 58, rows: 334 });
  const { columns, rows } = projectedGridSize(585, 1266);
  assert.equal(columns, 58);
  assert(rows > 400);
  const { grid, indices } = projectedGrid(columns, rows);
  assert.deepEqual(Array.from(grid.slice(0, 2)), [-1, 0]);
  assert.deepEqual(Array.from(grid.slice(-2)), [1, 1]);
  assert.deepEqual(Array.from(indices.slice(0, 6)), [0, columns, 1, 1, columns, columns + 1]);
  assert.equal(grid.length / 2, columns * (rows + 1));
  assert.equal(indices.length, (columns - 1) * rows * 6);
});

test('large, tall and small viewports keep resident Uint16 geometry bounded', () => {
  for (const [width, height] of [[1, 1], [390, 844], [844, 390], [960, 540], [4096, 8192], [8192, 4096]]) {
    const { columns, rows } = projectedGridSize(width, height);
    const { grid, indices } = projectedGrid(columns, rows);
    assert(grid.length / 2 <= 24000);
    assert(indices.every(index => index < grid.length / 2));
    assert(rows >= 8);
  }
  for (const [width, height, budget] of [[585, 1266, 8000], [1266, 585, 12000]]) {
    const { columns, rows } = projectedGridSize(width, height, budget);
    assert(columns * (rows + 1) <= budget);
    assert(rows > 100);
  }
  for (const width of [0, -1, NaN, Infinity]) assert.throws(() => projectedGridSize(width, 448));
});
