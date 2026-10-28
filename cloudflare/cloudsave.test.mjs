import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { DatabaseSync } from 'node:sqlite';
import test from 'node:test';
import worker from './worker.mjs';
import { MAX_SAVE_BYTES, normalizeCode, randomSymbols } from './cloudsave.mjs';

// A D1 stand-in over real SQLite with the production migration applied.
// Batches run in one transaction, as D1 does.
function d1() {
  const db = new DatabaseSync(':memory:');
  db.exec(readFileSync(new URL('./migrations/0001_cloud_saves.sql', import.meta.url), 'utf8'));
  const statement = (sql, params = []) => ({
    bind: (...values) => statement(sql, values),
    run: async () => statement(sql, params).exec(),
    all: async () => statement(sql, params).exec(),
    first: async () => statement(sql, params).exec().results[0] ?? null,
    exec() {
      const prepared = db.prepare(sql);
      if (/^\s*SELECT/i.test(sql)) return { results: prepared.all(...params).map(row => ({ ...row })), meta: { changes: 0 } };
      return { results: [], meta: { changes: Number(prepared.run(...params).changes) } };
    },
  });
  return {
    db,
    prepare: sql => statement(sql),
    batch: async statements => {
      db.exec('BEGIN');
      try { const results = statements.map(item => item.exec()); db.exec('COMMIT'); return results; } catch (failure) { db.exec('ROLLBACK'); throw failure; }
    },
  };
}

function limiter(limit = Infinity) {
  const counts = new Map();
  return { counts, limit: async ({ key }) => { counts.set(key, (counts.get(key) ?? 0) + 1); return { success: counts.get(key) <= limit }; } };
}

function setup(limits = {}) {
  return {
    SAVES: d1(),
    GAME_ORIGINS: 'https://furyremastered.com,https://atv-offroad-fury-2.champtoday.workers.dev',
    SAVE_READ_LIMIT: limiter(limits.read), SAVE_WRITE_LIMIT: limiter(limits.write),
    SAVE_CREATE_LIMIT: limiter(limits.create), SAVE_SHARE_LIMIT: limiter(limits.share),
  };
}

const KEY = 'FURY-7K3M-9Q2D-XW4H-1BCE';
function call(env, method, path, { key = KEY, share, body, headers = {} } = {}) {
  const extra = { 'CF-Connecting-IP': '203.0.113.9', ...headers };
  if (key !== null) extra['X-Fury-Pass-Key'] = key;
  if (share) extra['X-Fury-Share-Code'] = share;
  const request = new Request(`https://furyremastered.com${path}`, { method, headers: extra, body: body === undefined ? undefined : typeof body === 'string' ? body : JSON.stringify(body) });
  return worker.fetch(request, env);
}
async function result(response) { return { status: response.status, body: await response.json(), headers: response.headers }; }
const put = (env, baseRevision, save, options = {}) => call(env, 'PUT', '/api/save', { body: { baseRevision, save }, ...options }).then(result);

test('create, read and update a save with no-store JSON responses', async () => {
  const env = setup();
  assert.deepEqual((await result(await call(env, 'GET', '/api/save'))).body, { error: 'not_found' });
  const created = await put(env, 0, { v: 1, gold: 3 });
  assert.equal(created.status, 200); assert.equal(created.body.revision, 1);
  assert.match(created.body.updatedAt, /^\d{4}-\d\d-\d\dT/);
  assert.equal(created.headers.get('Cache-Control'), 'no-store');
  const read = await result(await call(env, 'GET', '/api/save'));
  assert.equal(read.status, 200);
  assert.deepEqual(read.body, { revision: 1, updatedAt: created.body.updatedAt, save: { v: 1, gold: 3 } });
  assert.equal(read.headers.get('Cache-Control'), 'no-store');
  assert.match(read.headers.get('Content-Type'), /^application\/json/);
  assert.equal((await put(env, 1, { v: 1, gold: 4 })).body.revision, 2);
  assert.deepEqual((await result(await call(env, 'GET', '/api/save'))).body.save, { v: 1, gold: 4 });
});

test('only hashes are stored, never the raw key', async () => {
  const env = setup();
  await put(env, 0, { v: 1 });
  const rows = env.SAVES.db.prepare('SELECT id FROM keys').all();
  assert.equal(rows.length, 1);
  assert.match(rows[0].id, /^[0-9a-f]{64}$/);
  const dump = JSON.stringify([rows, env.SAVES.db.prepare('SELECT * FROM revisions').all()]);
  assert.ok(!dump.includes('7K3M') && !dump.includes('7K3M9Q2D'));
});

