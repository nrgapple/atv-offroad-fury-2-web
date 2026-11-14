// The weekly challenge (research/GHOST_CHALLENGES.md "Weekly challenge"): an
// ISO-week-seeded race course (`weeklyPick`, shared with
// atv_core::medals::weekly) raced as a full-length Normal time trial without
// CPUs, on board `weekly:<YYYY-Www>`. Unlike the other boards it keeps every
// finisher's best, so its percentiles count them all. The cuts (the times at
// the top 1, 10, 25 and 50 %) and the rider count are refreshed by the cron at
// most every 15 minutes into `board_stats` and frozen once the week is over;
// a board read adds that one row, so a read never counts the board. Badges
// are the client's: its own best against the final cuts.
import { DAILY_POOL, MAX_LAP_GAP_FEET, MAX_LAP_SPEED, MIN_LAP_FEET, defaultLaps, ridden } from './boards.mjs';

const DAY_MS = 86_400_000;
const WEEK_MS = 7 * DAY_MS;
// Submissions for last week's challenge are taken this long after it ends.
export const WEEKLY_GRACE_MS = 30 * 60_000;
// Edge and client cache for a weekly board read, seconds.
export const WEEKLY_MAX_AGE = 60;
// The percentiles a badge is given for, best first.
export const BANDS = [1, 10, 25, 50];
// The cron refreshes a live week's cuts at most this often.
export const STATS_REFRESH_MS = 15 * 60_000;
// Weekly rows older than this many weeks are deleted by the cron (their
// final cuts stay in board_stats for late badges).
export const KEEP_WEEKS = 5;
// A flying lap's ridden ground against the median flying lap's: a lap far
// shorter was cut, a lap far longer carries a jump the reset marks missed.
export const LAP_GROUND_RANGE = [0.6, 1.6];

// The ISO week (Monday to Sunday, UTC) of a day number, as its index from the
// week of 1970-01-01 (a Thursday) and its `YYYY-Www` id.
export const weekIndexOfDay = day => Math.floor((day + 3) / 7);
export function weekId(index) {
  const thursday = index * 7;
  const year = new Date(thursday * DAY_MS).getUTCFullYear();
  const first = Math.floor(Date.UTC(year, 0, 1) / DAY_MS);
  const week = Math.floor((thursday - first) / 7) + 1;
  return `${year}-W${String(week).padStart(2, '0')}`;
}
export const isoWeekOf = ms => weekId(weekIndexOfDay(Math.floor(ms / DAY_MS)));
// A `YYYY-Www` id's week index, or null when no such week exists.
export function parseWeek(id) {
  const m = /^(\d{4})-W(\d{2})$/.exec(id ?? '');
  if (!m) return null;
  // January 4th is always in week 1.
  const index = weekIndexOfDay(Math.floor(Date.UTC(Number(m[1]), 0, 4) / DAY_MS)) + Number(m[2]) - 1;
  return Number(m[2]) >= 1 && weekId(index) === id ? index : null;
}
// The week's first and last moments: Monday 00:00 UTC to the next Monday.
export function weekSpan(id) {
  const index = parseWeek(id);
  if (index === null) return null;
  const start = (index * 7 - 3) * DAY_MS;
  return { start, end: start + WEEK_MS };
}

function mix(index) {
  let x = Math.imul((index ^ 0x5bd1e995) >>> 0, 0x9e3779b1) >>> 0;
  x = Math.imul(x ^ (x >>> 15), 0x2c1b3c6d) >>> 0;
  return (x ^ (x >>> 13)) >>> 0;
}

// The week's pick: a championship race course (the daily pool) and 'atv' or
// 'motorcycle' (one week in four).
export function weeklyPick(id) {
  const index = parseWeek(id);
  const hash = mix(index ?? 0);
  return { week: id, course: DAILY_POOL[hash % DAILY_POOL.length], vehicleClass: (hash >>> 9) % 4 === 3 ? 'motorcycle' : 'atv' };
}

// Every lap of a weekly race, from its trace (checkRace has checked the marks
// and the race's average speed): each covers a lap's ground at a possible
// speed, each after the first closes where it began, and the flying laps'
// ground agrees with their median. Returns an error reason or null.
export function checkWeeklyLaps(trace, course) {
  const marks = trace?.marks ?? [];
  if (marks.length !== defaultLaps(course)) return 'race_marks';
  const flying = [];
  for (let i = 0; i < marks.length; i++) {
    const from = i ? marks[i - 1] : 0;
    const { feet, start, end } = ridden(trace, from, marks[i]);
    if (feet < MIN_LAP_FEET) return 'lap_short';
    if (feet / ((marks[i] - from) / 1000) > MAX_LAP_SPEED) return 'lap_speed';
    if (i) {
      if (Math.hypot(start[0] - end[0], start[1] - end[1]) > MAX_LAP_GAP_FEET) return 'lap_open';
      flying.push(feet);
    }
  }
  const sorted = [...flying].sort((a, b) => a - b);
  const median = sorted[Math.floor(sorted.length / 2)];
  if (flying.some(feet => feet < median * LAP_GROUND_RANGE[0] || feet > median * LAP_GROUND_RANGE[1])) return 'lap_ground';
  return null;
}

