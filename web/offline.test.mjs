import test from 'node:test';
import assert from 'node:assert/strict';
import { persistStorage, recoverFailedStart, registerOffline, releaseBuild, standalone, startSucceeded, uncachedContent } from './offline.js';

const V = 'f'.repeat(40);
const media = matches => query => ({ matches: matches.includes(query) });

test('only packaged releases register the offline cache', () => {
  assert.equal(releaseBuild(`https://furyremastered.com/offline.js?v=${V}`), true);
  assert.equal(releaseBuild('http://127.0.0.1:4173/offline.js'), false);
  assert.equal(releaseBuild('http://127.0.0.1:4173/offline.js?v=dev'), false);
});

test('registration is skipped without service workers or outside a release, and failures are ignored', async () => {
  assert.equal(await registerOffline({ navigator: {} }, `https://x.test/offline.js?v=${V}`), null);
  let registered = 0;
  const serviceWorker = { register: async () => { registered++; throw new Error('blocked'); }, ready: new Promise(() => {}) };
  const win = { navigator: { serviceWorker }, document: { readyState: 'complete' }, location: { origin: 'https://x.test' }, performance: { getEntriesByType: () => [] }, setTimeout };
  assert.equal(await registerOffline(win, 'https://x.test/offline.js'), null);
  assert.equal(registered, 0);
  assert.equal(await registerOffline(win, `https://x.test/offline.js?v=${V}`), null);
  assert.equal(registered, 1);
});

test('content fetched without the worker is reported; worker-served, foreign and non-content entries are not', () => {
  const origin = 'https://furyremastered.com', blob = `/content/${'1'.repeat(64)}.bin`;
  const entries = [
    { name: origin + blob, workerStart: 0 },
    { name: origin + '/content/manifest.json', workerStart: 0 },
    { name: `${origin}/content/manifests/${V}.json` },
    { name: origin + `/content/${'2'.repeat(64)}.bin`, workerStart: 12.5 },
    { name: origin + `/content/${'3'.repeat(64)}.m4a`, workerStart: 0 },
    { name: `https://evil.test${blob}`, workerStart: 0 },
    { name: `${origin}/game.js?v=${V}`, workerStart: 0 },
    { name: 'not a url', workerStart: 0 },
  ];
  assert.deepEqual(uncachedContent(entries, origin), [origin + blob, origin + '/content/manifest.json', `${origin}/content/manifests/${V}.json`]);
});

test('persistent storage is requested only for an installed app, once, and refusal is harmless', async () => {
  let asked = 0;
  const storage = (persisted, result = true) => ({ persisted: async () => persisted, persist: async () => { asked++; if (result instanceof Error) throw result; return result; } });
  const win = (standaloneModes, store, iosStandalone = false) => ({ navigator: { storage: store, standalone: iosStandalone }, matchMedia: media(standaloneModes) });
  assert.equal(standalone(win(['(display-mode: standalone)'])), true);
  assert.equal(standalone(win(['(display-mode: fullscreen)'])), true);
  assert.equal(standalone(win([], undefined, true)), true);
  assert.equal(standalone(win([])), false);
  assert.equal(await persistStorage(win([], storage(false))), false);
  assert.equal(asked, 0, 'a browser tab never asks');
  assert.equal(await persistStorage(win(['(display-mode: standalone)'], storage(true))), true);
  assert.equal(asked, 0, 'already persistent');
  assert.equal(await persistStorage(win([], storage(false, false), true)), false);
  assert.equal(await persistStorage(win([], storage(false, new Error('denied')), true)), false);
  assert.equal(asked, 2);
  assert.equal(await persistStorage(win(['(display-mode: standalone)'], undefined)), false);
});

test('a failed start reloads once (without its service worker), then shows its error', async () => {
  const store = new Map();
  const sessionStorage = { getItem: key => store.get(key) ?? null, setItem: (key, value) => store.set(key, String(value)), removeItem: key => store.delete(key) };
  let unregistered = 0, reloads = 0;
  const registration = { unregister: async () => { unregistered++; return true; } };
  const page = controller => ({
    navigator: { serviceWorker: { controller, getRegistrations: async () => [registration] } },
    sessionStorage, location: { reload: () => { reloads++; } }
  });
  assert.equal(await recoverFailedStart(page({})), true);
  assert.deepEqual([unregistered, reloads], [1, 1], 'a controlled page drops its worker before reloading');
  assert.equal(await recoverFailedStart(page({})), false, 'a second failure in the session does not loop');
  assert.equal(reloads, 1);
  startSucceeded(page({}));
  assert.equal(await recoverFailedStart(page(null)), true, 'a successful start re-arms recovery');
  assert.deepEqual([unregistered, reloads], [1, 2], 'an uncontrolled page just reloads');
  const blocked = { ...page({}), sessionStorage: { getItem: () => { throw new Error('denied'); } } };
  assert.equal(await recoverFailedStart(blocked), false, 'without session storage it never reloads');
  assert.equal(await recoverFailedStart({ navigator: {} }), false);
});
