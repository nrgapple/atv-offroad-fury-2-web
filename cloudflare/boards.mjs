// World Hall of Fame boards and the daily event (research/WORLD_BOARDS.md).
// A board keeps each rider name's best; reads are cached at the edge, a
// finished event sends one bounded submission, and the server checks it is
// plausible (floor times, the run's 1 Hz position trace, score rates). No IP
// addresses or device identifiers are stored; IPs key only the edge limiters.
import { browserOriginAllowed } from './security.mjs';
import { markGhosts, pruneGhosts } from './ghosts.mjs';

export const TOP = 50;
export const MAX_SUBMIT_BYTES = 24 * 1024;
// Edge and client cache for a board read, seconds.
export const BOARD_MAX_AGE = 60;
const RETRY_AFTER = 60;
export const NAME = /^[A-Z0-9 ._!-]{1,8}$/;
export const VEHICLES = [1, 46];
// Vehicles 27 and above are the motorcycles (atv_core::player_models).
export const FIRST_MOTORCYCLE = 27;

// Race course ids (Tracks.dat): Nationals env/nat, Enduro short/long, Short
// Tracks and Supercross.
const RACE_COURSE = /^(?:env0[1-9](?:ends|endl|st)?|nat0[1-9]|sx(?:0[1-9]|1[0-9]))$/;
export const FREESTYLE = {
  unspstunt: ['env05', 'env02', 'env08', 'env06', 'env04', 'env01'],
  spstunt: ['free01', 'free04', 'free02', 'free06', 'free03'],
};

// The fastest plausible lap per event family, milliseconds. Conservative
// assumptions (no measured world-class laps yet): a lap below the floor is
// refused, whatever its trace says.
export function lapFloorMs(course) {
  if (/end[sl]$/.test(course)) return 45_000;
  if (/st$/.test(course)) return 12_000;
  if (course.startsWith('sx')) return 15_000;
  return 25_000;
}
export const defaultLaps = course => (/end[sl]$/.test(course) ? 3 : 5);

// Trace checks, world units (feet) per one-second sample.
export const MAX_STEP_FEET = 200;
export const MAX_LAP_SPEED = 150;
export const MIN_LAP_FEET = 500;
export const MAX_LAP_GAP_FEET = 400;
const MAX_POINTS = 1800;
const MAX_RESETS = 30;
const MARK_SLACK_MS = 50;

// Freestyle objective runs last 120 s (research/PROFILE.md), and a target
// run gains 10 s a green target: at most 30 hits before the last ends it
// (St. Croix Sound's 31, atv_core::freestyle::course_objectives).
export const MAX_FREESTYLE_SECONDS = 425;
export const MAX_SCORE = 1_000_000;
export const MAX_SCORE_RATE = 15_000;

// The daily event (atv_core::world_board::daily): the championship race
// courses in event-type order. Keep in step with DAILY_POOL there.
export const DAILY_POOL = [
  'env05ends', 'env02ends', 'env08ends', 'env06ends', 'env04ends', 'env01ends',
  'env05endl', 'env02endl', 'env08endl', 'env06endl', 'env04endl', 'env01endl',
  'env05', 'env02', 'env08', 'env06', 'env04', 'env01',
  'nat01', 'nat02', 'nat03', 'nat04', 'nat05', 'nat06', 'nat07',
  'sx08', 'sx12', 'sx01', 'sx13', 'sx02', 'sx06', 'sx05',
  'sx03', 'sx07', 'sx16', 'sx04', 'sx10', 'sx15', 'sx14',
];
const DAY_MS = 86_400_000;
// Submissions for yesterday's event are still taken this long after midnight.
const DAILY_GRACE_MS = 30 * 60_000;

function mix(day) {
  let x = Math.imul(day >>> 0, 0x9e3779b1) >>> 0;
  x = Math.imul(x ^ (x >>> 16), 0x45d9f3b) >>> 0;
  return (x ^ (x >>> 16)) >>> 0;
}

// The day's pick: course, and 'atv' or 'motorcycle' (one day in four).
export function dailyPick(date) {
  const day = Math.floor(Date.parse(`${date}T00:00:00Z`) / DAY_MS);
  const hash = mix(day);
  return { date, course: DAILY_POOL[hash % DAILY_POOL.length], vehicleClass: (hash >>> 8) % 4 === 3 ? 'motorcycle' : 'atv' };
}
export const dayOf = ms => new Date(ms).toISOString().slice(0, 10);
const vehicleInClass = (vehicle, kind) => (kind === 'motorcycle' ? vehicle >= FIRST_MOTORCYCLE : vehicle < FIRST_MOTORCYCLE);

