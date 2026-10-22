import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

const ORIGIN = 'https://fury.test';
const V = 'a'.repeat(40), OLD = 'b'.repeat(40), NEW = 'c'.repeat(40);
const SOURCE = readFileSync(new URL('./sw.js', import.meta.url), 'utf8');
const sha = text => createHash('sha256').update(text).digest('hex');

// Same-origin network responses are 'basic', and so are their clones.
class BasicResponse extends Response {
  get type() { return 'basic'; }
  clone() { return Object.setPrototypeOf(super.clone(), BasicResponse.prototype); }
}
class FakeCache {
  map = new Map();
  key(request) { return new URL(typeof request === 'string' ? request : request.url, ORIGIN).href; }
  async match(request) { return this.map.get(this.key(request))?.clone(); }
  async put(request, response) {
    const body = await response.arrayBuffer();
    this.map.set(this.key(request), new Response(body, { status: response.status, headers: response.headers }));
  }
  async delete(request) { return this.map.delete(this.key(request)); }
  async keys() { return [...this.map.keys()].map(url => ({ url })); }
}
class FakeStorage {
  caches = new Map();
  async open(name) { if (!this.caches.has(name)) this.caches.set(name, new FakeCache()); return this.caches.get(name); }
  async has(name) { return this.caches.has(name); }
  async delete(name) { return this.caches.delete(name); }
  async keys() { return [...this.caches.keys()]; }
  async match(request) { for (const cache of this.caches.values()) { const hit = await cache.match(request); if (hit) return hit; } }
  urls(name) { return [...(this.caches.get(name)?.map.keys() ?? [])].map(href => href.slice(ORIGIN.length)); }
}

// A service worker global over an in-memory site. `files` maps path+query to body text or {status, body}.
function worker({ version = V, files = {} } = {}) {
  const handlers = {}, requests = [], storage = new FakeStorage();
  const network = { offline: false, hang: false };
  const self = { location: { origin: ORIGIN }, addEventListener: (type, fn) => { handlers[type] = fn; }, clients: { claimed: 0, claim: async () => { self.clients.claimed++; } } };
  async function fetch(input) {
    const url = new URL(typeof input === 'string' ? input : input.url, ORIGIN);
    requests.push(url.pathname + url.search);
    if (network.hang) return new Promise(() => {});
    if (network.offline) throw new TypeError('Failed to fetch');
    const page = url.pathname === '/' ? '/index.html' : url.pathname;
    const file = files[url.pathname + url.search] ?? (page.endsWith('.html') ? files[page] : undefined);
    const { status = 200, body = '' } = file === undefined ? { status: 404, body: 'Not found' } : typeof file === 'string' ? { body: file } : file;
    return new BasicResponse(body, { status });
  }
  const source = version ? SOURCE.replace("'game.js'", `'game.js?v=${version}'`) : SOURCE;
  const context = vm.createContext({ self, caches: storage, fetch, URL, Response, crypto, setTimeout, clearTimeout, console });
  vm.runInContext(source, context);
  context.PRUNE_DELAY_MS = 0;
  const settle = async waits => { for (let i = 0; i < waits.length; i++) await waits[i].catch?.(() => {}); };
  async function lifecycle(type, data) {
    const waits = [];
    handlers[type]({ data, waitUntil: promise => waits.push(promise) });
    for (let i = 0; i < waits.length; i++) await waits[i];
  }
  async function request(path, { method = 'GET', mode = 'cors', headers = {} } = {}) {
    const waits = [];
    let responded = null;
    const event = { request: { url: new URL(path, ORIGIN).href, method, mode, headers: new Headers(headers) }, waitUntil: promise => waits.push(promise), respondWith: promise => { responded = promise; } };
    handlers.fetch(event);
    const response = responded && await responded;
    // A hung network never settles its cache write; do not wait on it forever.
    await Promise.race([settle(waits), new Promise(resolve => setTimeout(resolve, 300))]);
    return { intercepted: Boolean(responded), response, text: response ? await response.text() : null };
  }
  return { context, storage, requests, network, files, self, request, lifecycle };
}

