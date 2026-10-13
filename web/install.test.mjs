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
