// Public stats page (research/ANALYTICS.md, "Public stats page"). The five-
// minute cron rolls the Analytics Engine dataset, the game server health rows
// and the World boards up into one D1 row per UTC day (at most every 15
// minutes); GET /api/stats sums those rows, adds the live rider and room
// counts and serves aggregates only, cached for a minute. Nothing names a
// rider, room, device, release or the multiplayer infrastructure, and lists
// keep only buckets of at least MIN_BUCKET.
import { dailyPick, dayOf } from './boards.mjs';

export const STATS_PATH = '/api/stats';
// The smallest venue or country bucket the page lists, and the smallest total a split is shown for.
export const MIN_BUCKET = 10;
const MAX_AGE = 60;
const REFRESH_MS = 15 * 60_000;
// Analytics Engine keeps three months; a first run backfills them a chunk per cron run.
const BACKFILL_DAYS = 90;
const CHUNK_DAYS = 14;
const CHART_DAYS = 30;
const LIVE_TIMEOUT_MS = 3_000;
const LIVE_STALE_MS = 15 * 60_000;
const QUERY_TIMEOUT_MS = 20_000;
const DAY = 86_400_000;
// wrangler.jsonc account_id; the Worker cannot read it from its config.
const ACCOUNT = '92b7e16fd24d5fb585f866f5832a032b';
const DATASET = 'fury_usage';

// Venue display names (the original tracks.dat names for each launch id).
export const VENUES = Object.freeze({
  env01: 'Caribou Peak', env02: 'Tioga Valley', env03: 'Sonora Dunes', env04: 'Freeport Harbor',
  env05: 'Prospect Springs', env06: 'Rockbridge Park', env07: 'San Jacinto Isles', env08: 'Arlington Stream',
  free01: 'Tempe', free02: 'Nassau', free03: 'Crescent Hills', free04: 'St. Croix Sound', free05: 'Vancouver Ridge', free06: 'Wheeler Junction',
  nat01: 'White Sands Raceway', nat02: 'Saguaro Wash', nat03: 'Anaconda Run', nat04: 'Palisades Basin',
  nat05: 'Newcastle Landing', nat06: 'Hawthorne Mills', nat07: 'Alpine Skyway', nat08: 'Monterey Trail',
  sx01: 'Sausalito', sx02: 'Lafayette', sx03: 'Battle Creek', sx04: 'Williamsburg', sx05: 'Washington', sx06: 'Providence',
  sx07: 'Trenton', sx08: 'Greenfield', sx09: 'Fairbanks', sx10: 'Prescott', sx11: 'Kingston', sx12: 'Amherst',
  sx13: 'Concord', sx14: 'Ventura', sx15: 'Lincoln', sx16: 'Roanoke', training: 'Training Course', game01: 'Coonce Arena',
});
const VARIANTS = { ends: 'Enduro Short', endl: 'Enduro Long', st: 'Short Track' };
const SPLITS = { modes: ['single', 'multiplayer', 'freeride'], activities: ['race', 'freestyle'], vehicles: ['atv', 'motorcycle'] };
const SPLIT_COLUMNS = { modes: 'mode', activities: 'activity', vehicles: 'vehicle' };
const DEVICES = ['touch', 'desktop'];

export function courseName(course) {
  const match = /^([a-z]+\d+)(ends|endl|st)?$/.exec(course ?? '');
  const venue = match && VENUES[match[1]];
  if (!venue) return null;
  return match[2] ? `${venue} ${VARIANTS[match[2]]}` : venue;
}

const shift = (day, days) => dayOf(Date.parse(`${day}T00:00:00Z`) + days * DAY);
const number = value => { const n = Number(value); return Number.isFinite(n) && n > 0 ? n : 0; };
const whole = value => Math.round(number(value));
const add = (target, key, values) => { const row = target[key] ??= values.map(() => 0); values.forEach((v, i) => { row[i] += v; }); };

// --- Rollup (cron) ---------------------------------------------------------

async function analytics(env, sql, fetcher) {
  const response = await fetcher(`https://api.cloudflare.com/client/v4/accounts/${env.ANALYTICS_ACCOUNT_ID || ACCOUNT}/analytics_engine/sql`, {
    method: 'POST', body: `${sql} FORMAT JSON`, signal: AbortSignal.timeout(QUERY_TIMEOUT_MS),
    headers: { Authorization: `Bearer ${env.ANALYTICS_API_TOKEN}`, 'Content-Type': 'text/plain' },
  });
  if (!response.ok) throw new Error(`Analytics Engine query: HTTP ${response.status}`);
  return (await response.json())?.data ?? [];
}

