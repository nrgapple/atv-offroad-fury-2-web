import assert from 'node:assert/strict';
import test from 'node:test';
import worker from './worker.mjs';
import { MIN_BUCKET, STATS_PATH, courseName, refreshStats, statsRequest, summarize, usageDays, usageQueries } from './stats.mjs';
import { d1 } from './testing.mjs';

const NOW = Date.parse('2026-11-14T12:00:00Z');
const MINUTE = 60_000, DAY = 86_400_000;
const TODAY = '2026-11-14', YESTERDAY = '2026-11-13';
const clock = (start = NOW) => { let t = start; const now = () => t; now.advance = ms => { t += ms; }; return now; };
const stamp = day => `${day} 00:00:00`;

// Analytics Engine rows for one day, as the SQL API returns them (64-bit sums as strings).
function usage(day) {
  return {
    totals: [
      { day: stamp(day), event: 'session_start', n: '40', seconds: 0 },
      { day: stamp(day), event: 'ride_start', n: '30', seconds: 0 },
      { day: stamp(day), event: 'ride_time', n: '90', seconds: '36000' },
      { day: stamp(day), event: 'race_finish', n: '12', seconds: '2400' },
    ],
    splits: [
      { day: stamp(day), event: 'ride_start', mode: 'single', activity: 'race', vehicle: 'atv', n: '20', seconds: 0 },
      { day: stamp(day), event: 'ride_start', mode: 'multiplayer', activity: 'freestyle', vehicle: 'motorcycle', n: '6', seconds: 0 },
      { day: stamp(day), event: 'ride_start', mode: 'freeride', activity: 'freestyle', vehicle: 'atv', n: '4', seconds: 0 },
      { day: stamp(day), event: 'ride_time', mode: 'single', activity: 'race', vehicle: 'atv', n: '50', seconds: '18000' },
      { day: stamp(day), event: 'ride_time', mode: 'multiplayer', activity: 'freestyle', vehicle: 'motorcycle', n: '40', seconds: '18000' },
      { day: stamp(day), event: 'ride_start', mode: 'bogus', activity: '', vehicle: '', n: '3', seconds: 0 },
    ],
    venues: [
      { day: stamp(day), event: 'ride_start', map: 'env02', n: '20', seconds: 0 },
      { day: stamp(day), event: 'ride_time', map: 'env02', n: '60', seconds: '30000' },
      { day: stamp(day), event: 'race_finish', map: 'env02', n: '12', seconds: '2400' },
      { day: stamp(day), event: 'ride_start', map: 'sx12', n: '6', seconds: 0 },
      { day: stamp(day), event: 'ride_time', map: 'sx12', n: '20', seconds: '6000' },
      { day: stamp(day), event: 'ride_start', map: 'mystery', n: '4', seconds: 0 },
    ],
    sessions: [
      { day: stamp(day), country: 'US', device: 'touch', n: '25' },
      { day: stamp(day), country: 'US', device: 'desktop', n: '6' },
      { day: stamp(day), country: 'IS', device: 'desktop', n: '1' },
      { day: stamp(day), country: 'XX', device: 'desktop', n: '2' },
      { day: stamp(day), country: '', device: 'desktop', n: '6' },
    ],
  };
}

const kind = sql => (sql.includes('LIMIT 1') ? 'first' : sql.includes('blob2 AS country') ? 'sessions' : sql.includes('blob9 AS map') ? 'venues' : sql.includes('blob7 AS mode') ? 'splits' : 'totals');