function site(version = V) {
  const v = `?v=${version}`;
  return {
    '/play.html': `<link rel="manifest" href="/manifest.webmanifest"><link rel="icon" href="/fury-remastered-home-192-v1.png"><link rel="stylesheet" href="game.css${v}"><script type="module" src="game.js${v}"></script>`,
    '/index.html': `<script src="landing.js${v}" defer></script>`,
    '/engine.html': `<link rel="manifest" href="/manifest.webmanifest"><script type="module" src="engine.js${v}"></script>`,
    [`/engine.js${v}`]: `import init from './atv_app.js${v}';`,
    [`/atv_app.js${v}`]: `new URL('atv_app_bg.wasm${v}', import.meta.url);`,
    [`/atv_app_bg.wasm${v}`]: '\0asm',
    [`/game.js${v}`]: `import { hud } from './hud.js${v}';\nnew Worker('worker.js${v}');\nconst img = 'fury-remastered.svg';\nconst unused = 'missing.js${v}';
fetch('/content/manifest.json');`,
    [`/hud.js${v}`]: 'export const hud = 1;',
    [`/worker.js${v}`]: `fetch('atv_web.wasm${v}')`,
    [`/atv_web.wasm${v}`]: '\0asm',
    [`/game.css${v}`]: 'body{}',
    [`/landing.js${v}`]: 'location.replace("play.html")',
    '/manifest.webmanifest': JSON.stringify({ icons: [{ src: '/fury-remastered-home-512-v1.png' }] }),
    '/fury-remastered-home-192-v1.png': 'png192',
    '/fury-remastered-home-512-v1.png': 'png512',
    '/fury-remastered.svg': '<svg/>',
    '/content/manifest.json': '{"files":[]}',
  };
}

test('routes cache only app shell, pages, manifests and content blobs; everything else passes through', () => {
  const { context } = worker();
  const route = (path, method = 'GET', mode = 'cors', ranged = false) => context.route(method, new URL(path, ORIGIN).href, mode, ranged, ORIGIN);
  const blob = `/content/${'d'.repeat(64)}.bin`;
  assert.equal(route('/play.html?map=env01', 'GET', 'navigate'), 'page');
  assert.equal(route('/', 'GET', 'navigate'), 'page');
  assert.equal(route('/devlog.html', 'GET', 'navigate'), 'page');
  assert.equal(route('/workbench.html', 'GET', 'navigate'), null);
  assert.equal(route(`/game.js?v=${V}`), 'shell');
  assert.equal(route(`/atv_web.wasm?v=${OLD}`), 'shell');
  assert.equal(route('/game.js'), null);
  assert.equal(route('/game.js?v=dev'), null);
  assert.equal(route(blob), 'blob');
  assert.equal(route('/content/manifest.json'), 'manifest');
  assert.equal(route(`/content/manifests/${V}.json`), 'manifest');
  assert.equal(route('/manifest.webmanifest'), 'static');
  assert.equal(route('/fury-remastered-home-180-v1.png'), 'static');
  assert.equal(route('/fury-remastered-maskable-512-v1.png'), 'static');
  assert.equal(route('/favicon.ico'), 'static');
  assert.equal(route(blob, 'GET', 'cors', true), null, 'ranged reads pass through');
  assert.equal(route(`/content/${'d'.repeat(64)}.m4a`), null, 'soundtrack streams pass through');
  assert.equal(route(blob, 'POST'), null);
  for (const path of ['/multiplayer/freeride/riders', '/multiplayer/rooms', '/api/events', '/devlog.json', '/devlog-images/a.png', '/deployment.json', `${blob}?x=1`])
    assert.equal(route(path), null, path);
  assert.equal(context.route('GET', 'https://static.cloudflareinsights.com/beacon.min.js', 'no-cors', false, ORIGIN), null);
});

test('page versions, shell references and manifest blob sets are parsed strictly', () => {
  const { context } = worker();
  assert.equal(context.pageVersion(site()['/play.html']), V);
  assert.equal(context.pageVersion(`<script src="a.js?v=${V}"></script><script src="b.js?v=${OLD}"></script>`), null);
  assert.equal(context.pageVersion('<script src="game.js"></script>'), null);
  const refs = context.references(site()[`/game.js?v=${V}`], `${ORIGIN}/game.js?v=${V}`);
  assert.deepEqual([...refs].sort(), [`${ORIGIN}/fury-remastered.svg`, `${ORIGIN}/hud.js?v=${V}`, `${ORIGIN}/missing.js?v=${V}`, `${ORIGIN}/worker.js?v=${V}`]);
  const a = `/content/${'1'.repeat(64)}.bin`, b = `/content/${'2'.repeat(64)}.bin`, c = `/content/${'3'.repeat(64)}.bin`;
  const referenced = context.manifestBlobs({ files: [{ url: a }, { chunks: [{ url: b }] }] });
  assert.deepEqual(Object.keys(referenced).sort(), [a, b]);
  assert.deepEqual(context.unreferenced([ORIGIN + a, ORIGIN + c], referenced), [ORIGIN + c]);
  for (const bad of [null, {}, { files: [] }, { files: [{ url: '/elsewhere.bin' }] }, { files: [{ chunks: [] }] }]) assert.equal(context.manifestBlobs(bad), null);
});

test('an unversioned development worker intercepts nothing', async () => {
  const sw = worker({ version: null, files: site() });
  await sw.lifecycle('install');
  assert.deepEqual(await sw.storage.keys(), []);
  assert.equal((await sw.request('/play.html', { mode: 'navigate' })).intercepted, false);
  assert.equal((await sw.request(`/content/${'1'.repeat(64)}.bin`)).intercepted, false);
});

