// House riders (research/WORLD_BOARDS.md, "House riders"): authored rows every
// World board carries beside the riders' own, so no board is empty and a new
// rider has times to chase. They live in this Worker only: merged into a board
// when it is served and counted when a submission is ranked, never written to
// D1. Lap, race and score boards read the generated table (board_seeds.mjs,
// built by tools/build_board_seeds.mjs from measured races); a daily board's
// rows are computed from its date.
import { DAILY, HOUSE, HOUSE_DAY, HOUSE_NAMES } from './board_seeds.mjs';

// Rows a board's house riders fill.
export const HOUSE_ROWS = 10;
// The motorcycles a motorcycle day's house riders ride: the 125, 250 and 500
// classes (atv_core::motorcycle::definition), not the 50 cc minis.
const MOTORCYCLES = [29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44];

// FNV-1a of a seed text.
export function hashText(text) {
  let hash = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) hash = Math.imul(hash ^ text.charCodeAt(i), 0x01000193);
  return hash >>> 0;
}

// A repeatable stream of numbers in [0, 1) (mulberry32).
export function stream(seed) {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

// `count` values from `best` down a board: every gap is at least 0.2 %, most
// are small, a few are large, and they widen toward the bottom (about 9 %
// from first to tenth), as times on a real board do.
export function spread(random, best, count) {
  const values = [];
  let value = best;
  for (let i = 0; i < count; i++) {
    values.push(Math.round(value));
    value *= 1.002 + random() ** 1.6 * (0.012 + 0.0025 * i);
  }
  return values;
}

// `count` different items of `list`, in a drawn order.
export function draw(random, list, count) {
  const pool = [...list], out = [];
  while (out.length < count && pool.length) out.push(pool.splice(Math.floor(random() * pool.length), 1)[0]);
  return out;
}

// A daily board's house riders: the day's leader within 1.5 % of the course's
// fastest measured race (quicker on some days, slower on others), the rest
// spread behind, on vehicles of the day's class.
function daily(pick) {
  const course = DAILY[pick.course];
  if (!course) return [];
  const [race, quads] = course;
  const random = stream(hashText(`daily:${pick.date}`));
  const values = spread(random, race * (0.985 + random() * 0.03), HOUSE_ROWS);
  const names = draw(random, HOUSE_NAMES, HOUSE_ROWS);
  const vehicles = pick.vehicleClass === 'motorcycle' ? MOTORCYCLES : quads;
  return values.map((value, i) => ({ name: names[i], value, vehicle: vehicles[Math.floor(random() * vehicles.length)] }));
}

// The board's house rows, best first, as D1 keeps rows (`value` lower is
// better: milliseconds, or the negated score). `pick` is a daily board's
// event.
export function houseRows(board, pick) {
  const score = board.startsWith('score:');
  const rows = pick ? daily(pick) : (HOUSE[board] ?? []).map(([name, value, vehicle]) => ({ name: HOUSE_NAMES[name], value: score ? -value : value, vehicle }));
  return rows.map(row => ({ ...row, day: pick ? pick.date : HOUSE_DAY, house: true }));
}

// Riders' rows and house rows as one board, best first. A rider's row ranks
// ahead of a house row with the same value.
export function merge(rows, house, top) {
  return [...rows, ...house]
    .sort((a, b) => a.value - b.value || Number(Boolean(a.house)) - Number(Boolean(b.house)) || (a.name < b.name ? -1 : a.name > b.name ? 1 : 0))
    .slice(0, top);
}