// One query per breakdown for whole UTC days [from, to], weighted by _sample_interval.
export function usageQueries(from, to) {
  const where = `timestamp >= toDateTime('${from} 00:00:00') AND timestamp < toDateTime('${shift(to, 1)} 00:00:00')`;
  const rides = `(blob1 = 'ride_start' OR blob1 = 'ride_time')`;
  const day = 'toStartOfDay(timestamp) AS day';
  const sums = 'SUM(_sample_interval) AS n, SUM(_sample_interval * double2) AS seconds';
  return {
    totals: `SELECT ${day}, blob1 AS event, ${sums} FROM ${DATASET} WHERE ${where} GROUP BY day, event`,
    splits: `SELECT ${day}, blob1 AS event, blob7 AS mode, blob8 AS activity, blob10 AS vehicle, ${sums} FROM ${DATASET} WHERE ${where} AND ${rides} GROUP BY day, event, mode, activity, vehicle`,
    venues: `SELECT ${day}, blob1 AS event, blob9 AS map, ${sums} FROM ${DATASET} WHERE ${where} AND (${rides.slice(1, -1)} OR blob1 = 'race_finish') GROUP BY day, event, map`,
    sessions: `SELECT ${day}, blob2 AS country, blob4 AS device, SUM(_sample_interval) AS n FROM ${DATASET} WHERE ${where} AND blob1 = 'session_start' GROUP BY day, country, device`,
  };
}

// Usage fields of each day in [from, to] from the query rows.
export function usageDays(from, to, results) {
  const days = {};
  for (let day = from; day <= to; day = shift(day, 1)) {
    days[day] = { usage: 1, sessions: 0, rides: 0, seconds: 0, finishes: 0, modes: {}, activities: {}, vehicles: {}, devices: {}, maps: {}, countries: {} };
  }
  const each = (rows, visit) => { for (const row of rows ?? []) { const target = days[String(row.day ?? '').slice(0, 10)]; if (target) visit(target, row); } };
  each(results.totals, (d, row) => {
    if (row.event === 'session_start') d.sessions += whole(row.n);
    else if (row.event === 'ride_start') d.rides += whole(row.n);
    else if (row.event === 'ride_time') d.seconds += whole(row.seconds);
    else if (row.event === 'race_finish') d.finishes += whole(row.n);
  });
  each(results.splits, (d, row) => {
    const values = row.event === 'ride_start' ? [whole(row.n), 0] : [0, whole(row.seconds)];
    for (const [field, allowed] of Object.entries(SPLITS)) {
      const key = row[SPLIT_COLUMNS[field]];
      if (allowed.includes(key)) add(d[field], key, values);
    }
  });
  each(results.venues, (d, row) => {
    if (!VENUES[row.map]) return;
    add(d.maps, row.map, row.event === 'ride_start' ? [whole(row.n), 0, 0] : row.event === 'ride_time' ? [0, whole(row.seconds), 0] : [0, 0, whole(row.n)]);
  });
  each(results.sessions, (d, row) => {
    const n = whole(row.n);
    if (DEVICES.includes(row.device)) d.devices[row.device] = (d.devices[row.device] ?? 0) + n;
    if (/^[A-Z]{2}$/.test(row.country ?? '') && !['XX', 'T1'].includes(row.country)) d.countries[row.country] = (d.countries[row.country] ?? 0) + n;
  });
  return days;
}

async function boardCounts(db, today) {
  return db.prepare(`SELECT
      SUM(CASE WHEN board NOT LIKE 'daily:%' THEN 1 ELSE 0 END) AS times,
      COUNT(DISTINCT CASE WHEN board NOT LIKE 'daily:%' THEN name END) AS riders,
      COUNT(DISTINCT CASE WHEN board NOT LIKE 'daily:%' THEN board END) AS boards,
      SUM(CASE WHEN board LIKE 'daily:%' THEN 1 ELSE 0 END) AS daily,
      SUM(CASE WHEN board = ?1 THEN 1 ELSE 0 END) AS today
    FROM board_entries`).bind(`daily:${today}`).first();
}