test('install precaches the entry pages and their whole versioned shell, then marks it complete', async () => {
  const sw = worker({ files: site() });
  await sw.lifecycle('install');
  const shell = sw.storage.urls(`fury-shell-v1-${V}`).sort();
  assert.deepEqual(shell, ['/__fury-offline/shell-complete', '/fury-remastered-home-192-v1.png', '/fury-remastered-home-512-v1.png', '/fury-remastered.svg',
    `/game.css?v=${V}`, `/game.js?v=${V}`, `/hud.js?v=${V}`, `/landing.js?v=${V}`, '/manifest.webmanifest', `/worker.js?v=${V}`, `/atv_web.wasm?v=${V}`,
    `/engine.js?v=${V}`, `/atv_app.js?v=${V}`, `/atv_app_bg.wasm?v=${V}`].sort());
  assert.deepEqual(sw.storage.urls('fury-pages-v1').sort(), ['/engine.html', '/index.html', '/play.html']);
  assert.ok(sw.requests.includes(`/missing.js?v=${V}`), 'a quoted non-file is tried and skipped on 404');
  assert.deepEqual(sw.storage.urls('fury-manifests-v1'), ['/content/manifest.json'], 'the content manifest a module fetches is cached too');
  const unpublished = site();
  delete unpublished['/content/manifest.json'];
  const early = worker({ files: unpublished });
  await early.lifecycle('install');
  assert.equal(await early.context.shellComplete(V), true, 'a not-yet-published content manifest does not fail install');
});

test('install fails when the pages are from another deploy or a shell file fails', async () => {
  await assert.rejects(worker({ files: site(NEW) }).lifecycle('install'), /not version/);
  const files = site();
  files[`/hud.js?v=${V}`] = { status: 503, body: 'down' };
  const sw = worker({ files });
  await assert.rejects(sw.lifecycle('install'), /hud\.js.*503/);
  assert.equal(await sw.context.shellComplete(V), false, 'a partial shell is never marked complete');
  assert.deepEqual(sw.storage.urls('fury-pages-v1'), [], 'pages are written only after the shell');
});

test('activate deletes other deploys’ shells and manifests but keeps the shell the cached pages name, then claims', async () => {
  const sw = worker({ files: site() });
  await sw.lifecycle('install');
  for (const version of [OLD, NEW]) await (await sw.storage.open(`fury-shell-v1-${version}`)).put('/game.js?v=' + version, new Response('x'));
  await (await sw.storage.open('fury-legacy')).put('/x', new Response('x'));
  const manifests = await sw.storage.open('fury-manifests-v1');
  for (const version of [V, OLD]) await manifests.put(`/content/manifests/${version}.json`, new Response('{}'));
  await manifests.put('/content/manifest.json', new Response('{}'));
  // A newer deploy's page reached the cache (its own worker installed that shell).
  await (await sw.storage.open('fury-pages-v1')).put('/play.html', new Response(site(NEW)['/play.html']));
  await (await sw.storage.open('fury-pages-v1')).put('/devlog.html', new Response(`<script src="devlog.js?v=${OLD}"></script>`));
  await sw.lifecycle('activate');
  assert.deepEqual((await sw.storage.keys()).sort(), ['fury-manifests-v1', 'fury-pages-v1', `fury-shell-v1-${NEW}`, `fury-shell-v1-${V}`].sort());
  assert.deepEqual(sw.storage.urls('fury-manifests-v1').sort(), ['/content/manifest.json', `/content/manifests/${V}.json`]);
  assert.deepEqual(sw.storage.urls('fury-pages-v1').sort(), ['/engine.html', '/index.html', '/play.html'], 'a page naming a deleted shell is dropped');
  assert.equal(sw.self.clients.claimed, 1);
});

test('versioned shell is cache-first, and only this deploy’s version is written from the network', async () => {
  const sw = worker({ files: { ...site(), [`/game.js?v=${OLD}`]: 'current bytes under an old URL' } });
  await sw.lifecycle('install');
  sw.requests.length = 0;
  sw.network.offline = true;
  assert.equal((await sw.request(`/hud.js?v=${V}`)).text, 'export const hud = 1;');
  sw.network.offline = false;
  await sw.request(`/game.js?v=${OLD}`);
  await sw.request(`/game.js?v=${OLD}`);
  assert.deepEqual(sw.requests, [`/game.js?v=${OLD}`, `/game.js?v=${OLD}`]);
  assert.equal(await sw.storage.has(`fury-shell-v1-${OLD}`), false);
});

