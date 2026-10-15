import assert from 'node:assert/strict';
import test from 'node:test';
import worker from './worker.mjs';

const ORIGIN = 'https://furyremastered.com';
const post = (body, headers = { Origin: ORIGIN }) => new Request(`${ORIGIN}/api/events`, { method: 'POST', body: typeof body === 'string' ? body : JSON.stringify(body), headers });
const dataset = () => { const points = []; return { points, writeDataPoint: point => points.push(point) }; };

test('usage events are validated, enriched with release only and written', async () => {
  const USAGE = dataset(), limited = [];
  const env = { GAME_ORIGINS: ORIGIN, USAGE, BUILD_COMMIT: 'a'.repeat(40), EVENT_LIMIT: { limit: async ({ key }) => { limited.push(key); return { success: true }; } } };
  const request = post({ name: 'ride_start', mode: 'multiplayer', map: 'env01', quad: 4, players: 3, room: 'ABCD1234', player: 'Name' }, { Origin: ORIGIN, 'CF-Connecting-IP': '203.0.113.9' });
  const response = await worker.fetch(request, env);
  assert.equal(response.status, 204);
  assert.deepEqual(limited, ['203.0.113.9']);
  assert.equal(USAGE.points.length, 1);
  const [point] = USAGE.points;
  assert.deepEqual(point.indexes, ['ride_start']);
  assert.equal(point.blobs[2], 'a'.repeat(12));
  assert.ok(!JSON.stringify(point).includes('ABCD1234') && !JSON.stringify(point).includes('Name') && !JSON.stringify(point).includes('203.0.113.9'));
});

test('usage events reject foreign origins, bad bodies and other methods', async () => {
  const USAGE = dataset(), env = { GAME_ORIGINS: ORIGIN, USAGE };
  assert.equal((await worker.fetch(post({ name: 'session_start' }, { Origin: 'https://evil.example' }), env)).status, 403);
  assert.equal((await worker.fetch(post({ name: 'session_start' }, {}), env)).status, 403);
  assert.equal((await worker.fetch(post({ name: 'session_start' }, { 'Sec-Fetch-Site': 'same-origin' }), env)).status, 204);
  assert.equal((await worker.fetch(post('not json'), env)).status, 400);
  assert.equal((await worker.fetch(post({ name: 'drop_table' }), env)).status, 400);
  assert.equal((await worker.fetch(post('x'.repeat(2048)), env)).status, 413);
  assert.equal((await worker.fetch(new Request(`${ORIGIN}/api/events`), env)).status, 405);
  assert.equal(USAGE.points.length, 1);
});

test('rate limits and missing bindings drop events without errors', async () => {
  const USAGE = dataset();
  const limitedEnv = { GAME_ORIGINS: ORIGIN, USAGE, EVENT_LIMIT: { limit: async () => ({ success: false }) } };
  assert.equal((await worker.fetch(post({ name: 'session_start' }, { Origin: ORIGIN, 'CF-Connecting-IP': '198.51.100.1' }), limitedEnv)).status, 204);
  assert.equal(USAGE.points.length, 0);
  assert.equal((await worker.fetch(post({ name: 'session_start' }), { GAME_ORIGINS: ORIGIN })).status, 204);
});

test('web analytics beacon and CSP origins appear only with a valid site token', async () => {
  const html = '<!doctype html><html><body><p>Game</p></body></html>';
  const assets = { fetch: async () => new Response(html, { headers: { 'Content-Type': 'text/html; charset=utf-8', ETag: '"x"', 'Content-Length': String(html.length) } }) };
  const off = await worker.fetch(new Request(`${ORIGIN}/play.html`), { ASSETS: assets });
  assert.equal(await off.text(), html);
  assert.doesNotMatch(off.headers.get('Content-Security-Policy'), /cloudflareinsights/);
  const invalid = await worker.fetch(new Request(`${ORIGIN}/play.html`), { ASSETS: assets, WEB_ANALYTICS_TOKEN: '"><script>' });
  assert.equal(await invalid.text(), html);
  const token = '0123456789abcdef0123456789abcdef';
  const on = await worker.fetch(new Request(`${ORIGIN}/play.html`), { ASSETS: assets, WEB_ANALYTICS_TOKEN: token });
  const body = await on.text();
  assert.match(body, new RegExp(`static\\.cloudflareinsights\\.com/beacon\\.min\\.js" data-cf-beacon='\\{"token":"${token}"\\}'></script>\\n</body>`));
  assert.equal(on.headers.get('ETag'), null);
  assert.match(on.headers.get('Content-Security-Policy'), /script-src 'self' 'wasm-unsafe-eval' https:\/\/static\.cloudflareinsights\.com;/);
  assert.match(on.headers.get('Content-Security-Policy'), /connect-src 'self' wss:\/\/furyremastered\.com https:\/\/cloudflareinsights\.com;/);
  const head = await worker.fetch(new Request(`${ORIGIN}/play.html`, { method: 'HEAD' }), { ASSETS: { fetch: async () => new Response(null, { headers: { 'Content-Type': 'text/html' } }) }, WEB_ANALYTICS_TOKEN: token });
  assert.equal(await head.text(), '');
  const script = await worker.fetch(new Request(`${ORIGIN}/game.js`), { ASSETS: { fetch: async () => new Response('code', { headers: { 'Content-Type': 'text/javascript' } }) }, WEB_ANALYTICS_TOKEN: token });
  assert.equal(await script.text(), 'code');
});
