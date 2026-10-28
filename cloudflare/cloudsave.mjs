// Pass Key cloud saves (research/CLOUD_SAVE.md). A Pass Key is a retro save
// code, not an account: whoever has it continues that save. The D1 database
// stores only SHA-256 hashes of keys and share codes, never the codes, and no
// IP addresses; IPs key only the edge rate limiters.
import { browserOriginAllowed } from './security.mjs';

export const MAX_SAVE_BYTES = 64 * 1024;
// The request wraps the save in {"baseRevision": n, "save": ...}.
const MAX_BODY_BYTES = MAX_SAVE_BYTES + 2048;
export const KEEP_REVISIONS = 10;
const RETRY_AFTER = 60;
const SYMBOLS = '0123456789ABCDEFGHJKMNPQRSTVWXYZ';
const MAP = { O: '0', I: '1', L: '1' };

// Uppercase, drop spaces and dashes and the optional prefix, map O->0 and
// I/L->1. Returns the 16 Crockford base32 symbols, or null.
export function normalizeCode(value, prefix) {
  if (typeof value !== 'string' || value.length > 64) return null;
  let compact = value.toUpperCase().replace(/[\s-]+/g, '');
  if (compact.startsWith(prefix) && compact.length === prefix.length + 16) compact = compact.slice(prefix.length);
  compact = compact.replace(/[OIL]/g, symbol => MAP[symbol]);
  return /^[0-9A-HJKMNP-TV-Z]{16}$/.test(compact) ? compact : null;
}

export function formatCode(prefix, symbols) {
  return `${prefix}-${symbols.match(/.{4}/g).join('-')}`;
}

// 80 random bits as 16 Crockford base32 symbols.
export function randomSymbols() {
  const bytes = crypto.getRandomValues(new Uint8Array(10));
  let bits = 0n;
  for (const byte of bytes) bits = (bits << 8n) | BigInt(byte);
  let out = '';
  for (let i = 15; i >= 0; i--) out += SYMBOLS[Number((bits >> BigInt(i * 5)) & 31n)];
  return out;
}

async function hashSymbols(symbols) {
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(symbols));
  return [...new Uint8Array(digest)].map(byte => byte.toString(16).padStart(2, '0')).join('');
}

function json(body, status = 200, headers = {}) {
  return new Response(JSON.stringify(body), {
    status, headers: { 'Content-Type': 'application/json; charset=utf-8', 'Cache-Control': 'no-store', ...headers },
  });
}
const error = (code, status, extra = {}, headers = {}) => json({ error: code, ...extra }, status, headers);
const unavailable = () => error('unavailable', 503);
const rateLimited = () => error('rate_limited', 429, { retryAfter: RETRY_AFTER }, { 'Retry-After': String(RETRY_AFTER) });

class TooLarge extends Error {}

async function readBody(request) {
  const length = request.headers.get('Content-Length');
  if (length !== null && (!/^\d+$/.test(length) || Number(length) > MAX_BODY_BYTES)) throw new TooLarge();
  if (!request.body) return '';
  const reader = request.body.getReader(), chunks = []; let size = 0;
  try {
    while (true) {
      const { done, value } = await reader.read(); if (done) break;
      size += value.byteLength;
      if (size > MAX_BODY_BYTES) { await reader.cancel(); throw new TooLarge(); }
      chunks.push(value);
    }
  } finally { reader.releaseLock(); }
  const bytes = new Uint8Array(size); let offset = 0;
  for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.length; }
  return new TextDecoder('utf-8', { fatal: true }).decode(bytes);
}

// Returns {body} or {response} for a JSON object body.
async function parseBody(request) {
  let text;
  try { text = await readBody(request); } catch (failure) { return { response: failure instanceof TooLarge ? error('too_large', 413) : error('bad_body', 400) }; }
  try {
    const body = JSON.parse(text);
    if (body && typeof body === 'object' && !Array.isArray(body)) return { body };
  } catch { /* Invalid JSON. */ }
  return { response: error('bad_body', 400) };
}