test('a stale or duplicate write gets 409 with the current save for merging', async () => {
  const env = setup();
  await put(env, 0, { v: 1, a: 1 });
  await put(env, 1, { v: 1, a: 2 });
  const stale = await put(env, 1, { v: 1, a: 'stale' });
  assert.equal(stale.status, 409);
  assert.equal(stale.body.error, 'conflict'); assert.equal(stale.body.revision, 2);
  assert.deepEqual(stale.body.save, { v: 1, a: 2 }); assert.ok(stale.body.updatedAt);
  const recreate = await put(env, 0, { v: 1, a: 'new' });
  assert.equal(recreate.status, 409); assert.equal(recreate.body.revision, 2);
  const ahead = await put(env, 7, { v: 1 });
  assert.equal(ahead.status, 409);
  // An unknown key with a nonzero base has no current save to merge.
  const missing = await put(env, 3, { v: 1 }, { key: 'FURY-0000-0000-0000-0001' });
  assert.deepEqual(missing.body, { error: 'conflict', revision: 0, updatedAt: null, save: null });
  assert.deepEqual((await result(await call(env, 'GET', '/api/save'))).body.save, { v: 1, a: 2 });
});

test('revision history keeps the last 10 and restore makes an old one current', async () => {
  const env = setup();
  for (let n = 0; n < 13; n++) assert.equal((await put(env, n, { v: 1, n })).body.revision, n + 1);
  const list = await result(await call(env, 'GET', '/api/save/revisions'));
  assert.equal(list.status, 200);
  assert.deepEqual(list.body.revisions.map(item => item.revision), [13, 12, 11, 10, 9, 8, 7, 6, 5, 4]);
  assert.equal(list.body.revisions[0].bytes, JSON.stringify({ v: 1, n: 12 }).length);
  assert.ok(list.body.revisions.every(item => typeof item.updatedAt === 'string'));
  assert.equal(env.SAVES.db.prepare('SELECT COUNT(*) AS n FROM revisions').get().n, 10);
  assert.equal((await result(await call(env, 'POST', '/api/save/restore', { body: { revision: 3 } }))).status, 404);
  const restored = await result(await call(env, 'POST', '/api/save/restore', { body: { revision: 5 } }));
  assert.equal(restored.status, 200);
  assert.equal(restored.body.revision, 14); assert.deepEqual(restored.body.save, { v: 1, n: 4 });
  assert.deepEqual((await result(await call(env, 'GET', '/api/save'))).body, { revision: 14, updatedAt: restored.body.updatedAt, save: { v: 1, n: 4 } });
  const after = (await result(await call(env, 'GET', '/api/save/revisions'))).body.revisions.map(item => item.revision);
  assert.deepEqual(after, [14, 13, 12, 11, 10, 9, 8, 7, 6, 5]);
  for (const body of [{}, { revision: 0 }, { revision: '5' }, { revision: 1.5 }, []]) {
    assert.deepEqual((await result(await call(env, 'POST', '/api/save/restore', { body }))).body, { error: 'bad_body' });
  }
  assert.equal((await result(await call(env, 'GET', '/api/save/revisions', { key: 'FURY-0000-0000-0000-0002' }))).status, 404);
  assert.equal((await result(await call(env, 'POST', '/api/save/restore', { key: 'FURY-0000-0000-0000-0002', body: { revision: 1 } }))).status, 404);
});