// A fetcher answering the Analytics Engine SQL API from `days` and /healthz from `health`.
function network({ days = [TODAY], health = { ok: true, commit: 'a'.repeat(40), protocol: 6, rooms: 2, riders: 5, connections: 7 }, fail = false } = {}) {
  const calls = [];
  const fetcher = async (url, init = {}) => {
    calls.push({ url: String(url), init });
    if (String(url).includes('/analytics_engine/sql')) {
      if (fail) return new Response('nope', { status: 500 });
      const which = kind(init.body);
      if (which === 'first') return Response.json({ data: days.length ? [{ day: stamp([...days].sort()[0]) }] : [] });
      return Response.json({ data: days.flatMap(day => usage(day)[which]) });
    }
    if (String(url).endsWith('/healthz')) return health ? Response.json(health) : new Response('down', { status: 502 });
    throw new Error(`unexpected fetch ${url}`);
  };
  return { fetcher, calls, sql: () => calls.filter(call => call.url.includes('analytics_engine')) };
}

const env = (extra = {}) => ({ SAVES: d1(), GAME_SERVER_URL: 'https://mp.example', ANALYTICS_API_TOKEN: 'test-token', ...extra });
const stored = db => db.db.prepare('SELECT day, data, updated_at FROM stats_days ORDER BY day').all().map(row => ({ ...row, data: JSON.parse(row.data) }));
const check = (db, at, riders, rooms, ok = 1) => db.db.prepare(`INSERT INTO game_server_checks (checked_at, ok, status, latency_ms, build, rooms, riders, connections, error)
  VALUES (?, ?, 200, 30, 'b0b0b0b0', ?, ?, 99, NULL)`).run(at, ok, rooms, riders);
const board = (db, board, name, value = 60_000, day = TODAY) => db.db.prepare('INSERT INTO board_entries (board, name, value, vehicle, day) VALUES (?, ?, ?, 3, ?)').run(board, name, value, day);
const read = async (config, fetcher, now = clock()) => (await statsRequest(new Request(`https://furyremastered.com${STATS_PATH}`), config, { fetcher, now })).json();

test('course names use the original venue names and race variants', () => {
  assert.equal(courseName('env02'), 'Tioga Valley');
  assert.equal(courseName('env05ends'), 'Prospect Springs Enduro Short');
  assert.equal(courseName('env01st'), 'Caribou Peak Short Track');
  assert.equal(courseName('sx12'), 'Amherst');
  assert.equal(courseName('free05'), 'Vancouver Ridge');
  assert.equal(courseName('nope'), null);
});

test('queries cover whole UTC days and weight every count by the sample interval', () => {
  const queries = usageQueries('2026-11-01', TODAY);
  assert.deepEqual(Object.keys(queries), ['totals', 'splits', 'venues', 'sessions']);
  for (const sql of Object.values(queries)) {
    assert.match(sql, /timestamp >= toDateTime\('2026-11-01 00:00:00'\) AND timestamp < toDateTime\('2026-11-15 00:00:00'\)/);
    assert.match(sql, /SUM\(_sample_interval\)/);
    assert.doesNotMatch(sql, /blob3/, 'the release column is never read');
  }
});

test('rows roll up into per-day aggregates; unknown values are dropped', () => {
  const days = usageDays(YESTERDAY, TODAY, usage(TODAY));
  assert.deepEqual(Object.keys(days), [YESTERDAY, TODAY]);
  assert.equal(days[YESTERDAY].rides, 0);
  const day = days[TODAY];
  assert.deepEqual([day.sessions, day.rides, day.seconds, day.finishes], [40, 30, 36000, 12]);
  assert.deepEqual(day.modes, { single: [20, 18000], multiplayer: [6, 18000], freeride: [4, 0] });
  assert.deepEqual(day.vehicles, { atv: [24, 18000], motorcycle: [6, 18000] });
  assert.deepEqual(day.maps, { env02: [20, 30000, 12], sx12: [6, 6000, 0] });
  assert.deepEqual(day.countries, { US: 31, IS: 1 });
  assert.deepEqual(day.devices, { touch: 25, desktop: 15 });
});