// The save is opaque apart from its integer schema version and size.
function serializeSave(save) {
  if (!save || typeof save !== 'object' || Array.isArray(save) || !Number.isSafeInteger(save.v) || save.v < 1) return { response: error('bad_body', 400) };
  const text = JSON.stringify(save), bytes = new TextEncoder().encode(text).byteLength;
  if (bytes > MAX_SAVE_BYTES) return { response: error('too_large', 413) };
  return { text, bytes };
}

const isRevision = value => Number.isSafeInteger(value) && value >= 0;
const now = () => new Date().toISOString();

// Native rate-limit bindings, keyed by client IP (and key hash for writes).
async function allowed(env, binding, keys) {
  const limiter = env[binding];
  if (!limiter) return null;
  for (const key of keys) if (!(await limiter.limit({ key })).success) return false;
  return true;
}

// Commit a write prepared by `change` and return the current row. D1 runs a
// batch as one transaction, so the revision copy and pruning always match the
// row the change left behind; a change that matched no row writes nothing.
async function commit(db, id, change) {
  const results = await db.batch([
    change,
    db.prepare('INSERT OR IGNORE INTO revisions (key_id, revision, updated_at, bytes, save) SELECT id, revision, updated_at, bytes, save FROM keys WHERE id = ?1').bind(id),
    db.prepare('DELETE FROM revisions WHERE key_id = ?1 AND revision <= (SELECT revision FROM keys WHERE id = ?1) - ?2').bind(id, KEEP_REVISIONS),
    db.prepare('SELECT revision, updated_at, save FROM keys WHERE id = ?1').bind(id),
  ]);
  return { changed: (results[0].meta?.changes ?? 0) > 0, row: results[3].results?.[0] ?? null };
}

const conflict = row => error('conflict', 409, row ? { revision: row.revision, updatedAt: row.updated_at, save: JSON.parse(row.save) } : { revision: 0, updatedAt: null, save: null });

async function putSave(request, env, db, id, ip) {
  const parsed = await parseBody(request);
  if (parsed.response) return parsed.response;
  const { baseRevision, save } = parsed.body;
  if (!isRevision(baseRevision)) return error('bad_body', 400);
  const serialized = serializeSave(save);
  if (serialized.response) return serialized.response;
  if (baseRevision === 0) {
    const ok = await allowed(env, 'SAVE_CREATE_LIMIT', [`ip:${ip}`]);
    if (ok === null) return unavailable();
    if (!ok) return rateLimited();
  }
  const at = now();
  const change = baseRevision === 0
    ? db.prepare('INSERT INTO keys (id, revision, created_at, updated_at, bytes, save) VALUES (?1, 1, ?2, ?2, ?3, ?4) ON CONFLICT (id) DO NOTHING').bind(id, at, serialized.bytes, serialized.text)
    : db.prepare('UPDATE keys SET revision = revision + 1, updated_at = ?2, bytes = ?3, save = ?4 WHERE id = ?1 AND revision = ?5').bind(id, at, serialized.bytes, serialized.text, baseRevision);
  const { changed, row } = await commit(db, id, change);
  if (!changed) return conflict(row);
  return json({ revision: row.revision, updatedAt: row.updated_at });
}

async function restore(request, db, id) {
  const parsed = await parseBody(request);
  if (parsed.response) return parsed.response;
  const { revision } = parsed.body;
  if (!isRevision(revision) || revision < 1) return error('bad_body', 400);
  const change = db.prepare(`UPDATE keys SET revision = keys.revision + 1, updated_at = ?3, bytes = old.bytes, save = old.save
    FROM (SELECT bytes, save FROM revisions WHERE key_id = ?1 AND revision = ?2) AS old WHERE keys.id = ?1`).bind(id, revision, now());
  const { changed, row } = await commit(db, id, change);
  if (!changed) return error('not_found', 404);
  return json({ revision: row.revision, updatedAt: row.updated_at, save: JSON.parse(row.save) });
}

