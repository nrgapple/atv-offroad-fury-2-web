import assert from 'node:assert/strict';
import test from 'node:test';
import worker from './worker.mjs';
import { DAILY_POOL, FIRST_MOTORCYCLE, defaultLaps, judge, parseBoard } from './boards.mjs';
import {
  BANDS, KEEP_WEEKS, STATS_REFRESH_MS, WEEKLY_GRACE_MS, bandRanks, checkWeeklyLaps, isoWeekOf, parseWeek, refreshWeekly,
  weekSpan, weeklyBadge, weeklyPick,
} from './weekly.mjs';
import { d1, limiter } from './testing.mjs';

function setup() {
  return { SAVES: d1(), GAME_ORIGINS: 'https://furyremastered.com', BOARD_READ_LIMIT: limiter(), BOARD_SUBMIT_LIMIT: limiter() };
}
function call(env, method, path, body) {
  const request = new Request(`https://furyremastered.com${path}`, {
    method, headers: { 'CF-Connecting-IP': '203.0.113.9' }, body: body === undefined ? undefined : JSON.stringify(body),
  });
  return worker.fetch(request, env).then(async response => ({ status: response.status, body: await response.json(), headers: response.headers }));
}

// A race on a circular course of `feet`: `laps` laps of `lapSeconds`, one
// point a second; `edit` changes the points.
function race(laps, lapSeconds, feet = 4000, edit) {
  const radius = feet / (2 * Math.PI), points = [];
  for (let t = 0; t <= laps * lapSeconds; t++) {
    const angle = (2 * Math.PI * t) / lapSeconds;
    points.push([Math.round(radius * Math.cos(angle)), Math.round(radius * Math.sin(angle))]);
  }
  edit?.(points);
  return { points, marks: Array.from({ length: laps }, (_, i) => (i + 1) * lapSeconds * 1000) };
}
const NOW = Date.parse('2026-11-14T12:00:00Z');
const WEEK = '2026-W46';
// The week's pick: a full-length race of one-minute laps (above every floor
// of the pool's courses but the longest; the test picks the lap to fit).
function weeklyRun(overrides = {}, week = WEEK) {
  const pick = weeklyPick(week), laps = defaultLaps(pick.course);
  const lapSeconds = 220;
  return {
    name: 'ADA', vehicle: pick.vehicleClass === 'motorcycle' ? FIRST_MOTORCYCLE : 3, course: pick.course,
    raceMs: laps * lapSeconds * 1000, weekly: week, trace: race(laps, lapSeconds, 12_000), ...overrides,
  };
}

test('ISO weeks: ids, spans and edges of the year', () => {
  assert.equal(isoWeekOf(NOW), '2026-W46');
  assert.deepEqual(weekSpan('2026-W46'), { start: Date.parse('2026-11-09T00:00:00Z'), end: Date.parse('2026-11-16T00:00:00Z') });
  // 2027-01-01 (a Friday) belongs to 2026's 53rd week; 2021 has 52.
  assert.equal(isoWeekOf(Date.parse('2027-01-01T00:00:00Z')), '2026-W53');
  assert.equal(isoWeekOf(Date.parse('2024-12-30T00:00:00Z')), '2025-W01');
  assert.equal(isoWeekOf(Date.parse('1970-01-01T00:00:00Z')), '1970-W01');
  assert.equal(parseWeek('2021-W53'), null);
  assert.equal(parseWeek('2026-W00'), null);
  assert.notEqual(parseWeek('2020-W53'), null);
  assert.equal(parseBoard('weekly:2026-W46').kind, 'weekly');
  assert.equal(parseBoard('weekly:2026-W54'), null);
  // A Sunday night is still the week; Monday 00:00 starts the next.
  assert.equal(isoWeekOf(Date.parse('2026-11-15T23:59:59Z')), '2026-W46');
  assert.equal(isoWeekOf(Date.parse('2026-11-16T00:00:00Z')), '2026-W47');
});

test('the weekly pick is a pure function of the week, shared with the game', () => {
  const pick = weeklyPick(WEEK);
  assert.deepEqual(weeklyPick(WEEK), pick);
  assert.ok(DAILY_POOL.includes(pick.course));
  // atv_core::medals::weekly pins the same weeks.
  assert.deepEqual(['2026-W46', '2026-W47', '2027-W01'].map(id => `${weeklyPick(id).course}/${weeklyPick(id).vehicleClass}`), WEEKLY_EXPECTED);
  const seen = new Set(Array.from({ length: 104 }, (_, i) => weeklyPick(isoWeekOf(NOW + i * 7 * 86_400_000)).course));
  assert.ok(seen.size > 25, `two years visit most courses (${seen.size})`);
});
const WEEKLY_EXPECTED = ['sx08/atv', 'sx12/atv', 'env06ends/atv'];

