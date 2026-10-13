import test from 'node:test';
import assert from 'node:assert/strict';
import { isAppleMobile, isDismissed, INSTALL_DISMISS_MS } from './install.js';

test('Home Screen help recognizes iPhone and desktop-identifying iPad, not a touchless Mac', () => {
  assert.equal(isAppleMobile({ userAgent: 'iPhone', platform: 'iPhone', maxTouchPoints: 5 }), true);
  assert.equal(isAppleMobile({ userAgent: 'Macintosh', platform: 'MacIntel', maxTouchPoints: 5 }), true);
  assert.equal(isAppleMobile({ userAgent: 'Macintosh', platform: 'MacIntel', maxTouchPoints: 0 }), false);
  assert.equal(isAppleMobile({ userAgent: 'Android', platform: 'Linux', maxTouchPoints: 5 }), false);
});
test('a dismissed banner returns after a week; inaccessible or corrupt storage does not break startup', () => {
  const now = 1000;
  assert.equal(isDismissed(() => String(now + INSTALL_DISMISS_MS), now), true);
  for (const value of [null, '', 'NaN', 'Infinity', 'garbage', String(now), String(now + INSTALL_DISMISS_MS + 1)]) assert.equal(isDismissed(() => value, now), false);
  assert.equal(isDismissed(() => { throw Error('storage blocked'); }, now), false);
});

test('installed Apple viewport fills the bounded status-bar gap and rotates without inflating browser/split-screen/keyboard views', async () => {
  const { gameViewport } = await import('./install.js');
  const portrait = { innerWidth: 390, innerHeight: 785, visualViewport: { width: 390, height: 785, scale: 1 }, screen: { width: 390, height: 844 } };
  assert.deepEqual(gameViewport(portrait, true, true), { width: 390, height: 844 });
  assert.deepEqual(gameViewport(portrait, false, true), { width: 390, height: 785 });
  assert.deepEqual(gameViewport(portrait, true, false), { width: 390, height: 785 });
  const landscape = { ...portrait, innerWidth: 844, innerHeight: 369, visualViewport: { width: 844, height: 369, scale: 1 } };
  assert.deepEqual(gameViewport(landscape, true, true), { width: 844, height: 390 });
  const split = { ...portrait, innerWidth: 300, visualViewport: { width: 300, height: 785, scale: 1 } };
  assert.deepEqual(gameViewport(split, true, true), { width: 300, height: 785 });
  const keyboard = { ...portrait, visualViewport: { width: 390, height: 500, scale: 1 } };
  assert.deepEqual(gameViewport(keyboard, true, true), { width: 390, height: 500 });
  const zoom = { ...portrait, visualViewport: { width: 195, height: 392.5, scale: 2 } };
  assert.deepEqual(gameViewport(zoom, true, true), { width: 390, height: 785 });
});
