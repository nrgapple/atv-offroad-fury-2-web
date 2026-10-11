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
  for (const path of ['/content/source.iso', '/content/SCUS_972.11', '/content/manifest.json/other', '/devlog-images/unapproved.jpg']) {
    assert.equal((await worker.fetch(request(path), env)).status, 404);
  }
  assert.equal(reads, 0);
  assert.equal((await worker.fetch(request('/content/manifest.json', { method: 'PUT', body: 'bad' }), env)).status, 405);
  assert.equal(reads, 0);
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