const BOARD = /^(?:(lap|race):([a-z0-9]{3,10}):([nx])|score:(unspstunt|spstunt):([a-z0-9]{3,10})|daily:(\d{4}-\d{2}-\d{2}))$/;
export function parseBoard(id) {
  const m = BOARD.exec(id ?? '');
  if (!m) return null;
  if (m[1]) return RACE_COURSE.test(m[2]) ? { kind: m[1], course: m[2], expert: m[3] === 'x' } : null;
  if (m[4]) return FREESTYLE[m[4]].includes(m[5]) ? { kind: 'score', series: m[4], course: m[5] } : null;
  return Number.isFinite(Date.parse(`${m[6]}T00:00:00Z`)) ? { kind: 'daily', date: m[6] } : null;
}

function json(body, status = 200, headers = {}) {
  return new Response(JSON.stringify(body), {
    status, headers: { 'Content-Type': 'application/json; charset=utf-8', 'Cache-Control': 'no-store', ...headers },
  });
}
const error = (code, status, extra = {}, headers = {}) => json({ error: code, ...extra }, status, headers);
const unavailable = () => error('unavailable', 503);
const rateLimited = () => error('rate_limited', 429, { retryAfter: RETRY_AFTER }, { 'Retry-After': String(RETRY_AFTER) });
const refused = reason => error('implausible', 422, { reason });

async function limited(env, binding, ip) {
  const limiter = env[binding];
  if (!limiter) return null;
  return !(await limiter.limit({ key: `ip:${ip}` })).success;
}

async function readBody(request) {
  const length = request.headers.get('Content-Length');
  if (length !== null && (!/^\d+$/.test(length) || Number(length) > MAX_SUBMIT_BYTES)) return null;
  if (!request.body) return '';
  const reader = request.body.getReader(), chunks = []; let size = 0;
  try {
    while (true) {
      const { done, value } = await reader.read(); if (done) break;
      size += value.byteLength;
      if (size > MAX_SUBMIT_BYTES) { await reader.cancel(); return null; }
      chunks.push(value);
    }
  } finally { reader.releaseLock(); }
  const bytes = new Uint8Array(size); let offset = 0;
  for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.length; }
  return new TextDecoder('utf-8', { fatal: true }).decode(bytes);
}

const isInt = (value, min, max) => Number.isSafeInteger(value) && value >= min && value <= max;
const distance = (a, b) => Math.hypot(a[0] - b[0], a[1] - b[1]);

// The run's trace: `points` one [x, z] (integer feet) per second from GO,
// `marks` each completed lap's clock time in ms and `resets` the points a
// rider reset moved the vehicle to (the step into them is not ridden).
// Returns an error reason or null.
export function checkTrace(trace, seconds) {
  if (!trace || typeof trace !== 'object') return 'trace';
  const { points, marks = [], resets = [] } = trace;
  if (!Array.isArray(points) || points.length < 2 || points.length > MAX_POINTS) return 'trace';
  if (!Array.isArray(marks) || marks.length > 5) return 'trace';
  if (!Array.isArray(resets) || resets.length > MAX_RESETS || !resets.every(i => isInt(i, 1, points.length - 1))) return 'trace';
  for (const p of points) if (!Array.isArray(p) || p.length !== 2 || !isInt(p[0], -1e6, 1e6) || !isInt(p[1], -1e6, 1e6)) return 'trace';
  let last = 0;
  for (const mark of marks) { if (!isInt(mark, last + 1, 3_600_000)) return 'trace'; last = mark; }
  // One point a second from GO up to the end of the run, give or take one.
  if (Math.abs(points.length - (Math.floor(seconds) + 1)) > 2) return 'trace_length';
  const reset = new Set(resets);
  for (let i = 1; i < points.length; i++) if (!reset.has(i) && distance(points[i - 1], points[i]) > MAX_STEP_FEET) return 'trace_speed';
  return null;
}

// The ridden ground between two clock times: the points' path, skipping
// the steps into reset points.
function ridden(trace, fromMs, toMs) {
  const reset = new Set(trace.resets ?? []);
  const first = Math.max(0, Math.ceil(fromMs / 1000)), last = Math.min(trace.points.length - 1, Math.floor(toMs / 1000));
  let feet = 0;
  for (let i = first + 1; i <= last; i++) if (!reset.has(i)) feet += distance(trace.points[i - 1], trace.points[i]);
  return { feet, start: trace.points[first], end: trace.points[last] };
}

