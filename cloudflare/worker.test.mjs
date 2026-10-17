import assert from 'node:assert/strict';
import test from 'node:test';
import worker from './worker.mjs';

const request = (path, options) => new Request(`https://game.example${path}`, options);
const object = (body) => ({
  body, size: body.length, httpEtag: '"test-etag"',
  writeHttpMetadata: () => {}, json: async () => JSON.parse(body),
});

test('only approved object path shapes are accessible; writes are rejected', async () => {
  let reads = 0;
  const env = { CONTENT: { get: async () => { reads++; return object('safe'); } }, ASSETS: { fetch: async () => new Response('asset') } };
  for (const path of ['/content/source.iso', '/content/SCUS_972.11', '/content/manifest.json/other', '/devlog-images/unapproved.jpg', '/app-icons/private.png', '/app-icons/source.iso']) {
    assert.equal((await worker.fetch(request(path), env)).status, 404);
  }
  assert.equal(reads, 0);
  assert.equal((await worker.fetch(request('/content/manifest.json', { method: 'PUT', body: 'bad' }), env)).status, 405);
  assert.equal(reads, 0);
});

test('original-logo icons use R2 PNG responses and a conventional Safari root fallback', async () => {
  const key = `app-icons/${'b'.repeat(64)}.png`, reads = [];
  const env = { APPLE_TOUCH_ICON_KEY: key, APP_ICON_512_KEY: key, CONTENT: { get: async name => { reads.push(name); return object('png'); }, head: async () => object('png') } };
  const hashed = await worker.fetch(request('/' + key), env);
  assert.equal(hashed.headers.get('Content-Type'), 'image/png');
  assert.match(hashed.headers.get('Cache-Control'), /immutable/);
  for (const name of ['apple-touch-icon.png', 'apple-touch-icon-precomposed.png', 'app-icon-512.png']) {
    const response = await worker.fetch(request('/' + name), env);
    assert.equal(response.status, 200); assert.equal(response.headers.get('Content-Type'), 'image/png');
    assert.equal(response.headers.get('Cache-Control'), 'no-cache');
  }
  assert.deepEqual(reads, [key, key, key, key]);
  assert.equal(await (await worker.fetch(request('/' + key, { method: 'HEAD' }), env)).text(), '');
  assert.equal((await worker.fetch(request('/' + key, { method: 'PUT', body: 'bad' }), env)).status, 405);
});

test('hashed blobs preserve cache, MIME, length, conditional and HEAD semantics', async () => {
  const env = { CONTENT: { get: async () => object('safe'), head: async () => object('safe') } };
  const path = `/content/${'a'.repeat(64)}.bin`;
  const response = await worker.fetch(request(path), env);
  assert.equal(await response.text(), 'safe');
  assert.equal(response.headers.get('Content-Type'), 'application/octet-stream');
  assert.equal(response.headers.get('Content-Length'), '4');
  assert.match(response.headers.get('Cache-Control'), /immutable/);
  assert.equal((await worker.fetch(request(path, { headers: { 'If-None-Match': '"test-etag"' } }), env)).status, 304);
  assert.equal(await (await worker.fetch(request(path, { method: 'HEAD' }), env)).text(), '');
});

test('gallery overlays only screenshots whose authored checkpoint matches CI', async () => {
  const log = { entries: [{ id: 'match', title: 'Same' }, { id: 'changed', title: 'New' }] };
  const overlay = { entries: [
    { title: 'Same', id: 'match', image: '/devlog-images/match.png', caption: 'Approved' },
    { id: 'changed', title: 'Old', image: '/devlog-images/old.png' },
  ] };
  const env = { ASSETS: { fetch: async () => Response.json(log) }, CONTENT: { get: async () => object(JSON.stringify(overlay)) } };
  const result = await (await worker.fetch(request('/devlog.json'), env)).json();
  assert.equal(result.entries[0].image, '/devlog-images/match.png');
  assert.equal(result.entries[0].caption, 'Approved');
  assert.equal(result.entries[1].image, undefined);
  assert.equal(await (await worker.fetch(request('/devlog.json', { method: 'HEAD' }), env)).text(), '');
});

