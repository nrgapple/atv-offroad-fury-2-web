// Infrastructure guardrails; this object never receives player names or host credentials.
export const MAX_CREATE_BYTES = 512;
export const ADMISSION_HEADER = 'X-ATV-Admission-Lease';
const MINUTE = 60_000, DAY = 86_400_000, ROOM_LIFETIME = 12 * 60 * MINUTE;
export const DEFAULT_BUDGET = Object.freeze({ rooms: 256, createsPerMinute: 120, createsPerDay: 2000, connectsPerMinute: 2000, connectsPerDay: 100000 });

export function reserveBudget(state, action, now, limits = DEFAULT_BUDGET) {
  state.leases = (state.leases ?? []).filter(lease => lease.expires > now);
  if (state.minute !== Math.floor(now / MINUTE)) Object.assign(state, { minute: Math.floor(now / MINUTE), creates: 0, connects: 0 });
  if (state.day !== Math.floor(now / DAY)) Object.assign(state, { day: Math.floor(now / DAY), dailyCreates: 0, dailyConnects: 0 });
  if (action === 'create') {
    if (state.leases.length >= limits.rooms || state.creates >= limits.createsPerMinute || state.dailyCreates >= limits.createsPerDay) return false;
    state.creates++; state.dailyCreates++;
  } else {
    if (state.connects >= limits.connectsPerMinute || state.dailyConnects >= limits.connectsPerDay) return false;
    state.connects++; state.dailyConnects++;
  }
  return true;
}

export class LobbyAdmission {
  constructor(state) { this.state = state; }
  async fetch(request) {
    const path = new URL(request.url).pathname;
    if (request.method !== 'POST' || !['/create', '/connect', '/commit', '/release'].includes(path)) return new Response('Not found', { status: 404 });
    // Serialize admission and retirement globally, including persistence awaits.
    return this.state.blockConcurrencyWhile(async () => {
      const state = await this.state.storage.get('budget') ?? {};
      const now = Date.now();
      let result = {};
      if (path === '/create' || path === '/connect') {
        if (!reserveBudget(state, path.slice(1), now)) return new Response('Multiplayer capacity reached. Try again later.', { status: 429, headers: { 'Retry-After': '60' } });
        if (path === '/create') {
          const lease = crypto.randomUUID();
          state.leases.push({ id: lease, expires: now + MINUTE });
          result = { lease };
        }
      } else {
        const { lease } = await request.json();
        if (typeof lease !== 'string' || !/^[a-f0-9-]{36}$/.test(lease)) return new Response('Invalid lease', { status: 400 });
        state.leases = (state.leases ?? []).filter(item => item.expires > now);
        if (path === '/release') state.leases = state.leases.filter(item => item.id !== lease);
        else {
          const item = state.leases.find(item => item.id === lease);
          if (!item) return new Response('Reservation expired', { status: 409 });
          item.expires = now + ROOM_LIFETIME;
        }
      }
      await this.state.storage.put('budget', state);
      return Response.json(result);
    });
  }
}

async function boundedBody(request) {
  const length = request.headers.get('Content-Length');
  if (length !== null && (!/^\d+$/.test(length) || Number(length) > MAX_CREATE_BYTES)) return null;
  if (!request.body) return '';
  const reader = request.body.getReader(), chunks = []; let size = 0;
  try {
    while (true) {
      const { done, value } = await reader.read(); if (done) break;
      size += value.byteLength;
      if (size > MAX_CREATE_BYTES) { await reader.cancel(); return null; }
      chunks.push(value);
    }
  } finally { reader.releaseLock(); }
  const bytes = new Uint8Array(size); let offset = 0;
  for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.length; }
  return new TextDecoder('utf-8', { fatal: true }).decode(bytes);
}

// The main menu's Free Ride rider count is public and read-only. Each edge
// location answers it from cache for ten seconds, so menu views across all
// players reach the relay's single counter only a few times a minute.
const FREERIDE_RIDERS = '/multiplayer/freeride/riders';
async function freerideRiders(request, env) {
  const cache = globalThis.caches?.default, key = new Request(new URL(FREERIDE_RIDERS, request.url));
  const cached = await cache?.match(key);
  if (cached) return cached;
  const response = await env.MULTIPLAYER.fetch(new Request(key));
  if (response.ok && cache) await cache.put(key, response.clone());
  return response;
}