// The cron's stats work: returns what it refreshed, or null when it had nothing to do.
export async function refreshStats(env, { fetcher = fetch, now = Date.now } = {}) {
  const db = env.SAVES;
  if (!db) return null;
  const at = now(), today = dayOf(at), yesterday = shift(today, -1);
  const state = await db.prepare(`SELECT
      (SELECT MAX(day) FROM stats_days WHERE json_extract(data, '$.usage') = 1) AS usage,
      (SELECT updated_at FROM stats_days WHERE day = ?1) AS updated,
      (SELECT COUNT(*) FROM stats_days) AS days`).bind(today).first();
  const token = Boolean(env.ANALYTICS_API_TOKEN);
  const caughtUp = !token || state?.usage === today;
  if (caughtUp && state?.updated && at - state.updated < REFRESH_MS) return null;

  let usage = null, from = null, to = null;
  if (token) {
    from = state?.usage ? (state.usage < yesterday ? state.usage : yesterday) : shift(today, 1 - BACKFILL_DAYS);
    to = shift(from, CHUNK_DAYS - 1) < today ? shift(from, CHUNK_DAYS - 1) : today;
    try {
      const queries = usageQueries(from, to);
      const names = Object.keys(queries);
      const rows = await Promise.all(names.map(name => analytics(env, queries[name], fetcher)));
      usage = usageDays(from, to, Object.fromEntries(names.map((name, i) => [name, rows[i]])));
    } catch (failure) {
      console.error('stats usage query failed', String(failure?.message ?? failure).slice(0, 200));
    }
  }
  // Peak riders online per day from the health rows. A first run reaches back
  // CHUNK_DAYS so one run stays well inside the free plan's 50 D1 queries.
  const peakFrom = state?.days ? yesterday : shift(today, 1 - CHUNK_DAYS);
  const [peaks, existing, boards] = await Promise.all([
    db.prepare(`SELECT strftime('%Y-%m-%d', checked_at / 1000, 'unixepoch') AS day, MAX(riders) AS riders, MAX(rooms) AS rooms
      FROM game_server_checks WHERE ok = 1 AND checked_at >= ? GROUP BY day`).bind(Date.parse(`${peakFrom}T00:00:00Z`)).all(),
    db.prepare('SELECT day, data FROM stats_days WHERE day >= ?').bind([from, peakFrom].filter(Boolean).sort()[0]).all(),
    boardCounts(db, today).catch(() => null),
  ]);
  const rows = new Map(existing.results.map(row => [row.day, JSON.parse(row.data)]));
  const touched = new Set([today]);
  for (const [day, fields] of Object.entries(usage ?? {})) {
    const { peakRiders, peakRooms, boards: snapshot } = rows.get(day) ?? {};
    rows.set(day, { ...fields, ...(peakRiders !== undefined ? { peakRiders, peakRooms } : {}), ...(snapshot ? { boards: snapshot } : {}) });
    touched.add(day);
  }
  for (const peak of peaks.results) {
    const row = rows.get(peak.day) ?? {};
    row.peakRiders = Math.max(row.peakRiders ?? 0, whole(peak.riders));
    row.peakRooms = Math.max(row.peakRooms ?? 0, whole(peak.rooms));
    rows.set(peak.day, row);
    touched.add(peak.day);
  }
  const current = rows.get(today) ?? {};
  if (boards) current.boards = { times: whole(boards.times), riders: whole(boards.riders), boards: whole(boards.boards), daily: whole(boards.daily), today: whole(boards.today) };
  rows.set(today, current);
  const upsert = 'INSERT INTO stats_days (day, data, updated_at) VALUES (?, ?, ?) ON CONFLICT (day) DO UPDATE SET data = excluded.data, updated_at = excluded.updated_at';
  await db.batch([...touched].sort().map(day => db.prepare(upsert).bind(day, JSON.stringify(rows.get(day)), at)));
  return { days: touched.size, usage: usage ? { from, to } : null };
}

// --- Public summary (GET /api/stats) -----------------------------------------

async function liveCounts(env, latest, fetcher, at) {
  const base = (env.GAME_SERVER_URL ?? '').trim().replace(/\/+$/, '');
  if (base) {
    try {
      const response = await fetcher(`${base}/healthz`, { headers: { Accept: 'application/json' }, redirect: 'manual', signal: AbortSignal.timeout(LIVE_TIMEOUT_MS) });
      const body = response.status === 200 ? JSON.parse((await response.text()).slice(0, 4096)) : null;
      if (body?.ok === true && Number.isSafeInteger(body.riders) && Number.isSafeInteger(body.rooms)) return { riders: Math.max(0, body.riders), rooms: Math.max(0, body.rooms) };
    } catch { /* Fall back to the last health check. */ }
  }
  if (latest?.ok === 1 && at - latest.checked_at < LIVE_STALE_MS && latest.riders !== null) return { riders: whole(latest.riders), rooms: whole(latest.rooms) };
  return null;
}

const hours = seconds => Math.round(seconds / 360) / 10;
const split = (entries, ids) => {
  const list = ids.map(id => ({ id, rides: entries[id]?.[0] ?? 0, hours: hours(entries[id]?.[1] ?? 0) }));
  return list.reduce((sum, item) => sum + item.rides, 0) >= MIN_BUCKET ? list : null;
};

