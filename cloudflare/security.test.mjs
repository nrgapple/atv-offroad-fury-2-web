import assert from 'node:assert/strict';
import test from 'node:test';
import worker from './worker.mjs';
import { LobbyAdmission, DEFAULT_BUDGET, reserveBudget } from './security.mjs';

function admission() {
  const values = new Map(); let queue = Promise.resolve();
  const state = { storage: { get: async key => structuredClone(values.get(key)), put: async (key, value) => values.set(key, structuredClone(value)) },
    blockConcurrencyWhile: fn => { const result = queue.then(fn); queue = result.catch(() => {}); return result; } };
  const room = new LobbyAdmission(state);
  return { values, room, stub: { fetch: (url, options) => room.fetch(new Request(url, options)) } };
}
const req = (body = '{}', extra = {}) => new Request('https://furyremastered.com/multiplayer/rooms', { method: 'POST', headers: { Origin: 'https://furyremastered.com', 'CF-Connecting-IP': '203.0.113.1', ...extra }, body });
function guarded({ backend = async () => Response.json({ code: 'ABCD2345', hostToken: 'private' }, { status: 201 }), limit = true } = {}) {
  const budget = admission(); let calls = 0;
  return { budget, env: { MULTIPLAYER_GUARDS: 'required', ROOM_CREATE_LIMIT: { limit: async () => ({ success: limit }) }, ROOM_CONNECT_LIMIT: { limit: async () => ({ success: limit }) }, ADMISSION: { idFromName: name => name, get: () => budget.stub }, MULTIPLAYER: { fetch: async request => { calls++; return backend(request); } } }, calls: () => calls };
}

test('reject oversized chunked and declared bodies before any room allocation', async () => {
  const g = guarded();
  for (const r of [req('x'.repeat(513)), req('{}', { 'Content-Length': '100000' }), req('[]'), req('{"code":"../secret"}')]) {
    assert.ok([400, 413].includes((await worker.fetch(r, g.env)).status));
  }
  assert.equal(g.calls(), 0); assert.equal(g.budget.values.size, 0);
});

test('missing protection, missing IP, and exhausted edge budget never reach the relay', async () => {
  const g = guarded({ limit: false });
  assert.equal((await worker.fetch(req(), g.env)).status, 429);
  assert.equal(g.calls(), 0);
  delete g.env.ROOM_CREATE_LIMIT;
  assert.equal((await worker.fetch(req(), g.env)).status, 503);
  const fresh = guarded(); const request = req(); request.headers.delete('CF-Connecting-IP');
  assert.equal((await worker.fetch(request, fresh.env)).status, 503);
  assert.equal(fresh.calls(), 0);
});

test('global room capacity persists and concurrent reservations cannot exceed it', async () => {
  const a = admission();
  const { createsPerMinute, rooms } = DEFAULT_BUDGET;
  a.values.set('budget', { minute: Math.floor(Date.now() / 60000), day: Math.floor(Date.now() / 86400000), creates: 0, connects: 0, dailyCreates: 0, dailyConnects: 0,
    leases: Array.from({ length: rooms - 1 }, (_, id) => ({ id: String(id), expires: Date.now() + 60000 })) });
  const responses = await Promise.all(Array.from({ length: 5 }, () => a.stub.fetch('https://admission/create', { method: 'POST' })));
  assert.equal(responses.filter(r => r.status === 200).length, 1);
  assert.equal(a.values.get('budget').leases.length, rooms);
  assert.ok(createsPerMinute < rooms);
});

test('global daily budget survives minute changes and expired reservations release capacity', () => {
  const now = 5 * 86400000 + 60000, s = {};
  assert.equal(reserveBudget(s, 'create', now, { ...DEFAULT_BUDGET, createsPerDay: 1 }), true);
  assert.equal(reserveBudget(s, 'create', now + 60000, { ...DEFAULT_BUDGET, createsPerDay: 1 }), false);
  assert.equal(reserveBudget(s, 'create', now + 86400000, { ...DEFAULT_BUDGET, createsPerDay: 1 }), true);
  s.leases = Array.from({ length: DEFAULT_BUDGET.rooms }, () => ({ expires: now }));
  assert.equal(reserveBudget(s, 'connect', now), true); assert.equal(s.leases.length, 0);
});

test('failed creation releases its reservation; caller cannot inject an admission capability', async () => {
  let forwarded;
  const g = guarded({ backend: async r => { forwarded = r; return new Response('Taken', { status: 409 }); } });
  assert.equal((await worker.fetch(req('{}', { 'X-ATV-Admission-Lease': 'caller-chosen' }), g.env)).status, 409);
  assert.notEqual(forwarded.headers.get('X-ATV-Admission-Lease'), 'caller-chosen');
  assert.equal(g.budget.values.get('budget').leases.length, 0);
});

test('success commits a bounded lease; admission outages fail closed without leaking credentials', async () => {
  const g = guarded(); assert.equal((await worker.fetch(req(), g.env)).status, 201);
  const lease = g.budget.values.get('budget').leases[0];
  assert.ok(lease.expires - Date.now() > 11 * 3600000);
  assert.equal(JSON.stringify(g.budget.values.get('budget')).includes('hostToken'), false);
  g.env.ADMISSION.get = () => ({ fetch: async () => { throw Error('internal private data'); } });
  const failed = await worker.fetch(req(), g.env);
  assert.equal(failed.status, 503); assert.equal((await failed.text()).includes('private data'), false);
});

test('security headers protect assets/errors/redirects and permit browser WASM and WebSockets', async () => {
  const env = { CANONICAL_ORIGIN: 'https://furyremastered.com', ASSETS: { fetch: async () => new Response('html', { headers: { 'Content-Type': 'text/html' } }) } };
  for (const path of ['https://furyremastered.com/play.html', 'https://www.furyremastered.com/play.html', 'https://furyremastered.com/content/source.iso']) {
    const r = await worker.fetch(new Request(path), env);
    assert.equal(r.headers.get('X-Frame-Options'), 'DENY');
    assert.equal(r.headers.get('Referrer-Policy'), 'no-referrer');
    assert.equal(r.headers.get('X-Content-Type-Options'), 'nosniff');
    assert.match(r.headers.get('Strict-Transport-Security'), /max-age=31536000/);
    assert.match(r.headers.get('Content-Security-Policy'), /'wasm-unsafe-eval'/);
    assert.match(r.headers.get('Content-Security-Policy'), /wss:\/\//);
    assert.match(r.headers.get('Content-Security-Policy'), /frame-ancestors 'none'/);
  }
});