test('root serves the explicit landing asset while preserving request method', async () => {
  let fetched;
  const env = { ASSETS: { fetch: async request => { fetched = request; return new Response(null); } } };
  await worker.fetch(request('/', { method: 'HEAD' }), env);
  assert.equal(new URL(fetched.url).pathname, '/index.html');
  assert.equal(fetched.method, 'HEAD');
});

test('release pins select matching manifest and gallery; internal storage keys stay private', async () => {
  const commit = 'c'.repeat(40);
  const reads = [];
  const env = {
    RELEASE_COMMIT: commit,
    CONTENT: {
      get: async key => { reads.push(key); return object(JSON.stringify(key.endsWith('gallery.json') ? { entries: [] } : { version: 1 })); },
      head: async key => { reads.push(key); return object('head'); },
    },
    ASSETS: { fetch: async () => Response.json({ entries: [] }) },
  };
  await worker.fetch(request('/content/manifest.json'), env);
  await worker.fetch(request('/content/manifest.json', { method: 'HEAD' }), env);
  await worker.fetch(request('/devlog.json'), env);
  assert.deepEqual(reads, [`releases/${commit}/manifest.json`, `releases/${commit}/manifest.json`, `releases/${commit}/gallery.json`]);
  assert.equal(reads.includes('content/manifest.json'), false);
});

test('open tabs can request their immutable historical manifest after another release', async () => {
  const old = 'd'.repeat(40), current = 'e'.repeat(40), reads = [];
  const env = { RELEASE_COMMIT: current, CONTENT: { get: async key => { reads.push(key); return object('{}'); } } };
  assert.equal((await worker.fetch(request(`/content/manifests/${old}.json`), env)).status, 200);
  assert.deepEqual(reads, [`releases/${old}/manifest.json`]);
  assert.equal((await worker.fetch(request('/content/manifests/source.iso'), env)).status, 404);
});

test('multiplayer requests require same-origin and forward only to Rust service', async () => {
  let seen;
  const env = {MULTIPLAYER_GUARDS:"host-only",MULTIPLAYER:{fetch:async req=>{seen=req;return new Response('rust relay');}}};
  assert.equal((await worker.fetch(request('/multiplayer/rooms',{method:'POST'}),env)).status,403);
  assert.equal((await worker.fetch(request('/multiplayer/rooms',{method:'POST',headers:{Origin:'https://foreign.example'}}),env)).status,403);
  const req=request('/multiplayer/rooms',{method:'POST',headers:{Origin:'https://game.example'}});
  assert.equal(await (await worker.fetch(req,env)).text(),'rust relay');
  assert.equal(seen.url, req.url); assert.equal(seen.method, req.method);
  assert.equal((await worker.fetch(request('/multiplayer/rooms',{method:'DELETE',headers:{Origin:'https://game.example'}}),env)).status,405);
});


test('configured game origin remains strict through a local verification proxy', async () => {
  const env={GAME_ORIGIN:'https://approved.example',MULTIPLAYER:{fetch:async()=>new Response('rust')}};
  assert.equal((await worker.fetch(request('/multiplayer/version',{headers:{Origin:'https://game.example'}}),env)).status,403);
  assert.equal(await (await worker.fetch(request('/multiplayer/version',{headers:{Origin:'https://approved.example'}}),env)).text(),'rust');
});

