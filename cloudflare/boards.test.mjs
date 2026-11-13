import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { DatabaseSync } from 'node:sqlite';
import test from 'node:test';
import worker from './worker.mjs';
import { DAILY_POOL, TOP, checkTrace, dailyPick, judge, parseBoard } from './boards.mjs';

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

// A race on a circular course: `laps` laps of `lapSeconds` each, one point a
// second around a loop of `feet`.
function race(laps, lapSeconds, feet = 4000) {
  const radius = feet / (2 * Math.PI), total = laps * lapSeconds, points = [];
  for (let t = 0; t <= total; t++) {
    const angle = (2 * Math.PI * t) / lapSeconds;
    points.push([Math.round(radius * Math.cos(angle)), Math.round(radius * Math.sin(angle))]);
  }
  return { points, marks: Array.from({ length: laps }, (_, i) => (i + 1) * lapSeconds * 1000) };
}
const lapRun = (overrides = {}) => ({ name: 'ADA', vehicle: 3, course: 'nat01', lapMs: 60_000, raceMs: 300_000, trace: race(5, 60), ...overrides });
const NOW = Date.parse('2026-11-11T12:00:00Z');

test('board ids name known courses and kinds only', () => {
  assert.deepEqual(parseBoard('lap:nat01:n'), { kind: 'lap', course: 'nat01', expert: false });
  assert.deepEqual(parseBoard('race:env05ends:x'), { kind: 'race', course: 'env05ends', expert: true });
  assert.deepEqual(parseBoard('score:spstunt:free01'), { kind: 'score', series: 'spstunt', course: 'free01' });
  assert.equal(parseBoard('daily:2026-11-11').kind, 'daily');
  for (const bad of ['lap:nat01', 'lap:zzz01:n', 'score:spstunt:env05', 'daily:2026-13-45', 'lap:nat01:n;drop', '']) assert.equal(parseBoard(bad), null, bad);
});

test('the daily pick is a pure function of the UTC date', () => {
  const pick = dailyPick('2026-11-11');
  assert.deepEqual(dailyPick('2026-11-11'), pick);
  assert.ok(DAILY_POOL.includes(pick.course));
  // Shared with atv_core::world_board (its test pins the same dates).
  assert.deepEqual([dailyPick('2026-11-11'), dailyPick('2026-11-12'), dailyPick('2027-01-01')].map(p => `${p.course}/${p.vehicleClass}`), DAILY_EXPECTED);
  const seen = new Set(Array.from({ length: 365 }, (_, i) => dailyPick(new Date(NOW + i * 86_400_000).toISOString().slice(0, 10)).course));
  assert.ok(seen.size > 30, `a year visits most courses (${seen.size})`);
});
const DAILY_EXPECTED = ['env01endl/atv', 'env05/atv', 'env02/atv'];

test('plausibility: floors, traces, names, vehicles and scores', () => {
  assert.equal(judge(lapRun(), NOW).reason, undefined);
  assert.equal(judge(lapRun({ name: 'ada ' }), NOW).name, 'ADA');
  const reason = overrides => judge(lapRun(overrides), NOW).reason;
  assert.equal(reason({ name: '@0' }), 'name');
  assert.equal(reason({ name: 'TOOLONGNAME' }), 'name');
  assert.equal(reason({ name: 'A<b>' }), 'name');
  assert.equal(reason({ vehicle: 99 }), 'vehicle');
  assert.equal(reason({ course: 'mars01' }), 'board');
  // A 10 s lap is below the Nationals floor, whatever the trace says.
  assert.equal(reason({ lapMs: 10_000, raceMs: undefined, trace: race(1, 10, 1000) }), 'lap_floor');
  // The lap must be one of the trace's laps.
  assert.equal(reason({ lapMs: 59_000 }), 'lap_marks');
  // The trace covers the run, one point a second.
  assert.equal(reason({ trace: { ...race(5, 60), points: race(5, 60).points.slice(0, 100) } }), 'trace_length');
  // Teleporting is refused unless the client marks it as a reset.
  const jumped = race(5, 60); jumped.points[30] = [9000, 9000];
  assert.equal(reason({ trace: jumped }), 'trace_speed');
  // 40,000 ft laps in 60 s are 666 ft/s.
  assert.equal(reason({ trace: { ...race(5, 60, 40_000) } }), 'trace_speed');
  // Riding in place does not make a lap.
  assert.equal(reason({ trace: race(5, 60, 100) }), 'lap_short');
  assert.equal(reason({ raceMs: 290_000 }), 'race_marks');
  assert.equal(checkTrace({ points: [[0, 0], [1, 1]], marks: [], resets: [5] }, 1), 'trace');
  // A reset point skips its step.
  const reset = race(5, 60); reset.points[30] = [900, 900]; reset.points[31] = [900, 900];
  assert.equal(reason({ trace: { ...reset, resets: [30, 32] }, lapMs: 60_000 }), undefined);
  const freestyle = { name: 'ADA', vehicle: 3, series: 'spstunt', course: 'free01', score: 50_000, seconds: 120, trace: race(1, 120) };
  assert.deepEqual(judge(freestyle, NOW).entries, [['score:spstunt:free01', -50_000]]);
  assert.equal(judge({ ...freestyle, score: 5_000_000 }, NOW).reason, 'score');
  assert.equal(judge({ ...freestyle, seconds: 600 }, NOW).reason, 'seconds');
});