test('a first refresh backfills in chunks, then refreshes today and yesterday at most every 15 minutes', async () => {
  const config = env();
  const { fetcher, sql } = network({ days: [TODAY, YESTERDAY, '2026-08-20'] });
  const now = clock();
  check(config.SAVES, NOW - 10 * MINUTE, 9, 3);
  check(config.SAVES, NOW - DAY, 14, 4);

  for (let back = 2; back <= 29; back++) check(config.SAVES, NOW - back * DAY, 3, 1);
  const prepared = config.SAVES.prepared;
  const first = await refreshStats(config, { fetcher, now });
  assert.deepEqual(first.usage, { from: '2026-08-20', to: '2026-09-02' }, 'the backfill starts at the first day with events');
  assert.ok(config.SAVES.prepared - prepared <= 35, 'a run stays well inside the free plan D1 query limit');
  assert.match(sql()[0].init.body, /INTERVAL '90' DAY GROUP BY day ORDER BY day LIMIT 1/);
  assert.equal(sql().length, 5);
  for (const call of sql()) {
    assert.equal(call.init.method, 'POST');
    assert.equal(call.init.headers.Authorization, 'Bearer test-token');
    assert.match(call.url, /accounts\/92b7e16fd24d5fb585f866f5832a032b\/analytics_engine\/sql$/);
    assert.match(call.init.body, / FORMAT JSON$/);
  }
  let rows = stored(config.SAVES);
  assert.equal(rows.find(row => row.day === '2026-08-20').data.rides, 30);
  assert.equal(rows.find(row => row.day === YESTERDAY).data.peakRiders, 14);
  assert.equal(rows.find(row => row.day === TODAY).data.peakRiders, 9);
  assert.equal(rows.find(row => row.day === TODAY).data.usage, undefined, 'today is not counted until the backfill reaches it');

  // The backfill continues every run until it reaches today.
  let runs = 1;
  while (stored(config.SAVES).find(row => row.day === TODAY).data.usage !== 1) {
    now.advance(5 * MINUTE);
    assert.ok(await refreshStats(config, { fetcher, now }));
    assert.ok(++runs < 10);
  }
  assert.equal(runs, 7);
  rows = stored(config.SAVES);
  assert.equal(rows.find(row => row.day === TODAY).data.peakRiders, 9, 'usage rows keep the recorded peak');
  assert.equal(rows.find(row => row.day === YESTERDAY).data.rides, 30);

  // Caught up: the next runs wait 15 minutes, then refresh only yesterday and today.
  const before = sql().length;
  now.advance(5 * MINUTE);
  assert.equal(await refreshStats(config, { fetcher, now }), null);
  now.advance(10 * MINUTE);
  const later = await refreshStats(config, { fetcher, now });
  assert.deepEqual(later.usage, { from: YESTERDAY, to: TODAY });
  assert.equal(sql().length, before + 4);
});

test('without the analytics token the rollup keeps peaks and board counts and never queries Analytics Engine', async () => {
  const config = env({ ANALYTICS_API_TOKEN: undefined });
  const { fetcher, calls } = network();
  check(config.SAVES, NOW - 5 * MINUTE, 6, 2);
  board(config.SAVES, 'lap:env02:n', 'SAM');
  board(config.SAVES, `daily:${TODAY}`, 'SAM');
  const now = clock();
  assert.deepEqual(await refreshStats(config, { fetcher, now }), { days: 1, usage: null });
  assert.equal(calls.length, 0);
  const [today] = stored(config.SAVES);
  assert.deepEqual(today.data, { peakRiders: 6, peakRooms: 2, boards: { times: 1, riders: 1, boards: 1, daily: 1, today: 1 } });
  now.advance(5 * MINUTE);
  assert.equal(await refreshStats(config, { fetcher, now }), null, 'throttled');
  assert.equal(await refreshStats({}, { fetcher, now }), null, 'no database');
});

test('a failed Analytics Engine query is logged without its detail and keeps the stored totals', async () => {
  const config = env();
  await refreshStats(config, { fetcher: network({ days: ['2026-08-20'] }).fetcher, now: clock() });
  const errors = [], original = console.error;
  console.error = (...args) => errors.push(args.join(' '));
  try {
    const result = await refreshStats(config, { fetcher: network({ fail: true }).fetcher, now: clock(NOW + 5 * MINUTE) });
    assert.equal(result.usage, null);
  } finally { console.error = original; }
  assert.equal(errors.length, 1);
  assert.doesNotMatch(errors[0], /test-token/);
  assert.ok(stored(config.SAVES).some(row => row.data.rides === 30), 'earlier usage rows survive');
});

