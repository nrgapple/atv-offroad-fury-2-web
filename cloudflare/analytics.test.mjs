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
  // Safari beacons under Referrer-Policy: no-referrer carry an opaque origin.
  assert.equal((await worker.fetch(post({ name: 'session_start' }, { Origin: 'null', 'Sec-Fetch-Site': 'same-origin' }), env)).status, 204);
  assert.equal((await worker.fetch(post({ name: 'session_start' }, { Origin: 'null' }), env)).status, 403);
  assert.equal((await worker.fetch(post({ name: 'session_start' }, { Origin: 'null', 'Sec-Fetch-Site': 'cross-site' }), env)).status, 403);
  assert.equal((await worker.fetch(post('not json'), env)).status, 400);
  assert.equal((await worker.fetch(post({ name: 'drop_table' }), env)).status, 400);
  assert.equal((await worker.fetch(post('x'.repeat(2048)), env)).status, 413);
  assert.equal((await worker.fetch(new Request(`${ORIGIN}/api/events`), env)).status, 405);
  assert.equal(USAGE.points.length, 2);
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
  assert.match(on.headers.get('Content-Security-Policy'), /connect-src 'self' https:\/\/mp\.furyremastered\.com wss:\/\/mp\.furyremastered\.com https:\/\/cloudflareinsights\.com;/);
  const head = await worker.fetch(new Request(`${ORIGIN}/play.html`, { method: 'HEAD' }), { ASSETS: { fetch: async () => new Response(null, { headers: { 'Content-Type': 'text/html' } }) }, WEB_ANALYTICS_TOKEN: token });
  assert.equal(await head.text(), '');
  const script = await worker.fetch(new Request(`${ORIGIN}/game.js`), { ASSETS: { fetch: async () => new Response('code', { headers: { 'Content-Type': 'text/javascript' } }) }, WEB_ANALYTICS_TOKEN: token });
  assert.equal(await script.text(), 'code');
});

test('excluded addresses drop events silently; others are written', async () => {
  const { excludedAddress } = await import('./analytics.mjs');
  const list = '192.0.2.44, 198.51.100.0/24, 2001:db8:aa:bb::/64, ::ffff:203.0.113.0/120, bogus, 10.0.0.0/33';
  for (const ip of ['192.0.2.44', '198.51.100.7', '2001:db8:aa:bb:1:2:3:4', '2001:DB8:AA:BB::9', '203.0.113.200', '::ffff:192.0.2.44']) assert.ok(excludedAddress(ip, list), ip);
  for (const ip of ['192.0.2.45', '198.51.101.7', '2001:db8:aa:bc::1', '10.0.0.1', '::1', 'not an ip', '', null]) assert.ok(!excludedAddress(ip, list), String(ip));
  assert.ok(!excludedAddress('192.0.2.44', undefined) && !excludedAddress('192.0.2.44', ''));
  assert.ok(excludedAddress('::', '::/0') && excludedAddress('0.0.0.0', '0.0.0.0/0') && !excludedAddress('0.0.0.0', '::/0'));

  const USAGE = dataset(), limited = [];
  const env = { GAME_ORIGINS: ORIGIN, USAGE, ANALYTICS_EXCLUDED_IPS: list, EVENT_LIMIT: { limit: async ({ key }) => { limited.push(key); return { success: true }; } } };
  const send = ip => worker.fetch(post({ name: 'session_start' }, { Origin: ORIGIN, 'CF-Connecting-IP': ip }), env);
  assert.equal((await send('198.51.100.9')).status, 204);
  assert.equal((await send('2001:db8:aa:bb::1')).status, 204);
  assert.equal(USAGE.points.length, 0, 'excluded events are not written');
  assert.deepEqual(limited, [], 'excluded events never reach the limiter');
  assert.equal((await send('198.51.99.9')).status, 204);
  assert.equal(USAGE.points.length, 1);
  assert.ok(!JSON.stringify(USAGE.points).includes('198.51'));
  // Without the secret nothing is excluded.
  assert.equal((await worker.fetch(post({ name: 'session_start' }, { Origin: ORIGIN, 'CF-Connecting-IP': '198.51.100.9' }), { ...env, ANALYTICS_EXCLUDED_IPS: undefined })).status, 204);
  assert.equal(USAGE.points.length, 2);
});
