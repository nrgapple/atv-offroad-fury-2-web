// Fury Remastered offline cache. Design, cache names and update rules: research/OFFLINE.md.
// Classic (non-module) service worker so every browser with service workers runs it.
//
// Packaging rewrites the quoted module name below to 'engine.js?v=<commit>'. That
// names this deploy's shell cache and changes these bytes on every deploy, which
// is what makes browsers install the new worker. An unversioned development build
// leaves VERSION null and the worker intercepts nothing (offline.js does not
// register it there either).
var VERSION_PROBE = 'engine.js';
var VERSION = (/\?v=([a-f0-9]{40})$/.exec(VERSION_PROBE) || [])[1] || null;

var SHELL_PREFIX = 'fury-shell-v1-';
var PAGES = 'fury-pages-v1';
var CONTENT = 'fury-content-v1';
var MANIFESTS = 'fury-manifests-v1';
var CACHES = [PAGES, CONTENT, MANIFESTS];
// Written last by install; a shell cache without it is incomplete.
var COMPLETE = '/__fury-offline/shell-complete';
var PAGE_TIMEOUT_MS = 4000;
// Network attempts for a cache-first file the worker answers itself (see fetchWhole).
var FETCH_ATTEMPTS = 2;
var PRUNE_DELAY_MS = 20000;
var MAX_SHELL_FILES = 400;

var PAGE_PATHS = ['/', '/index.html', '/play.html', '/engine.html', '/devlog.html'];
// play.html is the Rust engine page (its module graph includes atv_app_bg.wasm);
// engine.html is the same page under its pre-switch name.
var PRECACHE_PAGES = ['/play.html', '/index.html', '/engine.html'];
var BLOB = /^\/content\/([a-f0-9]{64})\.bin$/;
var MANIFEST = /^\/content\/(?:manifest\.json|manifests\/([a-f0-9]{40})\.json)$/;
var ASSET = /^\/[a-z0-9_-]+\.(?:js|css|wasm)$/;
var VERSIONED = /^\?v=([a-f0-9]{40})$/;
// Unversioned shell files whose bytes may change between deploys.
var STATIC = /^\/(?:manifest\.webmanifest|favicon\.ico|fury-remastered\.svg|fury-remastered-(?:home|maskable)-\d+-v\d+\.png|apple-touch-icon(?:-precomposed)?\.png)$/;

// Which strategy handles a request, or null to leave it to the network untouched:
// non-GET, ranged (soundtrack .m4a streams), cross-origin, /multiplayer/, /api/,
// devlog.json, devlog images and anything else unlisted.
function route(method, href, mode, ranged, origin) {
  if (method !== 'GET' || ranged) return null;
  var url = new URL(href);
  if (url.origin !== origin) return null;
  var path = url.pathname;
  if (mode === 'navigate') return PAGE_PATHS.indexOf(path) >= 0 ? 'page' : null;
  if (url.search) return ASSET.test(path) && VERSIONED.test(url.search) ? 'shell' : null;
  if (BLOB.test(path)) return 'blob';
  if (MANIFEST.test(path)) return 'manifest';
  if (STATIC.test(path)) return 'static';
  return null;
}

function cacheable(response) {
  return Boolean(response) && response.status === 200 && response.type === 'basic' && !response.redirected;
}

// The commit a page's versioned references name, or null when it has none or mixes versions.
function pageVersion(html) {
  var versions = {};
  var pattern = /\?v=([a-f0-9]{40})\b/g, match;
  while ((match = pattern.exec(html))) versions[match[1]] = true;
  var found = Object.keys(versions);
  return found.length === 1 ? found[0] : null;
}

function pageKey(path) { return path === '/' ? '/index.html' : path; }