test('the endpoint serves totals, splits and venues and suppresses small buckets', async () => {
  const config = env();
  const days = [TODAY, YESTERDAY, '2026-11-01'];
  const { fetcher } = network({ days });
  const now = clock();
  check(config.SAVES, NOW - 3 * DAY, 21, 6);
  for (let i = 0; i < 8; i++) { await refreshStats(config, { fetcher, now }); now.advance(5 * MINUTE); }
  const body = await read(config, fetcher, clock());
  assert.deepEqual(body.live, { riders: 5, rooms: 2 });
  assert.deepEqual(body.peak, { riders: 21, day: '2026-11-11', today: 0 });
  assert.equal(body.since, '2026-11-01');
  assert.deepEqual(body.totals.today, { rides: 30, hours: 10, sessions: 40, racesFinished: 12 });
  assert.deepEqual(body.totals.week, { rides: 60, hours: 20, sessions: 80, racesFinished: 24 });
  assert.deepEqual(body.totals.allTime, { rides: 90, hours: 30, sessions: 120, racesFinished: 36 });
  assert.deepEqual(body.modes, [
    { id: 'single', rides: 60, hours: 15 }, { id: 'multiplayer', rides: 18, hours: 15 }, { id: 'freeride', rides: 12, hours: 0 },
  ]);
  assert.deepEqual(body.vehicles.map(item => item.id), ['atv', 'motorcycle']);
  assert.deepEqual(body.devices, [{ id: 'touch', sessions: 75 }, { id: 'desktop', sessions: 45 }]);
  assert.deepEqual(body.venues, [
    { id: 'env02', name: 'Tioga Valley', rides: 60, hours: 25, racesFinished: 36 },
    { id: 'sx12', name: 'Amherst', rides: 18, hours: 5, racesFinished: 0 },
  ]);
  // Two countries rode; only the one with at least MIN_BUCKET sessions is named.
  assert.deepEqual(body.countries, { count: 2, top: [{ code: 'US', sessions: 93 }] });
  assert.equal(body.days.length, 14, 'days since the first recorded day');
  assert.deepEqual(body.days.at(-1), { day: TODAY, rides: 30, hours: 10 });
  assert.equal(body.daily.course, courseName((await import('./boards.mjs')).dailyPick(TODAY).course));
});

test('splits and lists stay hidden below the minimum bucket', () => {
  const day = { usage: 1, sessions: 3, rides: 4, seconds: 600, finishes: 1, modes: { single: [4, 600] }, activities: { race: [4, 600] },
    vehicles: { atv: [4, 600] }, devices: { touch: 3 }, maps: { env01: [4, 600, 1] }, countries: { NZ: 3 } };
  const body = summarize([{ day: TODAY, data: JSON.stringify(day) }], { at: NOW });
  assert.equal(body.totals.allTime.rides, 4);
  assert.equal(MIN_BUCKET, 10);
  for (const field of ['modes', 'activities', 'vehicles', 'devices']) assert.equal(body[field], null, field);
  assert.deepEqual(body.venues, []);
  assert.deepEqual(body.countries, { count: 1, top: [] });
});

test('no forbidden fields: no rider names, server details, releases or connections', async () => {
  const config = env();
  const { fetcher } = network({ days: [TODAY] });
  check(config.SAVES, NOW - 5 * MINUTE, 3, 1);
  board(config.SAVES, 'lap:env02:n', 'ZORRO');
  board(config.SAVES, `daily:${TODAY}`, 'QUILL');
  await refreshStats(config, { fetcher, now: clock() });
  const text = JSON.stringify(await read(config, fetcher));
  for (const word of ['ZORRO', 'QUILL', 'commit', 'protocol', 'connections', 'build', 'release', 'relay', 'server', 'latency', 'b0b0b0b0', 'aaaaaaaa', 'mp.example', 'IS"']) {
    assert.ok(!text.includes(word), `${word} leaked`);
  }
  const body = JSON.parse(text);
  assert.deepEqual(Object.keys(body.live), ['riders', 'rooms']);
  assert.deepEqual(body.boards, { times: 1, riders: 1, boards: 1, dailyEntries: 1 });
  assert.equal(body.daily.entries, 1);
});