export async function multiplayerRequest(request, env) {
  const path = new URL(request.url).pathname;
  if (path === FREERIDE_RIDERS) return request.method === 'GET' ? freerideRiders(request, env) : new Response('Method not allowed', { status: 405, headers: { Allow: 'GET' } });
  const create = path === '/multiplayer/rooms' && request.method === 'POST';
  // Claiming an Arlington Free Ride slot is admitted like joining a room: the
  // shared rooms are a fixed set, so they never take a room-creation lease.
  const connect = (/^\/multiplayer\/rooms\/[A-Z0-9]{8}$/.test(path) && request.method === 'GET') || (path === '/multiplayer/freeride' && request.method === 'POST');
  if (!create && !connect) return env.MULTIPLAYER.fetch(request);
  let body;
  if (create) {
    try { body = await boundedBody(request); } catch { return new Response('Invalid lobby request', { status: 400 }); }
    if (body === null) return new Response('Lobby request too large', { status: 413 });
    try {
      const value = body ? JSON.parse(body) : {};
      if (!value || typeof value !== 'object' || Array.isArray(value) || (value.code != null && (typeof value.code !== 'string' || (value.code.trim() && !/^[A-Z0-9]{8}$/i.test(value.code.trim()))))) throw Error('code');
    } catch { return new Response('Invalid lobby request', { status: 400 }); }
  }
  if (env.MULTIPLAYER_GUARDS === 'host-only') return env.MULTIPLAYER.fetch(create ? new Request(request, { body }) : request);
  const limiter = create ? env.ROOM_CREATE_LIMIT : env.ROOM_CONNECT_LIMIT;
  if (!limiter || !env.ADMISSION) return new Response('Multiplayer temporarily unavailable', { status: 503 });
  let lease, stub;
  try {
    // Cloudflare supplies this IP header; browsers cannot choose another client's identity.
    const ip = request.headers.get('CF-Connecting-IP');
    if (!ip) return new Response('Multiplayer temporarily unavailable', { status: 503 });
    if (!(await limiter.limit({ key: ip })).success) return new Response('Too many lobby requests. Try again later.', { status: 429, headers: { 'Retry-After': '60' } });
    stub = env.ADMISSION.get(env.ADMISSION.idFromName('global-budget-v1'));
    const reservation = await stub.fetch(`https://admission/${create ? 'create' : 'connect'}`, { method: 'POST' });
    if (!reservation.ok) return reservation;
    if (create) lease = (await reservation.json()).lease;
    const headers = new Headers(request.headers); headers.delete(ADMISSION_HEADER);
    if (lease) headers.set(ADMISSION_HEADER, lease);
    const response = await env.MULTIPLAYER.fetch(new Request(request, { headers, ...(create ? { body } : {}) }));
    if (lease) {
      const result = await stub.fetch(`https://admission/${response.status === 201 ? 'commit' : 'release'}`, { method: 'POST', body: JSON.stringify({ lease }) });
      if (!result.ok) return new Response('Multiplayer temporarily unavailable', { status: 503 });
    }
    return response;
  } catch {
    if (lease && stub) { try { await stub.fetch('https://admission/release', { method: 'POST', body: JSON.stringify({ lease }) }); } catch { /* The short reservation also expires automatically. */ } }
    return new Response('Multiplayer temporarily unavailable', { status: 503 });
  }
}

export function secureResponse(response, request, { webAnalytics = false } = {}) {
  // Reconstructing an upgrade response would discard its accepted WebSocket.
  if (response.status === 101) return response;
  const secured = new Response(response.body, response);
  const socketOrigin = new URL(request.url).origin.replace(/^http/, 'ws');
  // Cloudflare Web Analytics loads its beacon and reports to these origins only when enabled.
  const beaconScript = webAnalytics ? ' https://static.cloudflareinsights.com' : '', beaconConnect = webAnalytics ? ' https://cloudflareinsights.com' : '';
  secured.headers.set('Content-Security-Policy', `default-src 'self'; script-src 'self' 'wasm-unsafe-eval'${beaconScript}; worker-src 'self' blob:; style-src 'self' 'unsafe-inline'; img-src 'self' blob: data:; media-src 'self' blob:; connect-src 'self' ${socketOrigin}${beaconConnect}; object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'`);
  secured.headers.set('X-Content-Type-Options', 'nosniff');
  secured.headers.set('X-Frame-Options', 'DENY');
  secured.headers.set('Referrer-Policy', 'no-referrer');
  secured.headers.set('Permissions-Policy', 'camera=(), microphone=(), geolocation=(), payment=(), usb=()');
  if (new URL(request.url).protocol === 'https:') secured.headers.set('Strict-Transport-Security', 'max-age=31536000');
  return secured;
}