// A lap of `lapMs`: it must be one of the trace's laps (consecutive marks),
// cover a lap's ground at a possible speed and end near where it started.
export function checkLap(trace, lapMs, course) {
  if (!isInt(lapMs, lapFloorMs(course), 1_800_000)) return 'lap_floor';
  const marks = trace.marks ?? [];
  for (let i = 0; i < marks.length; i++) {
    const from = i ? marks[i - 1] : 0;
    if (Math.abs(marks[i] - from - lapMs) > MARK_SLACK_MS) continue;
    const { feet, start, end } = ridden(trace, from, marks[i]);
    if (feet < MIN_LAP_FEET) return 'lap_short';
    if (feet / (lapMs / 1000) > MAX_LAP_SPEED) return 'lap_speed';
    // The first lap starts on the grid, not the line.
    if (i && distance(start, end) > MAX_LAP_GAP_FEET) return 'lap_open';
    return null;
  }
  return 'lap_marks';
}

// A full-length race of `raceMs`. The first lap (from the grid) may be
// shorter than a lap; every later one must clear the floor.
export function checkRace(trace, raceMs, course) {
  const laps = defaultLaps(course), floor = lapFloorMs(course);
  if (!isInt(raceMs, (laps - 1) * floor, 3_600_000)) return 'race_floor';
  const marks = trace.marks ?? [];
  if (marks.length !== laps || Math.abs(marks[laps - 1] - raceMs) > MARK_SLACK_MS) return 'race_marks';
  for (let i = 1; i < laps; i++) if (marks[i] - marks[i - 1] < floor) return 'lap_floor';
  if (ridden(trace, 0, raceMs).feet / (raceMs / 1000) > MAX_LAP_SPEED) return 'race_speed';
  return null;
}

export function cleanName(value) {
  if (typeof value !== 'string') return null;
  const name = value.trim().toUpperCase();
  return NAME.test(name) ? name : null;
}

// Validates a submission; returns {entries: [[board, value]], vehicle, name}
// or {reason}.
export function judge(body, nowMs) {
  const name = cleanName(body.name);
  if (!name) return { reason: 'name' };
  const vehicle = body.vehicle;
  if (!isInt(vehicle, VEHICLES[0], VEHICLES[1])) return { reason: 'vehicle' };
  const entries = [];
  if (body.series !== undefined) {
    const board = `score:${body.series}:${body.course}`;
    if (!parseBoard(board)) return { reason: 'board' };
    const { score, seconds } = body;
    if (!isInt(seconds, 1, MAX_FREESTYLE_SECONDS)) return { reason: 'seconds' };
    if (!isInt(score, 1, MAX_SCORE) || score > seconds * MAX_SCORE_RATE) return { reason: 'score' };
    const bad = checkTrace(body.trace, seconds);
    if (bad) return { reason: bad };
    entries.push([board, -score]);
    return { entries, vehicle, name };
  }
  const { course, lapMs, raceMs, trace } = body;
  const expert = body.expert === true;
  if (typeof course !== 'string' || !parseBoard(`lap:${course}:n`)) return { reason: 'board' };
  if (lapMs === undefined && raceMs === undefined) return { reason: 'empty' };
  const end = trace?.marks?.length ? trace.marks[trace.marks.length - 1] : 0;
  const bad = checkTrace(trace, end / 1000);
  if (bad) return { reason: bad };
  const tag = expert ? 'x' : 'n';
  if (lapMs !== undefined) {
    const reason = checkLap(trace, lapMs, course);
    if (reason) return { reason };
    entries.push([`lap:${course}:${tag}`, lapMs]);
  }
  if (raceMs !== undefined) {
    const reason = checkRace(trace, raceMs, course);
    if (reason) return { reason };
    entries.push([`race:${course}:${tag}`, raceMs]);
  }
  if (body.daily !== undefined) {
    const date = body.daily;
    const today = dayOf(nowMs), late = dayOf(nowMs - DAILY_GRACE_MS);
    if (date !== today && date !== late) return { reason: 'daily_date' };
    const pick = dailyPick(date);
    if (raceMs === undefined || expert || pick.course !== course || !vehicleInClass(vehicle, pick.vehicleClass)) return { reason: 'daily_pick' };
    entries.push([`daily:${date}`, raceMs]);
  }
  return { entries, vehicle, name };
}

