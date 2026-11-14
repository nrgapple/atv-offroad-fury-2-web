// Game server health (research/GAME_SERVER.md "Running it"): a five-minute
// cron fetches ${GAME_SERVER_URL}/healthz, records the answer in D1 for 30 days
// and, when GITHUB_ALERT_TOKEN is set, keeps one GitHub issue open while the
// server is down. GET /status/game-server reports the latest check, uptime and
// recent failures. Each run is one fetch and one or two D1 calls.
export const GAME_SERVER_STATUS_PATH = '/status/game-server';
const MINUTE = 60_000, DAY = 86_400_000;
const RETENTION = 30 * DAY;
const TIMEOUT_MS = 8_000;
const FAILURES_TO_ALERT = 2;
const REPOSITORY = 'nrgapple/atv-offroad-fury-2-web';
const ALERT_LABEL = 'area: infra';

const count = value => (Number.isSafeInteger(value) && value >= 0 ? value : null);
const text = (value, limit = 80) => (typeof value === 'string' && value ? value.slice(0, limit) : null);

// One /healthz request: up only on 200 with {"ok": true}.
export async function probe(base, fetcher = fetch, now = Date.now) {
  const started = now();
  try {
    const response = await fetcher(`${base.replace(/\/+$/, '')}/healthz`, {
      headers: { Accept: 'application/json', 'User-Agent': 'furyremastered-health/1' },
      redirect: 'manual', signal: AbortSignal.timeout(TIMEOUT_MS),
    });
    let body = null;
    try { body = JSON.parse((await response.text()).slice(0, 4096)); } catch { /* not JSON */ }
    const latency = now() - started;
    const ok = response.status === 200 && body?.ok === true;
    return {
      ok, status: response.status, latency, build: text(body?.commit), rooms: count(body?.rooms),
      riders: count(body?.riders), connections: count(body?.connections),
      error: ok ? null : response.status === 200 ? 'healthz did not report ok' : `HTTP ${response.status}`,
    };
  } catch (failure) {
    const error = failure?.name === 'TimeoutError' ? `no answer within ${TIMEOUT_MS / 1000} s` : text(String(failure?.message ?? failure), 200);
    return { ok: false, status: null, latency: now() - started, build: null, rooms: null, riders: null, connections: null, error };
  }
}

async function github(env, fetcher, method, path, body) {
  const response = await fetcher(`https://api.github.com/repos/${env.GITHUB_ALERT_REPO || REPOSITORY}${path}`, {
    method, body: JSON.stringify(body), signal: AbortSignal.timeout(TIMEOUT_MS),
    headers: {
      Authorization: `Bearer ${env.GITHUB_ALERT_TOKEN}`, Accept: 'application/vnd.github+json',
      'Content-Type': 'application/json', 'User-Agent': 'furyremastered-health/1', 'X-GitHub-Api-Version': '2022-11-28',
    },
  });
  if (!response.ok) throw new Error(`GitHub ${method} ${path}: HTTP ${response.status}`);
  return response.json();
}

// Opens one issue after FAILURES_TO_ALERT failed checks in a row; on the next
// healthy check comments and closes it. The open issue number lives in D1, so a
// failed GitHub call is retried by the next run and never duplicated.
async function alert(env, result, at, fetcher) {
  const open = await env.SAVES.prepare('SELECT issue FROM game_server_alert WHERE id = 1').first();
  const status = `${env.CANONICAL_ORIGIN || 'https://furyremastered.com'}${GAME_SERVER_STATUS_PATH}`;
  if (result.ok) {
    if (!open) return;
    await github(env, fetcher, 'POST', `/issues/${open.issue}/comments`, {
      body: `Recovered at ${new Date(at).toISOString()}: \`/healthz\` answered in ${result.latency} ms (commit ${result.build ?? 'unknown'}). Closing. Status: ${status}`,
    });
    await github(env, fetcher, 'PATCH', `/issues/${open.issue}`, { state: 'closed', state_reason: 'completed' });
    await env.SAVES.prepare('DELETE FROM game_server_alert WHERE id = 1').run();
    return;
  }
  if (open) return;
  const recent = await env.SAVES.prepare('SELECT ok FROM game_server_checks ORDER BY checked_at DESC LIMIT ?').bind(FAILURES_TO_ALERT).all();
  if (recent.results.length < FAILURES_TO_ALERT || recent.results.some(row => row.ok)) return;
  const issue = await github(env, fetcher, 'POST', '/issues', {
    title: 'Game server is down',
    labels: [ALERT_LABEL],
    body: `The five-minute health check of \`${env.GAME_SERVER_URL}/healthz\` failed ${FAILURES_TO_ALERT} times in a row (latest at ${new Date(at).toISOString()}: ${result.error}).\n\nStatus: ${status}\n\nThis issue closes itself when the server answers again.`,
  });
  await env.SAVES.prepare('INSERT OR REPLACE INTO game_server_alert (id, issue, opened_at) VALUES (1, ?, ?)').bind(issue.number, at).run();
}

