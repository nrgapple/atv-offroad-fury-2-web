import assert from 'node:assert/strict';
import test from 'node:test';
import worker from './worker.mjs';
import { checkGameServer, gameServerStatus, probe } from './gameserver.mjs';
import { d1 } from './testing.mjs';

const MINUTE = 60_000, DAY = 86_400_000;
const NOW = Date.parse('2026-11-13T12:00:00Z');
const HEALTHY = { ok: true, commit: 'abc1234', protocol: 6, uptimeSeconds: 90, rooms: 3, riders: 7, connections: 9 };

// A fetch stand-in: /healthz answers `health()`; GitHub calls are recorded.
function network(health) {
  const calls = [];
  let issue = 600;
  const fetcher = async (url, init = {}) => {
    calls.push({ url: String(url), method: init.method ?? 'GET', body: init.body ? JSON.parse(init.body) : undefined, headers: init.headers });
    if (String(url).startsWith('https://api.github.com/')) {
      if (init.method === 'POST' && String(url).endsWith('/issues')) return Response.json({ number: ++issue }, { status: 201 });
      return Response.json({}, { status: 200 });
    }
    const answer = health();
    if (answer instanceof Error) throw answer;
    return typeof answer === 'number' ? new Response('down', { status: answer }) : Response.json(answer);
  };
  return { fetcher, calls, github: () => calls.filter(call => call.url.startsWith('https://api.github.com/')) };
}

function clock(start = NOW) {
  let time = start;
  const now = () => time;
  now.advance = ms => { time += ms; };
  return now;
}

const env = (extra = {}) => ({ SAVES: d1(), GAME_SERVER_URL: 'https://mp.example.test', ...extra });
const rows = saves => saves.db.prepare('SELECT * FROM game_server_checks ORDER BY checked_at').all().map(row => ({ ...row }));

test('no URL means no check, no fetch and no write', async () => {
  for (const url of [undefined, '', '  ']) {
    const { fetcher, calls } = network(() => HEALTHY);
    const saves = d1();
    assert.equal(await checkGameServer({ SAVES: saves, GAME_SERVER_URL: url }, { fetcher }), null);
    assert.equal(calls.length, 0);
    assert.equal(saves.prepared, 0);
  }
});

test('a healthy answer records commit, counts and latency', async () => {
  const now = clock();
  const { fetcher, calls } = network(() => { now.advance(42); return HEALTHY; });
  const config = env({ GAME_SERVER_URL: 'https://mp.example.test/' });
  const result = await checkGameServer(config, { fetcher, now });
  assert.equal(calls[0].url, 'https://mp.example.test/healthz');
  assert.equal(result.ok, true);
  assert.deepEqual(rows(config.SAVES), [{
    checked_at: NOW, ok: 1, status: 200, latency_ms: 42, build: 'abc1234', rooms: 3, riders: 7, connections: 9, error: null,
  }]);
});

test('errors, timeouts and unhealthy bodies are failures with a reason', async () => {
  const timeout = Object.assign(new Error('aborted'), { name: 'TimeoutError' });
  const cases = [
    [503, 'HTTP 503'],
    [{ ok: false, commit: 'abc1234' }, 'healthz did not report ok'],
    [timeout, 'no answer within 8 s'],
    [new TypeError('connection refused'), 'connection refused'],
  ];
  for (const [answer, error] of cases) {
    const { fetcher } = network(() => answer);
    const result = await probe('https://mp.example.test', fetcher, clock());
    assert.equal(result.ok, false);
    assert.equal(result.error, error);
  }
  // Values that are not counts are not stored as counts.
  const { fetcher } = network(() => ({ ...HEALTHY, rooms: -1, riders: 'many', commit: 7 }));
  const odd = await probe('https://mp.example.test', fetcher, clock());
  assert.deepEqual([odd.ok, odd.rooms, odd.riders, odd.build], [true, null, null, null]);
});

test('rows older than 30 days are pruned on each check', async () => {
  const config = env();
  const now = clock(NOW - 31 * DAY);
  const { fetcher } = network(() => HEALTHY);
  await checkGameServer(config, { fetcher, now });
  now.advance(2 * DAY);
  await checkGameServer(config, { fetcher, now });
  assert.equal(rows(config.SAVES).length, 2);
  now.advance(29 * DAY);
  await checkGameServer(config, { fetcher, now });
  assert.deepEqual(rows(config.SAVES).map(row => row.checked_at), [NOW - 29 * DAY, NOW]);
});