test('the daily board takes only the day\'s pick, today', () => {
  const pick = dailyPick('2026-11-11');
  const vehicle = pick.vehicleClass === 'atv' ? 3 : 30;
  const laps = /end[sl]$/.test(pick.course) ? 3 : 5;
  const run = { name: 'ADA', vehicle, course: pick.course, raceMs: laps * 60_000, trace: race(laps, 60), daily: '2026-11-11' };
  assert.deepEqual(judge(run, NOW).entries.map(e => e[0]), [`race:${pick.course}:n`, 'daily:2026-11-11']);
  assert.equal(judge({ ...run, daily: '2026-11-10' }, NOW).reason, 'daily_date');
  assert.equal(judge({ ...run, daily: '2026-11-10' }, Date.parse('2026-11-11T00:10:00Z')).reason, 'daily_pick', 'grace for yesterday, but its own pick');
  assert.equal(judge({ ...run, expert: true }, NOW).reason, 'daily_pick');
  assert.equal(judge({ ...run, vehicle: pick.vehicleClass === 'atv' ? 30 : 3 }, NOW).reason, 'daily_pick');
});

test('submit ranks, keeps each name\'s best and reads the cached board', async () => {
  const env = setup();
  let result = await call(env, 'POST', '/api/boards/submit', lapRun());
  assert.equal(result.status, 200);
  assert.deepEqual(result.body.boards['lap:nat01:n'], { rank: 1, placed: true });
  assert.deepEqual(result.body.boards['race:nat01:n'], { rank: 1, placed: true });
  // A slower run by the same name changes nothing.
  result = await call(env, 'POST', '/api/boards/submit', lapRun({ lapMs: 61_000, raceMs: 305_000, trace: race(5, 61) }));
  assert.deepEqual(result.body.boards['lap:nat01:n'], { rank: 1, placed: false });
  result = await call(env, 'POST', '/api/boards/submit', lapRun({ name: 'BOB', lapMs: 55_000, raceMs: 275_000, trace: race(5, 55) }));
  assert.equal(result.body.boards['lap:nat01:n'].rank, 1);
  result = await call(env, 'GET', '/api/boards/lap:nat01:n');
  assert.equal(result.status, 200);
  assert.match(result.headers.get('Cache-Control'), /public, max-age=60/);
  assert.deepEqual(result.body.entries.map(e => [e.rank, e.name, e.value, e.vehicle]), [[1, 'BOB', 55_000, 3], [2, 'ADA', 60_000, 3]]);
  assert.match(result.body.entries[0].day, /^\d{4}-\d{2}-\d{2}$/);
  assert.deepEqual(Object.keys(result.body.entries[0]).sort(), ['day', 'name', 'rank', 'value', 'vehicle']);
  result = await call(env, 'GET', '/api/boards/lap:nat01:x');
  assert.deepEqual(result.body.entries, []);
});