// Same-origin shell files a page, module or stylesheet names: versioned
// js/css/wasm (exactly what packaging rewrites) and the static icons/manifest.
function references(text, base) {
  var found = [], match;
  var versioned = /(["'])(?:\.\/)?([a-z0-9_-]+\.(?:js|css|wasm)\?v=[a-f0-9]{40})\1/g;
  var fixed = /(["'])((?:\.?\/)?[a-z0-9_-]+(?:[.-][a-z0-9]+)*\.(?:png|svg|ico|webmanifest))\1/g;
  while ((match = versioned.exec(text))) found.push(new URL(match[2], base).href);
  while ((match = fixed.exec(text))) found.push(new URL(match[2], base).href);
  return found;
}

// Content manifests a module fetches by literal path (content.js).
function manifestReferences(text, base) {
  var found = [], match, pattern = /["'](\/content\/(?:manifest\.json|manifests\/[a-f0-9]{40}\.json))["']/g;
  while ((match = pattern.exec(text))) found.push(new URL(match[1], base).href);
  return found;
}

// Blob paths a hosted content manifest references, or null if it is not a usable manifest.
function manifestBlobs(manifest) {
  if (!manifest || !Array.isArray(manifest.files) || !manifest.files.length) return null;
  var paths = {};
  for (var i = 0; i < manifest.files.length; i++) {
    var record = manifest.files[i], chunks = record && (record.chunks || [record]);
    if (!Array.isArray(chunks) || !chunks.length) return null;
    for (var j = 0; j < chunks.length; j++) {
      if (!chunks[j] || typeof chunks[j].url !== 'string' || !BLOB.test(chunks[j].url)) return null;
      paths[chunks[j].url] = true;
    }
  }
  return paths;
}

// Cached blob URLs that the manifest no longer references.
function unreferenced(cachedUrls, referenced) {
  return cachedUrls.filter(function (href) { return !referenced[new URL(href).pathname]; });
}

// Network-first: the network answer unless it fails, is a server error or takes
// longer than `timeout` while a cached copy exists. Without a cached copy it
// waits for the network.
async function networkFirst(network, find, timeout) {
  var timer;
  var lookup = function () { return lookupSafely(find); };
  var expired = new Promise(function (resolve) { timer = setTimeout(resolve, timeout, null); });
  try {
    var first = await Promise.race([network, expired]);
    if (first && first.status < 500) return first;
    var cached = await lookup();
    return cached || first || await network;
  } catch (error) {
    return (await lookup()) || Response.error();
  } finally {
    clearTimeout(timer);
  }
}

// The network answer for a cache-first miss, read whole before the page sees it.
// A body passed through while still downloading fails the page's request when
// the transfer drops (WebKit: "FetchEvent.respondWith received an error: Load
// failed"), and WebKit counts a worker with no pending event as idle mid-transfer.
// Reading it here keeps the event pending and lets one dropped transfer be
// retried. Returns { response, bytes } (bytes null for an answer that is not a
// cacheable 200, passed on as the network sent it); rejects when every attempt failed.
async function fetchWhole(request) {
  for (var attempt = 1; ; attempt++) {
    try {
      var response = await fetch(request);
      if (!cacheable(response)) return { response: response, bytes: null };
      var bytes = await response.arrayBuffer();
      // The bytes are already decoded: drop the transfer encoding and length.
      var headers = new Headers(response.headers);
      headers.delete('content-encoding');
      headers.delete('content-length');
      return { response: new Response(bytes, { status: response.status, statusText: response.statusText, headers: headers }), bytes: bytes };
    } catch (error) {
      if (attempt >= FETCH_ATTEMPTS) throw error;
    }
  }
}

// A cache lookup; a storage failure counts as a miss rather than rejecting respondWith.
async function lookupSafely(lookup) {
  try { return (await lookup()) || null; } catch (error) { return null; }
}

async function sha256(bytes) {
  var digest = new Uint8Array(await crypto.subtle.digest('SHA-256', bytes));
  return Array.from(digest, function (value) { return value.toString(16).padStart(2, '0'); }).join('');
}

function shellName(version) { return SHELL_PREFIX + version; }

async function shellComplete(version) {
  if (!version || !(await caches.has(shellName(version)))) return false;
  return Boolean(await (await caches.open(shellName(version))).match(COMPLETE));
}

async function quietly(work) {
  try { await work; } catch (error) { /* Quota or storage failures never affect play. */ }
}

// Content blobs are content-addressed: keep only bytes that hash to their name.
async function storeBlob(href, response) {
  await storeBlobBytes(href, await response.arrayBuffer(), response.headers);
}

async function storeBlobBytes(href, bytes, headers) {
  if ((await sha256(bytes)) !== BLOB.exec(new URL(href).pathname)[1]) return;
  await (await caches.open(CONTENT)).put(href, new Response(bytes, { status: 200, headers: headers }));
}

async function storePage(key, response) {
  if (!cacheable(response)) return;
  // Only keep a page whose whole shell is cached, so an offline launch never
  // opens HTML whose modules are missing.
  if (!(await shellComplete(pageVersion(await response.clone().text())))) return;
  await (await caches.open(PAGES)).put(key, response);
}

async function storeManifest(href, response) {
  if (!cacheable(response)) return;
  var copy = response.clone();
  await (await caches.open(MANIFESTS)).put(href, response);
  return copy;
}

async function prune(response) {
  await new Promise(function (resolve) { setTimeout(resolve, PRUNE_DELAY_MS); });
  var referenced = manifestBlobs(await response.json());
  if (!referenced) return;
  var cache = await caches.open(CONTENT);
  var keys = await cache.keys();
  var stale = unreferenced(keys.map(function (request) { return request.url; }), referenced);
  for (var i = 0; i < stale.length; i++) await cache.delete(stale[i]);
}

function respond(event, kind, request) {
  if (kind === 'page') {
    var key = pageKey(new URL(request.url).pathname), stored = null;
    var page = fetch(request).then(function (response) { stored = quietly(storePage(key, response.clone())); return response; });
    event.waitUntil(page.then(function () { return stored; }, function () {}));
    return networkFirst(page, function () { return caches.open(PAGES).then(function (cache) { return cache.match(key); }); }, PAGE_TIMEOUT_MS);
  }
  if (kind === 'manifest') {
    var done = null;
    var manifest = fetch(request).then(function (response) {
      if (cacheable(response)) {
        // Prune later, off the startup path, from the manifest the network just confirmed.
        done = quietly(storeManifest(request.url, response.clone()).then(function (copy) { return copy && prune(copy); }));
      }
      return response;
    });
    event.waitUntil(manifest.then(function () { return done; }, function () {}));
    return networkFirst(manifest, function () { return caches.open(MANIFESTS).then(function (cache) { return cache.match(request.url); }); }, PAGE_TIMEOUT_MS);
  }
  if (kind === 'static') {
    var store = null;
    var file = fetch(request).then(function (response) {
      if (cacheable(response) && VERSION) store = quietly(caches.open(shellName(VERSION)).then(function (cache) { return cache.put(request.url, response.clone()); }));
      return response;
    });
    event.waitUntil(file.then(function () { return store; }, function () {}));
    return networkFirst(file, function () { return caches.match(request.url, { ignoreVary: true }); }, PAGE_TIMEOUT_MS);
  }
  return (async function () {
    var hit = await lookupSafely(function () {
      return kind === 'blob'
        ? caches.open(CONTENT).then(function (cache) { return cache.match(request.url, { ignoreVary: true }); })
        : caches.match(request.url, { ignoreVary: true });
    });
    if (hit) return hit;
    var fetched;
    // A plain network error, not a rejected respondWith (which WebKit logs as a page error).
    try { fetched = await fetchWhole(request); } catch (error) { return Response.error(); }
    var response = fetched.response;
    if (fetched.bytes) {
      // The Worker serves any ?v= with the current bytes, so only this
      // deploy's version is written; other versions are read-only here.
      if (kind === 'blob') event.waitUntil(quietly(storeBlobBytes(request.url, fetched.bytes, response.headers)));
      else if (VERSIONED.exec(new URL(request.url).search)[1] === VERSION) {
        var copy = response.clone();
        event.waitUntil(quietly(caches.open(shellName(VERSION)).then(function (shell) { return shell.put(request.url, copy); })));
      }
    }
    return response;
  })();
}

async function fetchPage(path) {
  var response = await fetch(path, { cache: 'reload' });
  if (!cacheable(response)) throw new Error('Offline cache: ' + path + ' returned ' + response.status);
  var text = await response.clone().text();
  // A newer deploy than this worker: fail so the browser installs that one instead.
  if (pageVersion(text) !== VERSION) throw new Error('Offline cache: ' + path + ' is not version ' + VERSION);
  return { path: path, response: response, text: text };
}

async function precache() {
  var origin = self.location.origin;
  var pages = await Promise.all(PRECACHE_PAGES.map(fetchPage));
  var shell = await caches.open(shellName(VERSION));
  var seen = {}, count = 0, manifests = {};
  var queue = [];
  pages.forEach(function (page) { queue = queue.concat(references(page.text, new URL(page.path, origin))); });
  while (queue.length) {
    var wave = queue.filter(function (href) {
      if (seen[href]) return false;
      seen[href] = true;
      var kind = route('GET', href, 'cors', false, origin);
      return kind === 'static' || (kind === 'shell' && VERSIONED.exec(new URL(href).search)[1] === VERSION);
    });
    count += wave.length;
    if (count > MAX_SHELL_FILES) throw new Error('Offline cache: shell is unexpectedly large');
    var found = await Promise.all(wave.map(async function (href) {
      var response = await fetch(href);
      // A quoted name that is not a deployed file; real files never 404.
      if (response.status === 404) return [];
      if (!cacheable(response)) throw new Error('Offline cache: ' + href + ' returned ' + response.status);
      var text = /\.(?:js|css|webmanifest)$/.test(new URL(href).pathname) ? await response.clone().text() : '';
      manifestReferences(text, href).forEach(function (manifest) { manifests[manifest] = true; });
      var more = references(text, href);
      await shell.put(href, response);
      return more;
    }));
    queue = [].concat.apply([], found);
  }
  // Best effort: a release's content manifest may be published after its code.
  for (var manifest in manifests) {
    await quietly(fetch(manifest).then(function (response) { return storeManifest(response.url || manifest, response); }));
  }
  await shell.put(COMPLETE, new Response('complete'));
  var cache = await caches.open(PAGES);
  for (var i = 0; i < pages.length; i++) await cache.put(pages[i].path, pages[i].response);
}

async function activate() {
  if (VERSION) {
    // Keep this deploy's shell and any shell a cached entry page still names.
    var keep = {};
    keep[VERSION] = true;
    var pages = await caches.open(PAGES);
    var entries = await pages.keys();
    var versions = {};
    for (var i = 0; i < entries.length; i++) {
      var response = await pages.match(entries[i]);
      versions[entries[i].url] = response ? pageVersion(await response.text()) : null;
    }
    PRECACHE_PAGES.forEach(function (path) {
      var version = versions[new URL(path, self.location.origin).href];
      if (version) keep[version] = true;
    });
    for (var href in versions) if (!keep[versions[href]]) await pages.delete(href);
    var names = await caches.keys();
    for (var j = 0; j < names.length; j++) {
      var name = names[j];
      var stale = name.indexOf(SHELL_PREFIX) === 0 ? !keep[name.slice(SHELL_PREFIX.length)] : name.indexOf('fury-') === 0 && CACHES.indexOf(name) < 0;
      if (stale) await caches.delete(name);
    }
    var manifests = await caches.open(MANIFESTS);
    var stored = await manifests.keys();
    for (var k = 0; k < stored.length; k++) {
      var pinned = MANIFEST.exec(new URL(stored[k].url).pathname);
      if (pinned && pinned[1] && !keep[pinned[1]]) await manifests.delete(stored[k]);
    }
  }
  // Take control of an uncontrolled first-visit page. No other version's page
  // can be open here: without skipWaiting this worker only activates once every
  // page of the previous worker has closed.
  await self.clients.claim();
}

// The page reports content it fetched before this worker controlled it (first
// visit, or the first launch of a Home Screen app with its own storage). The
// browser's HTTP cache normally answers these re-fetches.
async function warm(urls) {
  var origin = self.location.origin;
  for (var i = 0; i < urls.length && i < 1024; i++) {
    var url;
    try { url = new URL(String(urls[i]), origin); } catch (error) { continue; }
    var kind = route('GET', url.href, 'cors', false, origin);
    if (kind !== 'blob' && kind !== 'manifest') continue;
    var cache = await caches.open(kind === 'blob' ? CONTENT : MANIFESTS);
    if (await cache.match(url.href)) continue;
    try {
      var response = await fetch(url.href);
      if (!cacheable(response)) continue;
      await (kind === 'blob' ? storeBlob(url.href, response) : storeManifest(url.href, response));
    } catch (error) { /* Best effort; the next controlled load caches it. */ }
  }
}

if (typeof self !== 'undefined' && typeof self.addEventListener === 'function') {
  self.addEventListener('install', function (event) {
    if (VERSION) event.waitUntil(precache());
  });
  self.addEventListener('activate', function (event) {
    event.waitUntil(activate());
  });
  self.addEventListener('fetch', function (event) {
    if (!VERSION) return;
    var request = event.request;
    var kind = route(request.method, request.url, request.mode, request.headers.has('range'), self.location.origin);
    // Never reject respondWith: anything unexpected falls back to the network.
    if (kind) event.respondWith(respond(event, kind, request).catch(function () { return fetch(request).catch(function () { return Response.error(); }); }));
  });
  self.addEventListener('message', function (event) {
    var data = event.data;
    if (VERSION && data && data.type === 'fury-warm' && Array.isArray(data.urls)) event.waitUntil(warm(data.urls));
  });
}
