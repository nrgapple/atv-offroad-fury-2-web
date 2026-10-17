import test from 'node:test';
import assert from 'node:assert/strict';
import { createScreenWakeLock } from './wake-lock.js';

function fakes({ fail = 0 } = {}) {
  const listeners = {}, requests = [], locks = [];
  const doc = { visibilityState: 'visible', addEventListener: (type, fn) => { (listeners[type] ??= []).push(fn); } };
  const fire = type => listeners[type]?.forEach(fn => fn());
  const nav = { wakeLock: { request: async type => {
    requests.push(type);
    if (fail-- > 0) throw Object.assign(new Error('needs activation'), { name: 'NotAllowedError' });
    const handlers = [], lock = { released: false, addEventListener: (_, fn) => handlers.push(fn), release: async () => { lock.released = true; handlers.forEach(fn => fn()); } };
    locks.push(lock); return lock;
  } } };
  return { nav, doc, fire, requests, locks };
}

test('riding holds one screen lock and menus release it', async () => {
  const { nav, doc, requests, locks } = fakes();
  const wake = createScreenWakeLock(nav, doc);
  assert.equal(wake.supported, true);
  await wake.set(true); await wake.set(true);
  assert.deepEqual(requests, ['screen']);
  assert.equal(wake.held, true);
  await wake.set(false);
  assert.equal(wake.held, false);
  assert.equal(locks[0].released, true);
});

test('the lock is requested again after the page returns from the background', async () => {
  const { nav, doc, fire, requests, locks } = fakes();
  const wake = createScreenWakeLock(nav, doc);
  await wake.set(true);
  doc.visibilityState = 'hidden'; await locks[0].release(); fire('visibilitychange');
  assert.equal(wake.held, false);
  doc.visibilityState = 'visible'; fire('visibilitychange'); await new Promise(resolve => setTimeout(resolve));
  assert.equal(requests.length, 2);
  assert.equal(wake.held, true);
});

test('a refused request retries on the next gesture, and hidden or menu states never request', async () => {
  const { nav, doc, fire, requests } = fakes({ fail: 1 });
  const wake = createScreenWakeLock(nav, doc);
  await wake.set(true);
  assert.equal(wake.held, false);
  fire('pointerup'); await new Promise(resolve => setTimeout(resolve));
  assert.equal(wake.held, true);
  await wake.set(false); doc.visibilityState = 'hidden'; await wake.set(true); fire('keydown');
  assert.equal(requests.length, 2);
});

test('a lock that arrives after leaving the ride is released at once', async () => {
  const { nav, doc, locks } = fakes();
  const wake = createScreenWakeLock(nav, doc);
  const pending = wake.set(true); await wake.set(false); await pending;
  assert.equal(wake.held, false);
  assert.equal(locks[0].released, true);
});

test('browsers without the API are a no-op', async () => {
  const wake = createScreenWakeLock({}, { addEventListener() {} });
  assert.equal(wake.supported, false);
  await wake.set(true);
  assert.equal(wake.held, false);
});