async function share(db, id) {
  for (let attempt = 0; attempt < 3; attempt++) {
    const symbols = randomSymbols(), createdAt = now();
    try {
      const result = await db.prepare('INSERT INTO shares (id, created_at, bytes, save) SELECT ?1, ?2, bytes, save FROM keys WHERE id = ?3')
        .bind(await hashSymbols(symbols), createdAt, id).run();
      if (!result.meta?.changes) return error('not_found', 404);
      return json({ shareCode: formatCode('SHARE', symbols), createdAt }, 201);
    } catch (failure) {
      // An 80-bit collision is practically impossible; retry with a new code.
      if (!/UNIQUE/i.test(String(failure?.message))) throw failure;
    }
  }
  return unavailable();
}

const ROUTES = {
  '/api/save': { GET: 'read', PUT: 'write' },
  '/api/save/share': { POST: 'share' },
  '/api/save/revisions': { GET: 'read' },
  '/api/save/restore': { POST: 'write' },
  '/api/share': { GET: 'read' },
};

export function isCloudSavePath(path) {
  return path === '/api/share' || path === '/api/save' || path.startsWith('/api/save/');
}

export async function cloudSaveRequest(request, env, allowedOrigins) {
  const path = new URL(request.url).pathname;
  const methods = ROUTES[path];
  if (!methods) return error('not_found', 404);
  if (!browserOriginAllowed(request, allowedOrigins)) return error('origin_denied', 403);
  const kind = methods[request.method];
  if (!kind) return error('method_not_allowed', 405, {}, { Allow: Object.keys(methods).join(', ') });
  const shareLookup = path === '/api/share';
  const symbols = shareLookup ? normalizeCode(request.headers.get('X-Fury-Share-Code'), 'SHARE') : normalizeCode(request.headers.get('X-Fury-Pass-Key'), 'FURY');
  if (!symbols) return error('bad_key', 400);
  const db = env.SAVES;
  // Cloudflare supplies this IP header; clients cannot choose another identity.
  const ip = request.headers.get('CF-Connecting-IP');
  if (!db || !ip) return unavailable();
  try {
    const id = await hashSymbols(symbols);
    const binding = { read: 'SAVE_READ_LIMIT', write: 'SAVE_WRITE_LIMIT', share: 'SAVE_SHARE_LIMIT' }[kind];
    const ok = await allowed(env, binding, kind === 'write' ? [`ip:${ip}`, `key:${id}`] : [`ip:${ip}`]);
    if (ok === null) return unavailable();
    if (!ok) return rateLimited();
    if (shareLookup) {
      const row = await db.prepare('SELECT created_at, save FROM shares WHERE id = ?1').bind(id).first();
      return row ? json({ createdAt: row.created_at, save: JSON.parse(row.save) }) : error('not_found', 404);
    }
    if (path === '/api/save' && request.method === 'GET') {
      const row = await db.prepare('SELECT revision, updated_at, save FROM keys WHERE id = ?1').bind(id).first();
      return row ? json({ revision: row.revision, updatedAt: row.updated_at, save: JSON.parse(row.save) }) : error('not_found', 404);
    }
    if (path === '/api/save') return await putSave(request, env, db, id, ip);
    if (path === '/api/save/share') return await share(db, id);
    if (path === '/api/save/restore') return await restore(request, db, id);
    const { results } = await db.prepare('SELECT revision, updated_at, bytes FROM revisions WHERE key_id = ?1 ORDER BY revision DESC LIMIT ?2').bind(id, KEEP_REVISIONS).all();
    if (!results?.length) return error('not_found', 404);
    return json({ revisions: results.map(row => ({ revision: row.revision, updatedAt: row.updated_at, bytes: row.bytes })) });
  } catch {
    // Storage or limiter failure. Never echo request data or keys into logs.
    return unavailable();
  }
}