test('custom and legacy multiplayer origins are exact and missing/foreign origins stay denied', async () => {
  const env = {
    GAME_ORIGIN: 'https://furyremastered.com',
    GAME_ORIGINS: 'https://furyremastered.com, https://atv-offroad-fury-2.champtoday.workers.dev,',
    MULTIPLAYER: { fetch: async () => new Response('rust') },
  };
  for (const origin of ['https://furyremastered.com', 'https://atv-offroad-fury-2.champtoday.workers.dev']) {
    assert.equal((await worker.fetch(request('/multiplayer/version', { headers: { Origin: origin } }), env)).status, 200);
  }
  for (const origin of ['', 'null', 'http://furyremastered.com', 'https://www.furyremastered.com', 'https://furyremastered.com.evil.example', 'https://foreign.example']) {
    assert.equal((await worker.fetch(request('/multiplayer/rooms', { method: 'POST', headers: { Origin: origin } }), env)).status, 403);
  }
  assert.equal((await worker.fetch(request('/multiplayer/rooms', { method: 'POST' }), env)).status, 403);
});

test('www canonical redirect preserves game path and invite query without redirecting legacy hosts', async () => {
  const env = { CANONICAL_ORIGIN: 'https://furyremastered.com', ASSETS: { fetch: async () => new Response('game') } };
  const response = await worker.fetch(new Request('https://www.furyremastered.com/play.html?room=ABCD2345'), env);
  assert.equal(response.status, 308);
  assert.equal(response.headers.get('Location'), 'https://furyremastered.com/play.html?room=ABCD2345');
  const tricky = await worker.fetch(new Request('https://www.furyremastered.com//foreign.example/play.html?room=ABCD2345'), env);
  assert.equal(tricky.headers.get('Location'), 'https://furyremastered.com//foreign.example/play.html?room=ABCD2345');
  for (const origin of ['https://furyremastered.com', 'https://atv-offroad-fury-2.champtoday.workers.dev', 'https://www.furyremastered.com.evil.example']) {
    assert.equal((await worker.fetch(new Request(origin + '/play.html'), env)).status, 200);
  }
});


test('authored Home Screen icons use checked static assets including Safari aliases', async () => {
  const reads = [];
  const env = { APPLE_TOUCH_ICON_KEY: 'fury-remastered-home-180-v1.png', APP_ICON_512_KEY: 'fury-remastered-home-512-v1.png',
    ASSETS: { fetch: async request => { reads.push([new URL(request.url).pathname, request.method]); return new Response(request.method === 'HEAD' ? null : 'png', {headers:{'Content-Type':'image/png'}}); } },
    CONTENT: { get: async () => { throw Error('authored icon must not depend on R2'); } } };
  for (const [path, expected] of [['/apple-touch-icon.png',180],['/apple-touch-icon-precomposed.png',180],['/app-icon-512.png',512],['/fury-remastered-home-192-v1.png',192]]) {
    for (const method of ['GET','HEAD']) {
      const response = await worker.fetch(new Request('https://game.example'+path,{method}),env);
      assert.equal(response.status,200);assert.equal(response.headers.get('Content-Type'),'image/png');
      assert.deepEqual(reads.at(-1),[`/fury-remastered-home-${expected}-v1.png`,method]);
    }
  }
});

test('app files stamped with the deployed commit are immutable; other versions revalidate', async () => {
  const commit = 'c'.repeat(40);
  const env = { BUILD_COMMIT: commit, ASSETS: { fetch: async request => new URL(request.url).pathname === '/missing.js' ? new Response('no', { status: 404 }) : new Response('asset', { headers: { 'Cache-Control': 'public, max-age=0, must-revalidate' } }) } };
  const current = await worker.fetch(request(`/game.js?v=${commit}`), env);
  assert.equal(await current.text(), 'asset');
  assert.equal(current.headers.get('Cache-Control'), 'public, max-age=31536000, immutable');
  for (const path of [`/game.js?v=${'d'.repeat(40)}`, '/game.js', '/play.html', `/game.js?v=${commit}&x=1`, `/missing.js?v=${commit}`]) {
    assert.doesNotMatch((await worker.fetch(request(path), env)).headers.get('Cache-Control') ?? '', /immutable/, path);
  }
  assert.doesNotMatch((await worker.fetch(request(`/game.js?v=${commit}`), { ...env, BUILD_COMMIT: undefined })).headers.get('Cache-Control') ?? '', /immutable/);
});
