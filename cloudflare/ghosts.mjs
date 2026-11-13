// World board ghosts (research/WORLD_BOARDS.md "Ghosts"): the recorded lap of
// a lap board's top rows, so another rider can race it in Lap Attack. A ghost
// is uploaded after its lap placed (`POST /api/ghosts/<board>`), kept only
// while its row is one of the board's ten best with exactly its time, and
// downloaded by board, name and time (`GET /api/ghosts/<board>/<name>/<ms>`,
// cached at the edge). It is checked for what is cheap to check: size, format
// version, a length that fits the lap time, the row's course and vehicle and
// motion a vehicle could ride. Like a board row it is not proof.
import { browserOriginAllowed } from './security.mjs';
import { MAX_STEP_FEET, MIN_LAP_FEET, cleanName, parseBoard } from './boards.mjs';

// Rows of a lap board that keep a ghost.
export const GHOST_TOP = 10;
// The format (atv_core::world_ghost): a 32-byte header and 16 bytes a key.
export const GHOST_VERSION = 1;
export const GHOST_HEADER = 32;
export const GHOST_KEY = 16;
// Simulation steps a second, and steps between keys (10 keys a second).
export const STEP_HZ = 60;
export const GHOST_STRIDE = 6;
// 4094 keys: a lap of 6 min 49 s. Longer laps carry no ghost.
export const MAX_GHOST_BYTES = 64 * 1024;
export const MAX_GHOST_KEYS = (MAX_GHOST_BYTES - GHOST_HEADER) / GHOST_KEY;
// The ghost travels and is stored as base64 text.
export const MAX_GHOST_TEXT = Math.ceil(MAX_GHOST_BYTES / 3) * 4;
export const MAX_UPLOAD_BYTES = MAX_GHOST_TEXT + 1024;
// Edge and client cache for a download, seconds: the address names the lap
// time, so its content never changes.
export const GHOST_MAX_AGE = 3600;
const RETRY_AFTER = 60;
// The lap's steps against its time: the lap clock is read at the line, the
// steps are whole.
const LAP_SLACK_MS = 250;
// Position units a foot.
const UNITS = 32;
// Keys a rider reset may jump (the boards' trace allows as many).
const MAX_JUMPS = 30;
const MAX_FEET = 1e6;

function json(body, status = 200, headers = {}) {
  return new Response(JSON.stringify(body), {
    status, headers: { 'Content-Type': 'application/json; charset=utf-8', 'Cache-Control': 'no-store', ...headers },
  });
}
const error = (code, status, extra = {}, headers = {}) => json({ error: code, ...extra }, status, headers);
const unavailable = () => error('unavailable', 503);
const rateLimited = () => error('rate_limited', 429, { retryAfter: RETRY_AFTER }, { 'Retry-After': String(RETRY_AFTER) });

async function limited(env, binding, ip) {
  const limiter = env[binding];
  if (!limiter) return null;
  return !(await limiter.limit({ key: `ip:${ip}` })).success;
}

async function readBody(request) {
  const length = request.headers.get('Content-Length');
  if (length !== null && (!/^\d+$/.test(length) || Number(length) > MAX_UPLOAD_BYTES)) return null;
  if (!request.body) return '';
  const reader = request.body.getReader(), chunks = []; let size = 0;
  try {
    while (true) {
      const { done, value } = await reader.read(); if (done) break;
      size += value.byteLength;
      if (size > MAX_UPLOAD_BYTES) { await reader.cancel(); return null; }
      chunks.push(value);
    }
  } finally { reader.releaseLock(); }
  const bytes = new Uint8Array(size); let offset = 0;
  for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.length; }
  return new TextDecoder('utf-8', { fatal: true }).decode(bytes);
}