export function summarize(rows, { live = null, at = Date.now() } = {}) {
  const today = dayOf(at), weekStart = shift(today, -6), chartStart = shift(today, 1 - CHART_DAYS);
  const days = rows.map(row => ({ day: row.day, ...JSON.parse(row.data) })).sort((a, b) => a.day.localeCompare(b.day));
  const used = days.filter(d => d.usage);
  const peakDay = days.reduce((best, d) => ((d.peakRiders ?? 0) > (best?.peakRiders ?? 0) ? d : best), null);
  const todayRow = days.find(d => d.day === today);
  const boards = [...days].reverse().find(d => d.boards)?.boards ?? null;
  const pick = dailyPick(today);
  const body = {
    updated: new Date(at).toISOString(),
    live,
    peak: peakDay ? { riders: peakDay.peakRiders, day: peakDay.day, today: todayRow?.peakRiders ?? 0 } : null,
    daily: { course: courseName(pick.course), vehicle: pick.vehicleClass, entries: todayRow?.boards ? todayRow.boards.today : null },
    boards: boards && boards.times + boards.daily > 0 ? { times: boards.times, riders: boards.riders, boards: boards.boards, dailyEntries: boards.daily } : null,
  };
  const ridden = used.filter(d => d.rides || d.seconds || d.sessions);
  if (!ridden.length) return body;
  const total = list => {
    const sum = { rides: 0, seconds: 0, sessions: 0, finishes: 0 };
    for (const d of list) for (const key of Object.keys(sum)) sum[key] += number(d[key]);
    return { rides: sum.rides, hours: hours(sum.seconds), sessions: sum.sessions, racesFinished: sum.finishes };
  };
  const merged = { modes: {}, activities: {}, vehicles: {}, maps: {}, devices: {}, countries: {} };
  for (const d of used) {
    for (const field of ['modes', 'activities', 'vehicles', 'maps']) for (const [key, values] of Object.entries(d[field] ?? {})) add(merged[field], key, values);
    for (const field of ['devices', 'countries']) for (const [key, n] of Object.entries(d[field] ?? {})) merged[field][key] = (merged[field][key] ?? 0) + number(n);
  }
  const venues = Object.entries(merged.maps)
    .filter(([id, [rides]]) => VENUES[id] && rides >= MIN_BUCKET)
    .map(([id, [rides, seconds, finishes]]) => ({ id, name: VENUES[id], rides, hours: hours(seconds), racesFinished: finishes }))
    .sort((a, b) => b.hours - a.hours || b.rides - a.rides).slice(0, 12);
  const countries = Object.entries(merged.countries).filter(([, n]) => n > 0);
  const devices = DEVICES.map(id => ({ id, sessions: merged.devices[id] ?? 0 }));
  Object.assign(body, {
    since: ridden[0].day,
    totals: { today: total(used.filter(d => d.day === today)), week: total(used.filter(d => d.day >= weekStart)), allTime: total(used) },
    modes: split(merged.modes, SPLITS.modes),
    activities: split(merged.activities, SPLITS.activities),
    vehicles: split(merged.vehicles, SPLITS.vehicles),
    devices: devices.reduce((sum, d) => sum + d.sessions, 0) >= MIN_BUCKET ? devices : null,
    venues,
    countries: {
      count: countries.length,
      top: countries.filter(([, n]) => n >= MIN_BUCKET).sort((a, b) => b[1] - a[1]).slice(0, 12).map(([code, sessions]) => ({ code, sessions })),
    },
    days: days.filter(d => d.day >= chartStart && d.usage).map(d => ({ day: d.day, rides: whole(d.rides), hours: hours(number(d.seconds)) })),
  });
  return body;
}

export async function statsRequest(request, env, { fetcher = fetch, now = Date.now } = {}) {
  if (request.method !== 'GET' && request.method !== 'HEAD') return new Response('Method not allowed', { status: 405, headers: { Allow: 'GET, HEAD' } });
  const cache = globalThis.caches?.default;
  const key = new Request(new URL(STATS_PATH, request.url));
  const cached = cache && await cache.match(key);
  if (cached) return request.method === 'HEAD' ? new Response(null, cached) : cached;
  const at = now();
  let rows = [], latest = null;
  if (env.SAVES) {
    try {
      const [days, check] = await env.SAVES.batch([
        env.SAVES.prepare('SELECT day, data FROM stats_days ORDER BY day'),
        env.SAVES.prepare('SELECT checked_at, ok, riders, rooms FROM game_server_checks ORDER BY checked_at DESC LIMIT 1'),
      ]);
      rows = days.results; latest = check.results[0] ?? null;
    } catch (failure) { console.error('stats read failed', String(failure?.message ?? failure).slice(0, 200)); }
  }
  const body = summarize(rows, { live: await liveCounts(env, latest, fetcher, at), at });
  const response = new Response(JSON.stringify(body), {
    headers: { 'Content-Type': 'application/json', 'Cache-Control': `public, max-age=${MAX_AGE}`, 'X-Content-Type-Options': 'nosniff' },
  });
  if (cache) await cache.put(key, response.clone());
  return request.method === 'HEAD' ? new Response(null, response) : response;
}
