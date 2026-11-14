import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { DatabaseSync } from 'node:sqlite';
import test from 'node:test';
import worker from './worker.mjs';
import {
  GHOST_HEADER, GHOST_KEY, GHOST_LIMIT, GHOST_MAX_AGE, GHOST_STRIDE, GHOST_TOP, MAX_GHOST_BYTES, MAX_GHOST_KEYS, MAX_GHOST_TEXT,
  checkGhost, decodeGhost, ghostHeader, keptGhosts,
} from './ghosts.mjs';

// A D1 stand-in over real SQLite with the production migrations applied.
function d1() {
  const db = new DatabaseSync(':memory:');
  for (const file of ['0001_cloud_saves.sql', '0002_world_boards.sql', '0003_board_ghosts.sql']) db.exec(readFileSync(new URL(`./migrations/${file}`, import.meta.url), 'utf8'));
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
  return { limit: async ({ key }) => { counts.set(key, (counts.get(key) ?? 0) + 1); return { success: counts.get(key) <= limit }; } };
}
function setup(limits = {}) {
  return {
    SAVES: d1(), GAME_ORIGINS: 'https://furyremastered.com',
    BOARD_READ_LIMIT: limiter(limits.read), BOARD_SUBMIT_LIMIT: limiter(limits.submit),
  };
}
function call(env, method, path, body, headers = {}) {
  const request = new Request(`https://furyremastered.com${path}`, {
    method, headers: { 'CF-Connecting-IP': '203.0.113.9', ...headers }, body: body === undefined ? undefined : typeof body === 'string' ? body : JSON.stringify(body),
  });
  return worker.fetch(request, env).then(async response => ({ status: response.status, body: await response.json(), headers: response.headers }));
}

// A lap on a circular course of `feet`: the boards' trace and the ghost of
// the same ride (atv_core::world_ghost's bytes: a 32-byte header, then a key
// every sixth step of 1/60 s).
function trace(laps, lapSeconds, feet = 4000) {
  const radius = feet / (2 * Math.PI), points = [];
  for (let t = 0; t <= laps * lapSeconds; t++) {
    const angle = (2 * Math.PI * t) / lapSeconds;
    points.push([Math.round(radius * Math.cos(angle)), Math.round(radius * Math.sin(angle))]);
  }
  return { points, marks: Array.from({ length: laps }, (_, i) => (i + 1) * lapSeconds * 1000) };
}
function ghost({ course = 'nat01', vehicle = 3, lapMs = 60_000, steps = Math.round((lapMs * 60) / 1000), feet = 4000, edit } = {}) {
  const keys = Math.floor((steps - 1) / GHOST_STRIDE) + 1;
  const bytes = new Uint8Array(GHOST_HEADER + keys * GHOST_KEY), view = new DataView(bytes.buffer);
  bytes.set([0x46, 0x52, 0x47, 0x48, 1, GHOST_STRIDE]);
  view.setUint16(6, vehicle, true); view.setUint32(8, lapMs, true); view.setUint32(12, steps, true); view.setUint16(16, keys, true);
  bytes[18] = course.length;
  for (let i = 0; i < course.length; i++) bytes[19 + i] = course.charCodeAt(i);
  const radius = feet / (2 * Math.PI);
  for (let key = 0; key < keys; key++) {
    const angle = (2 * Math.PI * key * GHOST_STRIDE) / steps, at = GHOST_HEADER + key * GHOST_KEY;
    view.setInt32(at, Math.round(radius * Math.cos(angle) * 32), true);
    view.setInt32(at + 4, 12 * 32, true);
    view.setInt32(at + 8, Math.round(radius * Math.sin(angle) * 32), true);
    view.setUint32(at + 12, 0xc0080200, true);
  }
  edit?.(bytes, view);
  return bytes;
}
const text = bytes => Buffer.from(bytes).toString('base64');
const lapRun = (name, lapMs, vehicle = 3) => ({ name, vehicle, course: 'nat01', lapMs, trace: trace(1, lapMs / 1000) });
const BOARD = 'lap:nat01:n';
const submit = (env, name, lapMs, vehicle) => call(env, 'POST', '/api/boards/submit', lapRun(name, lapMs, vehicle));
const upload = (env, name, bytes, board = BOARD) => call(env, 'POST', `/api/ghosts/${board}`, { name, ghost: text(bytes) });
const stored = env => env.SAVES.db.prepare('SELECT name, value FROM board_ghosts ORDER BY value, name').all().map(row => `${row.name}:${row.value}`);