// The value at each band's rank among `riders` sorted rows (rank = ceil of
// the band's share, at least 1).
export const bandRanks = riders => BANDS.map(band => Math.max(1, Math.ceil((band / 100) * riders)));

// Counts a weekly board's riders and reads its cuts, then stores them.
// Reads about the board's rider count in rows (the count) plus half of it
// (the cuts); writes one row.
export async function refreshWeek(db, id, nowMs) {
  const board = `weekly:${id}`;
  const { riders } = await db.prepare('SELECT COUNT(*) AS riders FROM board_entries WHERE board = ?1').bind(board).first() ?? { riders: 0 };
  const cuts = {};
  if (riders) {
    const rows = await db.batch(bandRanks(riders).map(rank =>
      db.prepare('SELECT value FROM board_entries WHERE board = ?1 ORDER BY value, name LIMIT 1 OFFSET ?2').bind(board, rank - 1)));
    BANDS.forEach((band, i) => { const value = rows[i].results?.[0]?.value; if (value !== undefined) cuts[band] = value; });
  }
  const span = weekSpan(id);
  const final = nowMs >= span.end + WEEKLY_GRACE_MS ? 1 : 0;
  await db.prepare(`INSERT INTO board_stats (board, riders, cuts, final, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)
    ON CONFLICT (board) DO UPDATE SET riders = excluded.riders, cuts = excluded.cuts, final = excluded.final, updated_at = excluded.updated_at`)
    .bind(board, riders, JSON.stringify(cuts), final, nowMs).run();
  return { riders, cuts, final: Boolean(final) };
}

// The cron's part (worker.mjs `scheduled`): this week's cuts every 15
// minutes, last week's once more after it ends (then frozen), and weekly
// rows older than KEEP_WEEKS deleted. At most 12 D1 queries a run.
export async function refreshWeekly(env, nowMs = Date.now()) {
  const db = env.SAVES;
  if (!db) return null;
  const current = isoWeekOf(nowMs), previous = isoWeekOf(nowMs - WEEK_MS);
  const { results } = await db.prepare('SELECT board, final, updated_at FROM board_stats WHERE board IN (?1, ?2)')
    .bind(`weekly:${current}`, `weekly:${previous}`).all();
  const state = new Map((results ?? []).map(row => [row.board, row]));
  const done = [];
  for (const id of [previous, current]) {
    const row = state.get(`weekly:${id}`);
    if (row?.final) continue;
    // Last week's cuts become final once its grace is over.
    const over = nowMs >= weekSpan(id).end + WEEKLY_GRACE_MS;
    if (id === previous && !over && row) continue;
    if (!over && row && nowMs - row.updated_at < STATS_REFRESH_MS) continue;
    done.push(await refreshWeek(db, id, nowMs));
  }
  const oldest = isoWeekOf(nowMs - KEEP_WEEKS * WEEK_MS);
  await db.prepare("DELETE FROM board_entries WHERE board >= 'weekly:' AND board < ?1").bind(`weekly:${oldest}`).run();
  return done;
}

// A weekly board read's additions: the week's pick and end, its rider count
// and cuts (the cron's latest; none before its first run) and whether they
// are final. One row read.
export async function weeklyExtras(db, id) {
  const row = await db.prepare('SELECT riders, cuts, final FROM board_stats WHERE board = ?1').bind(`weekly:${id}`).first();
  let cuts = {};
  try { cuts = row ? JSON.parse(row.cuts) : {}; } catch { /* Kept empty. */ }
  return { pick: weeklyPick(id), ends: weekSpan(id).end, riders: row?.riders ?? 0, cuts, final: Boolean(row?.final) };
}

// After a submission stored rows (boards.mjs): a weekly board's answer gets
// the rider's percentile among the riders (the riders ahead counted exactly,
// the total from the cron's count, never fewer than the rank) in place of
// the internal fields. One row read per weekly board.
export async function weeklyPercentile(db, ranks) {
  for (const [board, result] of Object.entries(ranks)) {
    if (!board.startsWith('weekly:')) continue;
    const { ahead, value, ...rest } = result;
    ranks[board] = rest;
    if (ahead === undefined) continue;
    const row = await db.prepare('SELECT riders FROM board_stats WHERE board = ?1').bind(board).first();
    const riders = Math.max(row?.riders ?? 0, ahead + 1);
    ranks[board] = { ...rest, percentile: Math.max(1, Math.ceil((100 * (ahead + 1)) / riders)), riders, value };
  }
}

// The badge a weekly best earns against a board's cuts: the best band it is
// inside, else 'finished'. The client has the same rule
// (atv_core::medals::weekly_badge).
export function weeklyBadge(value, cuts) {
  for (const band of BANDS) if (cuts[band] !== undefined && value <= cuts[band]) return `top${band}`;
  return 'finished';
}