test('a board keeps the top 50', async () => {
  const env = setup();
  for (let i = 0; i < TOP + 5; i++) {
    const lap = 40 + i;
    const result = await call(env, 'POST', '/api/boards/submit', { name: `R${i}`, vehicle: 1, course: 'nat02', lapMs: lap * 1000, trace: race(1, lap) });
    assert.equal(result.status, 200, JSON.stringify(result.body));
  }
  const slow = await call(env, 'POST', '/api/boards/submit', { name: 'SLOW', vehicle: 1, course: 'nat02', lapMs: 200_000, trace: race(1, 200, 8000) });
  assert.deepEqual(slow.body.boards['lap:nat02:n'], { rank: null, placed: false });
  const fast = await call(env, 'POST', '/api/boards/submit', { name: 'FAST', vehicle: 1, course: 'nat02', lapMs: 30_000, trace: race(1, 30) });
  assert.deepEqual(fast.body.boards['lap:nat02:n'], { rank: 1, placed: true });
  const count = env.SAVES.db.prepare("SELECT COUNT(*) AS n FROM board_entries WHERE board = 'lap:nat02:n'").get().n;
  assert.equal(count, TOP);
  const board = await call(env, 'GET', '/api/boards/lap:nat02:n');
  assert.equal(board.body.entries.length, TOP);
  assert.equal(board.body.entries.at(-1).name, 'R48');
});

test('freestyle boards rank the highest score first', async () => {
  const env = setup();
  const run = (name, score) => call(env, 'POST', '/api/boards/submit', { name, vehicle: 2, series: 'unspstunt', course: 'env05', score, seconds: 120, trace: race(1, 120) });
  await run('LOW', 1000);
  assert.equal((await run('HIGH', 90_000)).body.boards['score:unspstunt:env05'].rank, 1);
  const board = await call(env, 'GET', '/api/boards/score:unspstunt:env05');
  assert.deepEqual(board.body.entries.map(e => [e.name, e.value]), [['HIGH', 90_000], ['LOW', 1000]]);
});

test('daily board reads carry the pick and yesterday\'s winner', async () => {
  const env = setup();
  env.SAVES.db.prepare("INSERT INTO board_entries VALUES ('daily:2026-11-10', 'WIN', 123456, 4, '2026-11-10')").run();
  const board = await call(env, 'GET', '/api/boards/daily:2026-11-11');
  assert.equal(board.status, 200);
  assert.deepEqual(board.body.pick, dailyPick('2026-11-11'));
  assert.deepEqual(board.body.yesterday, { date: '2026-11-10', name: 'WIN', value: 123456, vehicle: 4 });
});

test('refusals: method, size, JSON, origin, rate limit and storage failure', async () => {
  const env = setup({ submit: 3 });
  assert.equal((await call(env, 'GET', '/api/boards/submit')).status, 405);
  assert.equal((await call(env, 'POST', '/api/boards/lap:nat01:n')).status, 405);
  assert.equal((await call(env, 'GET', '/api/boards/lap:nowhere:n')).status, 404);
  assert.equal((await call(env, 'POST', '/api/boards/submit', 'x'.repeat(30_000))).status, 413);
  assert.equal((await call(env, 'POST', '/api/boards/submit', '{nope')).status, 400);
  assert.equal((await call(env, 'POST', '/api/boards/submit', lapRun({ lapMs: 1 }))).body.error, 'implausible');
  assert.equal((await call(env, 'POST', '/api/boards/submit', lapRun(), { Origin: 'https://evil.example' })).status, 403);
  const limited = await call(env, 'POST', '/api/boards/submit', lapRun());
  assert.equal(limited.status, 429);
  assert.equal(limited.headers.get('Retry-After'), '60');
  const broken = { ...setup(), SAVES: { prepare() { throw new Error('down'); }, batch() { throw new Error('down'); } } };
  assert.equal((await call(broken, 'GET', '/api/boards/lap:nat01:n')).status, 503);
  assert.equal((await call({ ...setup(), BOARD_READ_LIMIT: undefined }, 'GET', '/api/boards/lap:nat01:n')).status, 503);
});