test('the format: sizes, the header and what a ghost must match', () => {
  assert.equal(MAX_GHOST_KEYS, 4094);
  assert.equal(MAX_GHOST_TEXT, 87_384);
  // A 60 s lap: 600 keys, 9.6 kB.
  const lap = ghost();
  assert.equal(lap.length, 9632);
  assert.deepEqual(ghostHeader(lap), { stride: 6, vehicle: 3, lapMs: 60_000, steps: 3600, keys: 600, course: 'nat01' });
  const row = { course: 'nat01', vehicle: 3, lapMs: 60_000 };
  assert.equal(checkGhost(lap, row), null);
  assert.deepEqual(decodeGhost(text(lap)), lap);
  // The longest lap that keeps a ghost fills the cap.
  const longest = ghost({ lapMs: 409_400, feet: 40_000 });
  assert.equal(longest.length, MAX_GHOST_BYTES);
  assert.equal(checkGhost(longest, { ...row, lapMs: 409_400 }), null);
  // Another row's ghost.
  assert.equal(checkGhost(lap, { ...row, course: 'nat02' }), 'ghost_row');
  assert.equal(checkGhost(lap, { ...row, vehicle: 4 }), 'ghost_row');
  assert.equal(checkGhost(lap, { ...row, lapMs: 59_999 }), 'ghost_row');
  // Not a ghost.
  assert.equal(checkGhost(new Uint8Array(10), row), 'ghost_format');
  assert.equal(checkGhost(lap.subarray(0, lap.length - 1), row), 'ghost_format');
  assert.equal(checkGhost(ghost({ edit: bytes => { bytes[4] = 2; } }), row), 'ghost_format');
  assert.equal(checkGhost(ghost({ edit: bytes => { bytes[0] = 0x58; } }), row), 'ghost_format');
  assert.equal(checkGhost(ghost({ edit: bytes => { bytes[30] = 1; } }), row), 'ghost_format');
  assert.equal(checkGhost(ghost({ edit: bytes => { bytes[19] = 0x2f; } }), row), 'ghost_format');
  // A length that does not last the lap time, or another key rate.
  assert.equal(checkGhost(ghost({ steps: 1800 }), row), 'ghost_length');
  assert.equal(checkGhost(ghost({ edit: bytes => { bytes[5] = 12; } }), row), 'ghost_length');
  assert.equal(checkGhost(ghost({ edit: (_, view) => view.setUint32(12, 3601, true) }), row), 'ghost_length');
  // Motion no vehicle rides: standing still, far outside any course, or
  // jumping about more than resets allow.
  assert.equal(checkGhost(ghost({ feet: 100 }), row), 'ghost_short');
  assert.equal(checkGhost(ghost({ edit: (_, view) => view.setInt32(GHOST_HEADER + 160, 0x7f000000, true) }), row), 'ghost_bounds');
  assert.equal(checkGhost(ghost({ feet: 40_000 }), row), 'ghost_speed');
  // One reset's jump is ridden over.
  const reset = ghost({ edit: (_, view) => { for (let key = 300; key < 600; key++) view.setInt32(GHOST_HEADER + key * GHOST_KEY + 4, 500 * 32, true); } });
  assert.equal(checkGhost(reset, row), null);
  // Base64 as the game writes it, nothing else.
  assert.equal(decodeGhost('AAA'), null);
  assert.equal(decodeGhost('AA*A'), null);
  assert.equal(decodeGhost(5), null);
  assert.equal(decodeGhost('A'.repeat(MAX_GHOST_TEXT + 4)), null);
});

test('a ghost is stored for its kept row, marked on the board and downloaded from the edge cache', async () => {
  const env = setup();
  const store = new Map();
  globalThis.caches = { default: { match: async request => store.get(request.url)?.clone(), put: async (request, response) => { store.set(request.url, response); }, delete: async request => store.delete(request.url) } };
  try {
    assert.equal((await submit(env, 'ADA', 60_000)).body.boards[BOARD].placed, true);
    assert.equal((await submit(env, 'BOB', 62_000, 28)).status, 200);
    let board = await call(env, 'GET', `/api/boards/${BOARD}`);
    assert.deepEqual(board.body.entries.filter(e => !e.house).map(e => e.ghost), [undefined, undefined]);
    assert.ok(board.body.entries.filter(e => e.house).every(e => !e.ghost), 'house riders have no ghost');
    // The upload names its row by name, lap time and vehicle.
    const lap = ghost();
    assert.deepEqual((await upload(env, 'ada', lap)).body, { board: BOARD, name: 'ADA', kept: true });
    assert.deepEqual(stored(env), ['ADA:60000']);
    // The board's cached copy was dropped: the row now offers its ghost.
    board = await call(env, 'GET', `/api/boards/${BOARD}`);
    assert.deepEqual(board.body.entries.filter(e => !e.house).map(e => [e.name, e.ghost]), [['ADA', true], ['BOB', undefined]]);
    const got = await call(env, 'GET', `/api/ghosts/${BOARD}/ADA/60000`);
    assert.equal(got.status, 200);
    assert.deepEqual(got.body, { board: BOARD, name: 'ADA', value: 60_000, vehicle: 3, ghost: text(lap) });
    assert.equal(got.headers.get('Cache-Control'), `public, max-age=${GHOST_MAX_AGE}`);
    // The second download is the edge's copy: no storage read.
    env.SAVES.db.exec('DROP TABLE board_ghosts');
    assert.equal((await call(env, 'GET', `/api/ghosts/${BOARD}/ADA/60000`)).body.ghost, text(lap));
  } finally { delete globalThis.caches; }
});