test('share codes name immutable snapshots that load without a Pass Key', async () => {
  const env = setup();
  assert.equal((await result(await call(env, 'POST', '/api/save/share'))).status, 404);
  await put(env, 0, { v: 1, progress: 100 });
  const shared = await result(await call(env, 'POST', '/api/save/share'));
  assert.equal(shared.status, 201);
  assert.match(shared.body.shareCode, /^SHARE-[0-9A-HJKMNP-TV-Z]{4}(-[0-9A-HJKMNP-TV-Z]{4}){3}$/);
  assert.ok(shared.body.createdAt);
  await put(env, 1, { v: 1, progress: 0 });
  const loaded = await result(await call(env, 'GET', '/api/share', { key: null, share: shared.body.shareCode }));
  assert.equal(loaded.status, 200);
  assert.deepEqual(loaded.body, { createdAt: shared.body.createdAt, save: { v: 1, progress: 100 } });
  const compact = shared.body.shareCode.replace(/^SHARE-/, '').replaceAll('-', '').toLowerCase().replaceAll('0', 'o').replaceAll('1', 'l');
  assert.equal((await result(await call(env, 'GET', '/api/share', { key: null, share: compact }))).body.save.progress, 100);
  const second = await result(await call(env, 'POST', '/api/save/share'));
  assert.notEqual(second.body.shareCode, shared.body.shareCode);
  assert.equal((await result(await call(env, 'GET', '/api/share', { key: null, share: second.body.shareCode }))).body.save.progress, 0);
  assert.equal((await result(await call(env, 'GET', '/api/share', { key: null, share: 'SHARE-0000-0000-0000-0000' }))).status, 404);
  assert.deepEqual((await result(await call(env, 'GET', '/api/share', { key: null, share: 'SHARE-UUUU-0000-0000-0000' }))).body, { error: 'bad_key' });
  assert.ok(!JSON.stringify(env.SAVES.db.prepare('SELECT id FROM shares').all()).includes(shared.body.shareCode.slice(6, 10)));
  // A share code is not a Pass Key: it cannot write anywhere.
  assert.equal((await put(env, 0, { v: 1 }, { key: shared.body.shareCode })).status, 400);
});

test('Pass Key normalization accepts case, spacing, dashes, prefix and O/I/L', async () => {
  const symbols = '7K3M9Q2DXW4H1BCE';
  for (const form of [KEY, symbols, 'fury 7k3m 9q2d xw4h 1bce', 'FURY7K3M9Q2DXW4HIBCE', '7K3M-9Q2D-XW4H-LBCE', ' fury-7k3m9q2d-xw4h-1bce ']) {
    assert.equal(normalizeCode(form, 'FURY'), symbols, form);
  }
  assert.equal(normalizeCode('FURY-0O00-0000-0000-000O', 'FURY'), '0000000000000000');
  for (const bad of ['', 'FURY-7K3M-9Q2D-XW4H-1BC', 'FURY-7K3M-9Q2D-XW4H-1BCEE', 'FURY-7K3M-9Q2D-XW4H-1BCU', 'FURY-7K3M_9Q2D-XW4H-1BCE', 'SHARE-7K3M-9Q2D-XW4H-1BCE', null, 'X'.repeat(80)]) {
    assert.equal(normalizeCode(bad, 'FURY'), null, String(bad));
  }
  // A 16-symbol code that happens to start with S-H-A-R-E is not stripped.
  assert.equal(normalizeCode('SHARE00000000000', 'SHARE'), 'SHARE00000000000');
  assert.equal(normalizeCode('SHARE-SHARE000-0000-0000', 'SHARE'), 'SHARE00000000000');
  for (let i = 0; i < 50; i++) assert.match(randomSymbols(), /^[0-9A-HJKMNP-TV-Z]{16}$/);
  const env = setup();
  await put(env, 0, { v: 1, same: true }, { key: 'fury-7k3m-9q2d-xw4h-lbce' });
  assert.deepEqual((await result(await call(env, 'GET', '/api/save', { key: '7K3M 9Q2D XW4H IBCE' }))).body.save, { v: 1, same: true });
});

test('bad keys, bodies, sizes and methods are rejected before storage writes', async () => {
  const env = setup();
  for (const key of [null, '', 'FURY-1234', 'FURY-UUUU-UUUU-UUUU-UUUU']) {
    assert.deepEqual((await result(await call(env, 'GET', '/api/save', { key }))).body, { error: 'bad_key' });
  }
  for (const body of ['not json', '[]', 'null', { save: { v: 1 } }, { baseRevision: -1, save: { v: 1 } }, { baseRevision: '0', save: { v: 1 } },
    { baseRevision: 0 }, { baseRevision: 0, save: [] }, { baseRevision: 0, save: {} }, { baseRevision: 0, save: { v: 0 } }, { baseRevision: 0, save: { v: '1' } }, { baseRevision: 0, save: { v: 1.5 } }]) {
    const response = await result(await call(env, 'PUT', '/api/save', { body }));
    assert.equal(response.status, 400, JSON.stringify(body)); assert.deepEqual(response.body, { error: 'bad_body' });
  }
  const filler = 'x'.repeat(MAX_SAVE_BYTES - JSON.stringify({ v: 1, f: '' }).length);
  assert.equal((await put(env, 0, { v: 1, f: filler })).status, 200);
  assert.deepEqual((await put(env, 1, { v: 1, f: filler + 'x' })).body, { error: 'too_large' });
  // Multi-byte characters count as their UTF-8 size.
  assert.equal((await put(env, 1, { v: 1, f: 'é'.repeat(MAX_SAVE_BYTES / 2) })).status, 413);
  assert.equal((await result(await call(env, 'PUT', '/api/save', { body: 'x'.repeat(MAX_SAVE_BYTES * 2) }))).status, 413);
  assert.equal((await result(await call(env, 'PUT', '/api/save', { body: '{}', headers: { 'Content-Length': '999999' } }))).status, 413);
  const wrong = await result(await call(env, 'DELETE', '/api/save'));
  assert.equal(wrong.status, 405); assert.equal(wrong.headers.get('Allow'), 'GET, PUT');
  assert.equal((await result(await call(env, 'GET', '/api/save/share'))).status, 405);
  assert.equal((await result(await call(env, 'GET', '/api/save/other'))).status, 404);
  assert.equal(env.SAVES.db.prepare('SELECT revision FROM keys').get().revision, 1);
});