test('content blobs are cache-first, stored only when their bytes match their hash', async () => {
  const good = 'verified bytes', path = `/content/${sha(good)}.bin`, wrong = `/content/${sha('other')}.bin`;
  const sw = worker({ files: { [path]: good, [wrong]: 'tampered', [`/content/${'e'.repeat(64)}.bin`]: { status: 500, body: 'x' } } });
  assert.equal((await sw.request(path)).text, good);
  sw.network.offline = true;
  assert.equal((await sw.request(path)).text, good, 'served from cache offline');
  assert.equal((await sw.request(wrong)).response.type, 'error', 'an offline cache miss is a network error, not a rejected respondWith');
  sw.network.offline = false;
  await sw.request(wrong);
  await sw.request(`/content/${'e'.repeat(64)}.bin`);
  assert.deepEqual(sw.storage.urls('fury-content-v1'), [path]);
});

test('pages are network-first with offline and slow-network fallback to a complete cached shell', async () => {
  const sw = worker({ files: site() });
  await sw.lifecycle('install');
  sw.files['/play.html'] = site()['/play.html'] + '<!-- updated -->';
  assert.match((await sw.request('/play.html?map=env01', { mode: 'navigate' })).text, /updated/);
  assert.match(await (await (await sw.storage.open('fury-pages-v1')).match('/play.html')).text(), /updated/, 'network copy refreshed the cache');
  sw.network.offline = true;
  assert.match((await sw.request('/play.html?play=1', { mode: 'navigate' })).text, /updated/);
  sw.network.offline = false;
  sw.network.hang = true;
  sw.context.PAGE_TIMEOUT_MS = 10;
  const slow = sw.request('/', { mode: 'navigate' });
  assert.match((await Promise.race([slow, new Promise(resolve => setTimeout(resolve, 1000, { text: 'timeout' }))])).text, /landing\.js/);
  sw.network.hang = false;
  // A newer deploy's page is served but not cached until its own shell is complete.
  sw.files['/play.html'] = site(NEW)['/play.html'];
  assert.match((await sw.request('/play.html', { mode: 'navigate' })).text, new RegExp(NEW));
  assert.match(await (await (await sw.storage.open('fury-pages-v1')).match('/play.html')).text(), new RegExp(V));
  sw.network.offline = true;
  assert.equal((await sw.request('/devlog.html', { mode: 'navigate' })).response.type, 'error', 'no cached copy: a network error');
});

test('a fresh content manifest is cached and prunes blobs it no longer references', async () => {
  const keep = `/content/${'1'.repeat(64)}.bin`, drop = `/content/${'2'.repeat(64)}.bin`, url = `/content/manifests/${V}.json`;
  const sw = worker({ files: { [url]: JSON.stringify({ files: [{ chunks: [{ url: keep }] }] }) } });
  const content = await sw.storage.open('fury-content-v1');
  for (const path of [keep, drop]) await content.put(path, new Response('x'));
  assert.match((await sw.request(url)).text, /chunks/);
  assert.deepEqual(sw.storage.urls('fury-content-v1'), [keep]);
  sw.network.offline = true;
  assert.match((await sw.request(url)).text, /chunks/, 'offline falls back to the cached manifest');
  sw.files[url] = '{"files":[]}';
  sw.network.offline = false;
  await sw.request(url);
  assert.deepEqual(sw.storage.urls('fury-content-v1'), [keep], 'an unusable manifest prunes nothing');
});

test('the page can report content fetched before the worker took control; other URLs are ignored', async () => {
  const body = 'menu archive', blob = `/content/${sha(body)}.bin`;
  const sw = worker({ files: { [blob]: body, '/content/manifest.json': '{"files":[]}' } });
  await sw.lifecycle('message', { type: 'fury-warm', urls: [ORIGIN + blob, '/content/manifest.json', 'https://evil.test/content/x.bin', '/api/events', '/game.js', 42] });
  assert.deepEqual(sw.requests, [blob, '/content/manifest.json']);
  assert.deepEqual(sw.storage.urls('fury-content-v1'), [blob]);
  assert.deepEqual(sw.storage.urls('fury-manifests-v1'), ['/content/manifest.json']);
  await sw.lifecycle('message', { type: 'fury-warm', urls: [blob] });
  assert.equal(sw.requests.length, 2, 'already cached content is not fetched again');
});

test('streams, ranged reads, multiplayer, analytics and non-GET requests are never intercepted', async () => {
  const sw = worker({ files: site() });
  const blob = `/content/${'1'.repeat(64)}.bin`;
  for (const [path, options] of [[`/content/${'1'.repeat(64)}.m4a`, {}], [blob, { headers: { Range: 'bytes=0-9' } }], ['/multiplayer/freeride/riders', {}],
    ['/multiplayer/rooms', { method: 'POST' }], ['/api/events', { method: 'POST' }], ['/devlog.json', {}], [blob, { method: 'HEAD' }]])
    assert.equal((await sw.request(path, options)).intercepted, false, path);
});