test('a ghost needs its row: the name, the exact lap time, the vehicle and the course', async () => {
  const env = setup();
  await submit(env, 'ADA', 60_000);
  const kept = async (name, options, board) => {
    const answer = await upload(env, name, ghost(options), board);
    return answer.status === 200 ? answer.body.kept : `${answer.status} ${answer.body.reason ?? answer.body.error}`;
  };
  assert.equal(await kept('EVE', {}), false, 'no such row');
  assert.equal(await kept('ADA', { lapMs: 59_000 }), false, 'a faster lap than the row');
  assert.equal(await kept('ADA', { vehicle: 4 }), false, 'another vehicle');
  assert.equal(await kept('ADA', { course: 'nat02' }), '422 ghost_row');
  assert.equal(await kept('ADA', {}, 'lap:nat01:x'), false, 'the Expert board has no such row');
  assert.equal(await kept('ADA', { steps: 1800 }), '422 ghost_length');
  assert.equal(await kept('a/b', {}), '422 name');
  assert.deepEqual(stored(env), []);
  assert.equal(await kept('ADA', {}), true);
  // The stored ghost of a lap is not replaced by another upload for it.
  const first = env.SAVES.db.prepare('SELECT data FROM board_ghosts').get().data;
  assert.equal(await kept('ADA', { feet: 5000 }), false);
  assert.equal(env.SAVES.db.prepare('SELECT data FROM board_ghosts').get().data, first);
  // Only lap boards carry ghosts.
  assert.equal((await upload(env, 'ADA', ghost(), 'race:nat01:n')).status, 404);
  assert.equal((await call(env, 'GET', '/api/ghosts/score:spstunt:free01/ADA/5')).status, 404);
  assert.equal((await call(env, 'GET', `/api/ghosts/${BOARD}/ADA/61000`)).status, 404);
  assert.equal((await call(env, 'GET', `/api/ghosts/${BOARD}/EVE/60000`)).status, 404);
  assert.equal((await call(env, 'GET', `/api/ghosts/${BOARD}/ada/60000`)).status, 404);
  assert.equal((await call(env, 'GET', `/api/ghosts/${BOARD}/ADA/abc`)).status, 404);
  assert.equal((await call(env, 'GET', `/api/ghosts/${BOARD}/ADA`)).status, 404);
  assert.equal((await call(env, 'GET', `/api/ghosts/${BOARD}/%E0%A4%A/1`)).status, 404);
});