test('degrades without the database, the token or the game server', async () => {
  // Nothing configured: only the day's event pick.
  const bare = await read({}, network({ health: null }).fetcher);
  assert.equal(bare.live, null);
  assert.equal(bare.peak, null);
  assert.equal(bare.boards, null);
  assert.equal(bare.totals, undefined);
  assert.equal(typeof bare.daily.course, 'string');
  assert.equal(bare.daily.entries, null);

  // Game server unreachable: a recent health check stands in; a stale one does not.
  const config = env({ ANALYTICS_API_TOKEN: undefined });
  check(config.SAVES, NOW - 4 * MINUTE, 4, 1);
  assert.deepEqual((await read(config, network({ health: null }).fetcher)).live, { riders: 4, rooms: 1 });
  assert.equal((await read(config, network({ health: null }).fetcher, clock(NOW + 20 * MINUTE))).live, null);
  const throwing = async () => { throw new TypeError('network'); };
  assert.deepEqual((await read(config, throwing)).live, { riders: 4, rooms: 1 });

  // A broken stats table still answers.
  const broken = env();
  broken.SAVES.db.exec('DROP TABLE stats_days');
  const original = console.error; console.error = () => {};
  try { assert.deepEqual((await read(broken, network().fetcher)).live, { riders: 5, rooms: 2 }); } finally { console.error = original; }
});

test('through the Worker: GET and HEAD only, cached for a minute at the edge', async () => {
  const store = new Map();
  globalThis.caches = { default: { match: async r => store.get(r.url)?.clone(), put: async (r, response) => { store.set(r.url, response); } } };
  const original = globalThis.fetch;
  let healthReads = 0;
  globalThis.fetch = async () => { healthReads++; return Response.json({ ok: true, rooms: 1, riders: 2 }); };
  try {
    const config = { SAVES: d1(), GAME_SERVER_URL: 'https://mp.example' };
    const response = await worker.fetch(new Request('https://furyremastered.com/api/stats?x=1'), config);
    assert.equal(response.status, 200);
    assert.equal(response.headers.get('Content-Type'), 'application/json');
    assert.equal(response.headers.get('Cache-Control'), 'public, max-age=60');
    assert.ok(response.headers.get('Content-Security-Policy'));
    assert.deepEqual((await response.json()).live, { riders: 2, rooms: 1 });
    await worker.fetch(new Request('https://furyremastered.com/api/stats'), config);
    assert.equal(healthReads, 1, 'the second read comes from the cache');
    const head = await worker.fetch(new Request('https://furyremastered.com/api/stats', { method: 'HEAD' }), config);
    assert.equal(await head.text(), '');
    assert.equal((await worker.fetch(new Request('https://furyremastered.com/api/stats', { method: 'POST' }), config)).status, 405);
  } finally { globalThis.fetch = original; delete globalThis.caches; }
});

test('the cron runs the stats rollup after the health check', async () => {
  const pending = [], config = { SAVES: d1(), GAME_SERVER_URL: 'https://mp.example' };
  const original = globalThis.fetch;
  globalThis.fetch = async () => Response.json({ ok: true, rooms: 3, riders: 8, connections: 9 });
  try {
    worker.scheduled({ cron: '*/5 * * * *' }, config, { waitUntil: promise => pending.push(promise) });
    await Promise.all(pending);
  } finally { globalThis.fetch = original; }
  const [today] = stored(config.SAVES);
  assert.equal(today.data.peakRiders, 8);
});