// Canonical base64 text to bytes, or null.
export function decodeGhost(text) {
  if (typeof text !== 'string' || text.length > MAX_GHOST_TEXT || text.length % 4 !== 0 || !/^[A-Za-z0-9+/]*={0,2}$/.test(text)) return null;
  let binary;
  try { binary = atob(text); } catch { return null; }
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

// The header of a ghost's bytes, or null when they are not a version-1 ghost
// of a consistent size.
export function ghostHeader(bytes) {
  if (!(bytes instanceof Uint8Array) || bytes.length < GHOST_HEADER + 2 * GHOST_KEY || bytes.length > MAX_GHOST_BYTES) return null;
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (String.fromCharCode(...bytes.subarray(0, 4)) !== 'FRGH' || bytes[4] !== GHOST_VERSION) return null;
  const stride = bytes[5], vehicle = view.getUint16(6, true), lapMs = view.getUint32(8, true);
  const steps = view.getUint32(12, true), keys = view.getUint16(16, true), length = bytes[18];
  if (length < 3 || length > 10 || bytes[29] || bytes[30] || bytes[31]) return null;
  for (let i = 19 + length; i < 29; i++) if (bytes[i]) return null;
  const course = String.fromCharCode(...bytes.subarray(19, 19 + length));
  if (!/^[a-z0-9]+$/.test(course) || bytes.length !== GHOST_HEADER + keys * GHOST_KEY) return null;
  return { stride, vehicle, lapMs, steps, keys, course };
}

// Checks a ghost against the row it rides for: {course, vehicle, lapMs}.
// Returns an error reason or null.
export function checkGhost(bytes, row) {
  const header = ghostHeader(bytes);
  if (!header) return 'ghost_format';
  if (header.course !== row.course || header.vehicle !== row.vehicle || header.lapMs !== row.lapMs) return 'ghost_row';
  // One key every sixth step from the lap's first, and steps that last the
  // lap time.
  if (header.stride !== GHOST_STRIDE || header.steps < 1 || header.keys !== Math.floor((header.steps - 1) / GHOST_STRIDE) + 1) return 'ghost_length';
  if (Math.abs((header.steps * 1000) / STEP_HZ - header.lapMs) > LAP_SLACK_MS) return 'ghost_length';
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  // A key's step at twice the boards' fastest one-second step, in feet.
  const limit = (2 * MAX_STEP_FEET * GHOST_STRIDE) / STEP_HZ;
  let feet = 0, jumps = 0, last = null;
  for (let key = 0; key < header.keys; key++) {
    const at = GHOST_HEADER + key * GHOST_KEY;
    const point = [view.getInt32(at, true) / UNITS, view.getInt32(at + 4, true) / UNITS, view.getInt32(at + 8, true) / UNITS];
    if (point.some(value => Math.abs(value) > MAX_FEET)) return 'ghost_bounds';
    if (last) {
      const step = Math.hypot(point[0] - last[0], point[2] - last[2]);
      if (step > limit) jumps++; else feet += step;
    }
    last = point;
  }
  if (jumps > MAX_JUMPS) return 'ghost_speed';
  if (feet < MIN_LAP_FEET) return 'ghost_short';
  return null;
}

// The board's rows that keep a ghost: its ten best riders.
const KEPT = `SELECT name, value, vehicle FROM board_entries WHERE board = ?1 ORDER BY value, name LIMIT ${GHOST_TOP}`;

// Drops a board's ghosts whose row left the ten best or changed its time
// (the rider improved; the new lap's ghost follows in its own upload).
const prune = (db, board) => db.prepare(
  `DELETE FROM board_ghosts WHERE board = ?1 AND NOT EXISTS (SELECT 1 FROM (${KEPT}) kept WHERE kept.name = board_ghosts.name AND kept.value = board_ghosts.value)`,
).bind(board);

// After a submission stored rows (boards.mjs): the lap boards it changed drop
// the ghosts that no longer belong to a kept row.
export async function pruneGhosts(db, ranks) {
  const boards = Object.entries(ranks).filter(([board, result]) => result.placed && board.startsWith('lap:')).map(([board]) => board);
  if (boards.length) await db.batch(boards.map(board => prune(db, board)));
}

// Marks a lap board's entries that have a ghost to race (boards.mjs's read).
// House riders have none.
export async function markGhosts(db, board, entries) {
  if (!board.startsWith('lap:')) return;
  const { results } = await db.prepare('SELECT name, value FROM board_ghosts WHERE board = ?1').bind(board).all();
  const ghosts = new Map((results ?? []).map(row => [row.name, row.value]));
  for (const entry of entries) if (!entry.house && ghosts.get(entry.name) === entry.value) entry.ghost = true;
}

const boardKey = (request, board) => new Request(new URL(`/api/boards/${board}`, request.url));

async function upload(request, db, board, course) {
  const text = await readBody(request).catch(() => undefined);
  if (text === null) return error('too_large', 413);
  let body;
  try { body = JSON.parse(text); } catch { /* Invalid JSON. */ }
  if (!body || typeof body !== 'object' || Array.isArray(body)) return error('bad_body', 400);
  const name = cleanName(body.name);
  if (!name) return error('implausible', 422, { reason: 'name' });
  const bytes = decodeGhost(body.ghost);
  const header = bytes && ghostHeader(bytes);
  if (!header) return error('implausible', 422, { reason: 'ghost_format' });
  const reason = checkGhost(bytes, { course, vehicle: header.vehicle, lapMs: header.lapMs });
  if (reason) return error('implausible', 422, { reason });
  // Stored only for the kept row with this name, time and vehicle; a ghost
  // already stored for that time is not replaced.
  const results = await db.batch([
    db.prepare(`INSERT INTO board_ghosts (board, name, value, data)
      SELECT ?1, ?2, ?3, ?4 WHERE EXISTS (SELECT 1 FROM (${KEPT}) kept WHERE kept.name = ?2 AND kept.value = ?3 AND kept.vehicle = ?5)
      ON CONFLICT (board, name) DO UPDATE SET value = excluded.value, data = excluded.data WHERE excluded.value < board_ghosts.value`)
      .bind(board, name, header.lapMs, body.ghost, header.vehicle),
    prune(db, board),
  ]);
  const kept = (results[0].meta?.changes ?? 0) > 0;
  // This location's cached copy of the board now lacks the row's ghost mark.
  if (kept) await globalThis.caches?.default?.delete(boardKey(request, board));
  return json({ board, name, kept });
}

async function download(request, db, board, name, value) {
  const cache = globalThis.caches?.default, key = new Request(new URL(new URL(request.url).pathname, request.url));
  const cached = await cache?.match(key);
  if (cached) return cached;
  const row = await db.prepare(
    `SELECT g.data AS data, e.vehicle AS vehicle FROM board_ghosts g JOIN board_entries e ON e.board = g.board AND e.name = g.name AND e.value = g.value
      WHERE g.board = ?1 AND g.name = ?2 AND g.value = ?3`).bind(board, name, value).first();
  if (!row) return error('not_found', 404);
  const response = json({ board, name, value, vehicle: row.vehicle, ghost: row.data }, 200, { 'Cache-Control': `public, max-age=${GHOST_MAX_AGE}` });
  await cache?.put(key, response.clone());
  return response;
}

export const isGhostPath = path => path === '/api/ghosts' || path.startsWith('/api/ghosts/');

export async function ghostRequest(request, env, allowedOrigins) {
  if (!browserOriginAllowed(request, allowedOrigins)) return error('origin_denied', 403);
  let parts;
  try { parts = new URL(request.url).pathname.slice('/api/ghosts/'.length).split('/').map(decodeURIComponent); } catch { return error('not_found', 404); }
  const board = parts[0], parsed = parseBoard(board);
  if (!parsed || parsed.kind !== 'lap' || (parts.length !== 1 && parts.length !== 3)) return error('not_found', 404);
  const submit = parts.length === 1;
  const name = submit ? null : cleanName(parts[1]);
  const value = submit ? null : /^[1-9]\d{0,7}$/.test(parts[2]) ? Number(parts[2]) : null;
  if (!submit && (!name || name !== parts[1] || value === null)) return error('not_found', 404);
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
    return submit ? await upload(request, db, board, parsed.course) : await download(request, db, board, name, value);
  } catch {
    // Storage or limiter failure. Never echo request data into logs.
    return unavailable();
  }
}