test('ghosts are kept for the ten best rows and one a band of five ranks, and dropped when a row leaves them or improves', async () => {
  const env = setup();
  const names = Array.from({ length: GHOST_TOP + 7 }, (_, i) => `R${i}`);
  for (const [i, name] of names.entries()) await submit(env, name, 60_000 + i * 1000);
  const lapOf = name => ghost({ lapMs: 60_000 + Number(name.slice(1)) * 1000 });
  for (const name of names.slice(0, GHOST_TOP)) assert.equal((await upload(env, name, lapOf(name))).body.kept, true, name);
  // Ranks 11-15 are one band: its best-ranked row with a ghost keeps it.
  assert.equal((await upload(env, 'R11', lapOf('R11'))).body.kept, true, 'the band\'s first ghost');
  assert.equal((await upload(env, 'R12', lapOf('R12'))).body.kept, false, 'a worse row of the same band');
  assert.equal((await upload(env, 'R10', lapOf('R10'))).body.kept, true, 'a better row of the band replaces it');
  assert.ok(!stored(env).some(row => row.startsWith('R11:')));
  // Ranks 16-20 are the next band.
  assert.equal((await upload(env, 'R16', lapOf('R16'))).body.kept, true);
  assert.equal(stored(env).length, GHOST_TOP + 2);
  // A new fastest lap pushes the tenth row into the band (it ranks ahead of
  // R10 there) and R10's ghost is deleted with the submission.
  await submit(env, 'NEW', 59_000);
  assert.deepEqual(stored(env), [...names.slice(0, GHOST_TOP), 'R16'].map(name => `${name}:${60_000 + Number(name.slice(1)) * 1000}`));
  assert.equal((await call(env, 'GET', `/api/ghosts/${BOARD}/R10/70000`)).status, 404);
  // A rider's better lap drops the old lap's ghost; the new one follows.
  await submit(env, 'R3', 58_000);
  assert.ok(!stored(env).some(row => row.startsWith('R3:')));
  let board = await call(env, 'GET', `/api/boards/${BOARD}`);
  assert.equal(board.body.entries.find(e => e.name === 'R3').ghost, undefined);
  assert.equal((await upload(env, 'R3', ghost({ lapMs: 63_000 }))).body.kept, false, 'the old lap');
  assert.equal((await upload(env, 'R3', ghost({ lapMs: 58_000 }))).body.kept, true);
  board = await call(env, 'GET', `/api/boards/${BOARD}`);
  assert.deepEqual(board.body.entries.filter(e => e.ghost).map(e => e.name), ['R3', 'R0', 'R1', 'R2', 'R4', 'R5', 'R6', 'R7', 'R8', 'R9', 'R16']);
  // A slower lap changes nothing.
  await submit(env, 'R0', 75_000);
  assert.ok(stored(env).includes('R0:60000'));
});

test('the kept ghosts: ten best, one a band down to rank 40, never more than the limit', () => {
  const rows = Array.from({ length: 50 }, (_, i) => ({ name: `R${i}`, value: 60_000 + i }));
  const all = rows.map(({ name, value }) => ({ name, value }));
  const kept = [...keptGhosts(rows, all)];
  assert.equal(GHOST_LIMIT, 16);
  assert.deepEqual(kept, ['R0', 'R1', 'R2', 'R3', 'R4', 'R5', 'R6', 'R7', 'R8', 'R9', 'R10', 'R15', 'R20', 'R25', 'R30', 'R35']);
  // A band's ghost is its best-ranked row that has one; a stale time keeps none.
  const some = [{ name: 'R13', value: 60_013 }, { name: 'R14', value: 60_014 }, { name: 'R2', value: 1 }, { name: 'R45', value: 60_045 }];
  assert.deepEqual([...keptGhosts(rows, some)], ['R13']);
});

test('ghost refusals: method, size, JSON, origin, rate limit and storage failure', async () => {
  const env = setup();
  await submit(env, 'ADA', 60_000);
  assert.equal((await call(env, 'GET', `/api/ghosts/${BOARD}`)).status, 405);
  assert.equal((await call(env, 'POST', `/api/ghosts/${BOARD}/ADA/60000`, {})).status, 405);
  assert.equal((await call(env, 'POST', `/api/ghosts/${BOARD}`, 'x'.repeat(MAX_GHOST_TEXT + 2000))).status, 413);
  assert.equal((await call(env, 'POST', `/api/ghosts/${BOARD}`, '{')).status, 400);
  assert.equal((await call(env, 'POST', `/api/ghosts/${BOARD}`, '[]')).status, 400);
  assert.equal((await call(env, 'POST', `/api/ghosts/${BOARD}`, { name: 'ADA', ghost: 'AAAA' })).body.reason, 'ghost_format');
  assert.equal((await call(env, 'POST', `/api/ghosts/${BOARD}`, { name: 'ADA', ghost: 7 })).body.reason, 'ghost_format');
  assert.equal((await call(env, 'GET', `/api/ghosts/${BOARD}/ADA/60000`, undefined, { Origin: 'https://evil.example' })).status, 403);
  assert.equal((await call(setup({ submit: 0 }), 'POST', `/api/ghosts/${BOARD}`, { name: 'ADA', ghost: text(ghost()) })).status, 429);
  assert.equal((await call(setup({ read: 0 }), 'GET', `/api/ghosts/${BOARD}/ADA/60000`)).status, 429);
  assert.equal((await call({ GAME_ORIGINS: 'https://furyremastered.com' }, 'GET', `/api/ghosts/${BOARD}/ADA/60000`)).status, 503);
  const broken = setup();
  broken.SAVES.batch = async () => { throw new Error('storage'); };
  assert.equal((await call(broken, 'POST', `/api/ghosts/${BOARD}`, { name: 'ADA', ghost: text(ghost()) })).status, 503);
});