test('status reports the latest check, 24-hour and 7-day uptime and recent failures', async () => {
  const config = env();
  const now = clock(NOW - 6 * DAY);
  let up = true;
  const { fetcher } = network(() => (up ? HEALTHY : 502));
  // Six days ago: 3 failures out of 4. Last day: 1 failure out of 4.
  for (const healthy of [true, false, false, false]) { up = healthy; await checkGameServer(config, { fetcher, now }); now.advance(5 * MINUTE); }
  now.advance(6 * DAY - 40 * MINUTE);
  for (const healthy of [true, true, false, true]) { up = healthy; await checkGameServer(config, { fetcher, now }); now.advance(5 * MINUTE); }
  const response = await gameServerStatus(new Request('https://furyremastered.com/status/game-server'), config, { now });
  assert.equal(response.status, 200);
  assert.match(response.headers.get('Cache-Control'), /max-age=60/);
  const body = await response.json();
  assert.equal(body.configured, true);
  assert.equal(body.latest.ok, true);
  assert.equal(body.latest.commit, 'abc1234');
  assert.deepEqual(body.uptime, { last24Hours: { percent: 75, checks: 4 }, last7Days: { percent: 50, checks: 8 } });
  assert.equal(body.recentFailures.length, 4);
  assert.equal(body.recentFailures[0].error, 'HTTP 502');
  assert.ok(Date.parse(body.recentFailures[0].checkedAt) > Date.parse(body.recentFailures[1].checkedAt));
});

test('status without checks or configuration, and through the Worker', async () => {
  const empty = await (await gameServerStatus(new Request('https://furyremastered.com/status/game-server'), { SAVES: d1(), GAME_SERVER_URL: '' })).json();
  assert.deepEqual(empty, {
    configured: false, intervalMinutes: 5, latest: null,
    uptime: { last24Hours: { percent: null, checks: 0 }, last7Days: { percent: null, checks: 0 } }, recentFailures: [],
  });
  const routed = await worker.fetch(new Request('https://furyremastered.com/status/game-server'), { SAVES: d1() });
  assert.equal(routed.status, 200);
  assert.equal(routed.headers.get('Content-Type'), 'application/json');
  const post = await worker.fetch(new Request('https://furyremastered.com/status/game-server', { method: 'POST' }), { SAVES: d1() });
  assert.equal(post.status, 405);
});

test('the cron handler runs the check in the background', async () => {
  const pending = [];
  const config = env();
  const original = globalThis.fetch;
  globalThis.fetch = async () => Response.json(HEALTHY);
  try {
    worker.scheduled({ cron: '*/5 * * * *' }, config, { waitUntil: promise => pending.push(promise) });
    await Promise.all(pending);
  } finally {
    globalThis.fetch = original;
  }
  assert.equal(rows(config.SAVES).length, 1);
});

test('no alert token: failures never call GitHub', async () => {
  const { fetcher, github } = network(() => 503);
  const config = env();
  for (let i = 0; i < 4; i++) await checkGameServer(config, { fetcher, now: clock(NOW + i * 5 * MINUTE) });
  assert.equal(github().length, 0);
});

test('alert: one issue after two failures, none duplicated, closed on recovery', async () => {
  let answer = 503;
  const { fetcher, github } = network(() => answer);
  const config = env({ GITHUB_ALERT_TOKEN: 'test-token', CANONICAL_ORIGIN: 'https://furyremastered.com' });
  const now = clock();
  const run = async () => { await checkGameServer(config, { fetcher, now }); now.advance(5 * MINUTE); };

  await run();
  assert.equal(github().length, 0, 'one failure is not an outage');
  await run();
  assert.equal(github().length, 1);
  const [opened] = github();
  assert.equal(opened.url, 'https://api.github.com/repos/nrgapple/atv-offroad-fury-2-web/issues');
  assert.deepEqual(opened.body.labels, ['area: infra']);
  assert.equal(opened.headers.Authorization, 'Bearer test-token');
  assert.match(opened.body.body, /https:\/\/furyremastered\.com\/status\/game-server/);
  await run();
  await run();
  assert.equal(github().length, 1, 'still the same outage');

  answer = HEALTHY;
  await run();
  const [, comment, close] = github();
  assert.equal(comment.url, 'https://api.github.com/repos/nrgapple/atv-offroad-fury-2-web/issues/601/comments');
  assert.match(comment.body.body, /Recovered/);
  assert.deepEqual([close.method, close.url.endsWith('/issues/601'), close.body.state], ['PATCH', true, 'closed']);
  await run();
  assert.equal(github().length, 3, 'healthy with no open issue does nothing');

  answer = 503;
  await run();
  await run();
  assert.equal(github().at(-1).url.endsWith('/issues'), true, 'a new outage opens a new issue');
  assert.equal(github().length, 4);
});

test('alert: a failed GitHub call is retried by the next run', async () => {
  let githubUp = false;
  const calls = [];
  const fetcher = async (url, init = {}) => {
    if (!String(url).startsWith('https://api.github.com/')) return new Response('down', { status: 503 });
    calls.push(init.method);
    return githubUp ? Response.json({ number: 9 }, { status: 201 }) : new Response('nope', { status: 500 });
  };
  const config = env({ GITHUB_ALERT_TOKEN: 'test-token' });
  const now = clock();
  await checkGameServer(config, { fetcher, now }); now.advance(5 * MINUTE);
  await assert.rejects(checkGameServer(config, { fetcher, now }), /HTTP 500/); now.advance(5 * MINUTE);
  assert.equal(rows(config.SAVES).length, 2, 'the check is recorded even when alerting fails');
  githubUp = true;
  await checkGameServer(config, { fetcher, now });
  assert.deepEqual(calls, ['POST', 'POST']);
  assert.deepEqual(config.SAVES.db.prepare('SELECT issue FROM game_server_alert').all().map(row => ({ ...row })), [{ issue: 9 }]);
});