test('rate limits answer 429 with Retry-After per IP, and per key for writes', async () => {
  const env = setup({ read: 2, write: 3, create: 1, share: 1 });
  assert.equal((await call(env, 'GET', '/api/save')).status, 404);
  assert.equal((await call(env, 'GET', '/api/save')).status, 404);
  const limited = await result(await call(env, 'GET', '/api/save'));
  assert.equal(limited.status, 429);
  assert.deepEqual(limited.body, { error: 'rate_limited', retryAfter: 60 });
  assert.equal(limited.headers.get('Retry-After'), '60');
  assert.equal((await put(env, 0, { v: 1 })).status, 200);
  assert.equal((await put(env, 0, { v: 1 }, { key: 'FURY-0000-0000-0000-0003' })).status, 429, 'second create from one IP');
  assert.equal((await put(env, 1, { v: 1 }, { headers: { 'CF-Connecting-IP': '198.51.100.1' } })).status, 200);
  assert.equal((await put(env, 2, { v: 1 }, { headers: { 'CF-Connecting-IP': '198.51.100.2' } })).status, 200);
  assert.equal((await put(env, 3, { v: 1 }, { headers: { 'CF-Connecting-IP': '198.51.100.3' } })).status, 429, 'per-key write budget');
  assert.equal((await call(env, 'POST', '/api/save/share')).status, 201);
  assert.equal((await call(env, 'POST', '/api/save/share')).status, 429);
  // The limiter never sees a raw key.
  assert.ok([...env.SAVE_WRITE_LIMIT.counts.keys()].every(key => !key.includes('7K3M')));
});

test('missing storage, limiter or client IP is unavailable, and storage errors are 503', async () => {
  for (const drop of ['SAVES', 'SAVE_READ_LIMIT']) {
    const env = setup(); delete env[drop];
    assert.deepEqual((await result(await call(env, 'GET', '/api/save'))).body, { error: 'unavailable' });
  }
  assert.equal((await call(setup(), 'GET', '/api/save', { headers: { 'CF-Connecting-IP': '' } })).status, 503);
  const env = setup(); env.SAVES = { prepare: () => { throw new Error('D1 down'); }, batch: async () => { throw new Error('D1 down'); } };
  assert.equal((await call(env, 'GET', '/api/save')).status, 503);
  assert.equal((await put(env, 0, { v: 1 })).status, 503);
});

test('browser origins must be game origins; native clients send none', async () => {
  const env = setup();
  for (const origin of ['https://furyremastered.com', 'https://atv-offroad-fury-2.champtoday.workers.dev']) {
    assert.equal((await put(env, 0, { v: 1 }, { key: `FURY-0000-0000-0000-00${origin.length}`, headers: { Origin: origin } })).status, 200, origin);
  }
  assert.equal((await put(env, 0, { v: 1 })).status, 200, 'no Origin (desktop/Android)');
  for (const headers of [{ Origin: 'https://evil.example' }, { Origin: 'null' }, { Origin: 'http://furyremastered.com' }, { Origin: 'https://furyremastered.com.evil.example' }, { 'Sec-Fetch-Site': 'cross-site' }]) {
    const response = await result(await call(env, 'GET', '/api/save', { headers }));
    assert.equal(response.status, 403, JSON.stringify(headers)); assert.deepEqual(response.body, { error: 'origin_denied' });
  }
  // Security headers still wrap API responses.
  assert.equal((await call(env, 'GET', '/api/save')).headers.get('X-Content-Type-Options'), 'nosniff');
});