test('weekly submissions: the week, the pick, Normal and every lap\'s trace', () => {
  assert.equal(judge(weeklyRun(), NOW).reason, undefined);
  assert.deepEqual(judge(weeklyRun(), NOW).entries.map(e => e[0]), [`race:${weeklyPick(WEEK).course}:n`, `weekly:${WEEK}`]);
  const reason = (overrides, now = NOW) => judge(weeklyRun(overrides), now).reason;
  assert.equal(reason({ weekly: '2026-W45' }), 'weekly_week');
  // Last week's is taken for half an hour after it ends.
  const end = weekSpan(WEEK).end;
  assert.equal(reason({}, end + WEEKLY_GRACE_MS - 1000), undefined);
  assert.equal(reason({}, end + WEEKLY_GRACE_MS + 1000), 'weekly_week');
  assert.equal(reason({ expert: true }), 'weekly_pick');
  assert.equal(reason({ vehicle: weeklyPick(WEEK).vehicleClass === 'atv' ? FIRST_MOTORCYCLE : 3 }), 'weekly_pick');
  assert.equal(reason({ raceMs: undefined, lapMs: 220_000 }), 'weekly_pick');
  // A cut lap: one flying lap rides far less ground than the others.
  const laps = defaultLaps(weeklyPick(WEEK).course);
  const cut = race(laps, 220, 12_000, points => { for (let t = 2 * 220 + 1; t < 3 * 220; t++) points[t] = points[2 * 220]; });
  assert.equal(checkWeeklyLaps(cut, weeklyPick(WEEK).course), 'lap_short');
  const short = race(laps, 220, 12_000, points => { for (let t = 2 * 220 + 1; t < 2 * 220 + 150; t++) points[t] = points[2 * 220]; });
  assert.equal(checkWeeklyLaps(short, weeklyPick(WEEK).course), 'lap_ground');
  // A lap that ends far from where it began.
  const open = race(laps, 220, 12_000, points => { points[2 * 220] = [points[2 * 220][0] + 500, points[2 * 220][1]]; });
  assert.equal(checkWeeklyLaps(open, weeklyPick(WEEK).course), 'lap_open');
  assert.equal(checkWeeklyLaps(race(laps, 220, 12_000), weeklyPick(WEEK).course), null);
});

test('a weekly board keeps every finisher, answers a percentile and serves the cron\'s cuts', async () => {
  const env = setup();
  const board = `weekly:${WEEK}`;
  // 60 riders: more than a board's top 50, all kept.
  for (let i = 0; i < 60; i++) {
    const answer = await call(env, 'POST', '/api/boards/submit', weeklyRun({ name: `R${i}`, raceMs: undefined }));
    assert.equal(answer.status, 422, 'a weekly run needs its race time');
  }
  const submit = (name, seconds) => {
    const laps = defaultLaps(weeklyPick(WEEK).course);
    return worker.fetch(new Request('https://furyremastered.com/api/boards/submit', {
      method: 'POST', headers: { 'CF-Connecting-IP': '203.0.113.9' },
      body: JSON.stringify(weeklyRun({ name, raceMs: laps * seconds * 1000, trace: race(laps, seconds, 12_000) })),
    }), env).then(r => r.json());
  };
  // Date.now is the judge's clock: pin it to the week.
  const realNow = Date.now;
  Date.now = () => NOW;
  try {
    for (let i = 0; i < 60; i++) {
      const answer = await submit(`R${i}`, 220 + i);
      assert.equal(answer.boards[board].placed, true, `R${i}`);
    }
    const count = env.SAVES.db.prepare('SELECT COUNT(*) AS n FROM board_entries WHERE board = ?').get(board).n;
    assert.equal(count, 60);
    // Before the cron's first run the board has no cuts; the percentile
    // counts the riders ahead against at least the rank.
    let read = await call(env, 'GET', `/api/boards/${board}`);
    assert.equal(read.status, 200);
    assert.deepEqual([read.body.riders, read.body.cuts, read.body.final], [0, {}, false]);
    assert.equal(read.body.pick.course, weeklyPick(WEEK).course);
    assert.equal(read.body.ends, weekSpan(WEEK).end);
    assert.equal(read.body.entries.length, 50);
    await refreshWeekly(env, NOW);
    read = await call(env, 'GET', `/api/boards/${board}`);
    assert.equal(read.body.riders, 60);
    const laps = defaultLaps(weeklyPick(WEEK).course);
    // Top 1 % of 60 is the first rider, 10 % the 6th, 25 % the 15th, 50 % the 30th.
    assert.deepEqual(bandRanks(60), [1, 6, 15, 30]);
    assert.deepEqual(read.body.cuts, Object.fromEntries(BANDS.map((band, i) => [band, laps * (220 + [0, 5, 14, 29][i]) * 1000])));
    // A new rider at the 40th place of 61: ceil(100 × 40 / 61) = 66.
    const answer = await submit('NEW', 220 + 39);
    assert.equal(answer.boards[board].percentile, Math.ceil((100 * 40) / 60));
    assert.equal(answer.boards[board].riders, 60);
    // The cron refreshes no sooner than 15 minutes, then freezes the week.
    assert.deepEqual(await refreshWeekly(env, NOW + 60_000), []);
    assert.equal((await refreshWeekly(env, NOW + STATS_REFRESH_MS))[0].riders, 61);
    const over = weekSpan(WEEK).end + WEEKLY_GRACE_MS;
    assert.equal((await refreshWeekly(env, over))[0].final, true);
    assert.deepEqual((await refreshWeekly(env, over + STATS_REFRESH_MS)).filter(s => s.final), [], 'a final week is not counted again');
  } finally { Date.now = realNow; }
  // Old weeks' rows go; their final cuts stay.
  await refreshWeekly(env, weekSpan(WEEK).end + KEEP_WEEKS * 7 * 86_400_000 + 1);
  assert.equal(env.SAVES.db.prepare('SELECT COUNT(*) AS n FROM board_entries WHERE board = ?').get(board).n, 0);
  assert.equal(env.SAVES.db.prepare('SELECT final FROM board_stats WHERE board = ?').get(board).final, 1);
});

test('badges: the best band a weekly best is inside, else finished', () => {
  const cuts = { 1: 100, 10: 200, 25: 300, 50: 400 };
  assert.deepEqual([50, 100, 150, 300, 399, 401].map(v => weeklyBadge(v, cuts)), ['top1', 'top1', 'top10', 'top25', 'top50', 'finished']);
  assert.equal(weeklyBadge(100, {}), 'finished');
});