// The cron's work. Returns null when no game server is configured.
export async function checkGameServer(env, { fetcher = fetch, now = Date.now } = {}) {
  const base = (env.GAME_SERVER_URL ?? '').trim();
  if (!base || !env.SAVES) return null;
  const at = now();
  const result = await probe(base, fetcher, now);
  await env.SAVES.batch([
    env.SAVES.prepare(`INSERT OR REPLACE INTO game_server_checks
      (checked_at, ok, status, latency_ms, build, rooms, riders, connections, error) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`)
      .bind(at, result.ok ? 1 : 0, result.status, result.latency, result.build, result.rooms, result.riders, result.connections, result.error),
    env.SAVES.prepare('DELETE FROM game_server_checks WHERE checked_at < ?').bind(at - RETENTION),
  ]);
  if (env.GITHUB_ALERT_TOKEN) await alert(env, result, at, fetcher);
  return result;
}

const percent = (up, total) => (total ? Math.round((10_000 * (up ?? 0)) / total) / 100 : null);
const check = row => row && {
  checkedAt: new Date(row.checked_at).toISOString(), ok: row.ok === 1, status: row.status, latencyMs: row.latency_ms,
  commit: row.build, rooms: row.rooms, riders: row.riders, connections: row.connections, error: row.error,
};

export async function gameServerStatus(request, env, { now = Date.now } = {}) {
  if (request.method !== 'GET' && request.method !== 'HEAD') return new Response('Method not allowed', { status: 405, headers: { Allow: 'GET, HEAD' } });
  if (!env.SAVES) return new Response('Status unavailable', { status: 503 });
  // The edge cache keeps a burst of readers to one D1 call a minute.
  const cache = globalThis.caches?.default;
  const key = new Request(new URL(GAME_SERVER_STATUS_PATH, request.url));
  const cached = cache && await cache.match(key);
  if (cached) return request.method === 'HEAD' ? new Response(null, cached) : cached;
  const at = now();
  const [latest, windows, failures] = await env.SAVES.batch([
    env.SAVES.prepare('SELECT * FROM game_server_checks ORDER BY checked_at DESC LIMIT 1'),
    env.SAVES.prepare(`SELECT COUNT(*) AS weekTotal, SUM(ok) AS weekUp,
      SUM(CASE WHEN checked_at >= ? THEN 1 ELSE 0 END) AS dayTotal, SUM(CASE WHEN checked_at >= ? THEN ok ELSE 0 END) AS dayUp
      FROM game_server_checks WHERE checked_at >= ?`).bind(at - DAY, at - DAY, at - 7 * DAY),
    env.SAVES.prepare('SELECT * FROM game_server_checks WHERE ok = 0 AND checked_at >= ? ORDER BY checked_at DESC LIMIT 20').bind(at - 7 * DAY),
  ]);
  const window = windows.results[0] ?? {};
  const body = JSON.stringify({
    configured: Boolean((env.GAME_SERVER_URL ?? '').trim()),
    intervalMinutes: 5,
    latest: check(latest.results[0]) ?? null,
    uptime: {
      last24Hours: { percent: percent(window.dayUp, window.dayTotal), checks: window.dayTotal ?? 0 },
      last7Days: { percent: percent(window.weekUp, window.weekTotal), checks: window.weekTotal ?? 0 },
    },
    recentFailures: failures.results.map(check),
  });
  const response = new Response(body, {
    headers: {
      'Content-Type': 'application/json', 'Cache-Control': `public, max-age=${MINUTE / 1000}`,
      'Access-Control-Allow-Origin': '*', 'X-Content-Type-Options': 'nosniff',
    },
  });
  if (cache) await cache.put(key, response.clone());
  return request.method === 'HEAD' ? new Response(null, response) : response;
}