// Keeps the name's best on each board while it ranks in the top 50, then
// drops whatever fell below 50th. Returns each board's new rank (or null).
async function store(db, verdict, day) {
  const statements = [];
  for (const [board, value] of verdict.entries) {
    statements.push(
      db.prepare(`INSERT INTO board_entries (board, name, value, vehicle, day)
        SELECT ?1, ?2, ?3, ?4, ?5 WHERE (SELECT COUNT(*) FROM board_entries WHERE board = ?1 AND value <= ?3 AND name <> ?2) < ?6
        ON CONFLICT (board, name) DO UPDATE SET value = excluded.value, vehicle = excluded.vehicle, day = excluded.day WHERE excluded.value < board_entries.value`)
        .bind(board, verdict.name, value, verdict.vehicle, day, TOP),
      db.prepare(`DELETE FROM board_entries WHERE board = ?1 AND value > (SELECT value FROM board_entries WHERE board = ?1 ORDER BY value, name LIMIT 1 OFFSET ?2)`)
        .bind(board, TOP - 1),
      db.prepare(`SELECT 1 + (SELECT COUNT(*) FROM board_entries o WHERE o.board = e.board AND o.value < e.value) AS rank, e.value = ?3 AS mine
        FROM board_entries e WHERE e.board = ?1 AND e.name = ?2`).bind(board, verdict.name, value),
    );
  }
  const results = await db.batch(statements);
  const ranks = {};
  verdict.entries.forEach(([board], i) => {
    const changed = (results[i * 3].meta?.changes ?? 0) > 0;
    const row = results[i * 3 + 2].results?.[0];
    ranks[board] = { rank: row?.rank ?? null, placed: changed && Boolean(row?.mine) };
  });
  return ranks;
}

const boardKey = (request, board) => new Request(new URL(`/api/boards/${board}`, request.url));

async function readBoard(request, env, db, board) {
  const cache = globalThis.caches?.default, key = boardKey(request, board);
  const cached = await cache?.match(key);
  if (cached) return cached;
  const { results } = await db.prepare('SELECT name, value, vehicle, day FROM board_entries WHERE board = ?1 ORDER BY value, name LIMIT ?2').bind(board, TOP).all();
  const score = board.startsWith('score:');
  const entries = (results ?? []).map((row, i) => ({ rank: i + 1, name: row.name, value: score ? -row.value : row.value, vehicle: row.vehicle, day: row.day }));
  const body = { board, entries };
  const parsed = parseBoard(board);
  await markGhosts(db, board, entries);
  if (parsed.kind === 'daily') {
    body.pick = dailyPick(parsed.date);
    const yesterday = dayOf(Date.parse(`${parsed.date}T00:00:00Z`) - DAY_MS);
    const winner = await db.prepare('SELECT name, value, vehicle FROM board_entries WHERE board = ?1 ORDER BY value, name LIMIT 1').bind(`daily:${yesterday}`).first();
    body.yesterday = winner ? { date: yesterday, name: winner.name, value: winner.value, vehicle: winner.vehicle } : { date: yesterday };
  }
  const response = json(body, 200, { 'Cache-Control': `public, max-age=${BOARD_MAX_AGE}` });
  await cache?.put(key, response.clone());
  return response;
}

export const isBoardPath = path => path === '/api/boards' || path.startsWith('/api/boards/');

export async function boardRequest(request, env, allowedOrigins, nowMs = Date.now()) {
  const path = new URL(request.url).pathname;
  if (!browserOriginAllowed(request, allowedOrigins)) return error('origin_denied', 403);
  const submit = path === '/api/boards/submit';
  const board = submit ? null : decodeURIComponent(path.slice('/api/boards/'.length));
  if (!submit && !parseBoard(board)) return error('not_found', 404);
  const method = submit ? 'POST' : 'GET';
  if (request.method !== method) return error('method_not_allowed', 405, {}, { Allow: method });
  const db = env.SAVES;
  // Cloudflare supplies this IP header; clients cannot choose another identity.
  const ip = request.headers.get('CF-Connecting-IP');
  if (!db || !ip) return unavailable();
  try {
    const stop = await limited(env, submit ? 'BOARD_SUBMIT_LIMIT' : 'BOARD_READ_LIMIT', ip);
    if (stop === null) return unavailable();
    if (stop) return rateLimited();
    if (!submit) return await readBoard(request, env, db, board);
    const text = await readBody(request).catch(() => undefined);
    if (text === null) return error('too_large', 413);
    let body;
    try { body = JSON.parse(text); } catch { /* Invalid JSON. */ }
    if (!body || typeof body !== 'object' || Array.isArray(body)) return error('bad_body', 400);
    const verdict = judge(body, nowMs);
    if (verdict.reason) return refused(verdict.reason);
    const ranks = await store(db, verdict, dayOf(nowMs));
    await pruneGhosts(db, ranks);
    // This location's cached copies of the changed boards.
    const cache = globalThis.caches?.default;
    for (const [id, result] of Object.entries(ranks)) if (result.placed) await cache?.delete(boardKey(request, id));
    return json({ name: verdict.name, boards: ranks });
  } catch {
    // Storage or limiter failure. Never echo request data into logs.
    return unavailable();
  }
}
