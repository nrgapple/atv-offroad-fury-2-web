// Builds cloudflare/board_seeds.mjs, the World boards' house riders and
// measured lap times (research/WORLD_BOARDS.md "House riders"), from races the
// game's own race path driver rode on every course:
//
//   cargo run --release -p atv-game --example cpu_times -- CONTENT_DIR OUT.jsonl ...
//   node tools/build_board_seeds.mjs [--from DIR] [--report FILE]
//   node tools/build_board_seeds.mjs --verify [--from DIR]
//
// DIR (default private/reports/world-boards/cpu-times) holds the example's
// .jsonl files; they stay private. The table is a pure function of them and
// of the constants here: rerunning without new measurements changes nothing.
// --report writes each course's anchors as JSON for the research note.
// --verify builds nothing: it hands every measured run the driver rode for the
// player to the server's plausibility checks (as the game would submit it:
// best flying lap, race time and trace) and lists any the built table refuses.
import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { DAILY_POOL, FREESTYLE, defaultLaps, judge, parseBoard } from '../cloudflare/boards.mjs';
import { HOUSE_ROWS, hashText, spread, stream } from '../cloudflare/house.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const args = process.argv.slice(2);
const option = name => { const at = args.indexOf(name); return at < 0 ? undefined : args[at + 1]; };
const from = path.resolve(root, option('--from') ?? 'private/reports/world-boards/cpu-times');

// Authored rider names (not people, and not the original's CPU roster, whose
// names are the content's): each with the discipline it rides best.
const RIDERS = [
  ['RIGGS', 'sx'], ['DAKOTA', 'nat'], ['TALON', 'sx'], ['BRIX', 'enduro'], ['MOJAVE', 'enduro'], ['KESTREL', 'nat'],
  ['ROOK', 'sx'], ['SLADE', 'nat'], ['JINX', 'sx'], ['ZEKE', 'enduro'], ['WREN', 'nat'], ['BODIE', 'enduro'],
  ['FALCO', 'sx'], ['GRITS', 'enduro'], ['LOBO', 'nat'], ['MOSS', 'enduro'], ['NITRO', 'sx'], ['PIKE', 'nat'],
  ['QUILL', 'free'], ['RUSTY', 'enduro'], ['STITCH', 'free'], ['TANK', 'nat'], ['VEGA', 'sx'], ['WHIP', 'free'],
  ['YETI', 'enduro'], ['AXEL', 'sx'], ['BLAZE', 'free'], ['DUSTY', 'nat'], ['ECHO', 'free'], ['FLINT', 'nat'],
  ['KNOX', 'sx'], ['RENO', 'free'], ['SABLE', 'enduro'], ['T-BONE', 'free'],
];
// The day the house riders went up (their rows' `day`).
const HOUSE_DAY = '2026-11-12';
// Riders drawn per course and difficulty: the lap and race boards each show
// the quickest HOUSE_ROWS of them, so the two boards differ a little.
const FIELD = HOUSE_ROWS + 2;
// A house race is at least this much over laps x the rider's best lap.
const RACE_OVER_LAPS = 1.002;
// The top "Score Stunt Points" level of each Freestyle course's objectives
// (atv_core::freestyle::course_objectives).
const TOP_LEVEL = {
  env01: 50_000, env02: 50_000, env04: 50_000, env05: 50_000, env06: 50_000, env08: 50_000,
  free01: 50_000, free02: 50_000, free03: 50_000, free04: 100_000, free06: 100_000,
};

const discipline = course => (/end[sl]$/.test(course) ? 'enduro' : course.startsWith('sx') || /st$/.test(course) ? 'sx' : 'nat');
const ms = seconds => Math.round(seconds * 1000);

// Every measured race, by course.
const runs = new Map();
for (const file of readdirSync(from).filter(name => name.endsWith('.jsonl')).sort()) {
  for (const line of readFileSync(path.join(from, file), 'utf8').split('\n').filter(Boolean)) {
    const run = JSON.parse(line);
    if (!runs.has(run.course)) runs.set(run.course, []);
    runs.get(run.course).push(run);
  }
}

// A rider's flying laps (every lap after the one from the grid), seconds.
const flying = rider => rider.marks.slice(1).map((mark, i) => mark - rider.marks[i]);
const least = values => (values.length ? Math.min(...values) : undefined);
// A full race the rider really rode: every lap crossed (no race-over
// estimate) and a first lap that is a lap. On env01st the first roster CPU
// is credited a lap 0.13 s after GO, so it rides one lap fewer (issue #584);
// those races are left out.
const rode = (rider, laps) => !rider.estimated && rider.marks.length === laps && rider.marks[0] >= 0.6 * least(flying(rider));

// One course's measurements, seconds: the fastest flying lap and full-length
// race of the CPU riders on each difficulty and of the driver on the
// player's vehicle; the course's anchors, the fastest of them all (the
// difficulty changes only the CPUs' leader limit, not the course or the
// vehicles, so Normal and Expert share them); the least a race took over
// laps x its rider's best lap; and the quads the CPU roster rode there.
function anchors(course) {
  const list = runs.get(course) ?? [];
  const laps = defaultLaps(course);
  const full = list.filter(run => run.laps === laps);
  const riders = (difficulty, player) => full.filter(run => player ? run.autopilot : run.difficulty === difficulty).flatMap(run => run.riders.filter(rider => rider.player === player));
  const of = group => ({ lap: least(group.flatMap(flying)), race: least(group.filter(rider => rode(rider, laps)).map(rider => rider.time)) });
  const cpu = { n: of(riders(1, false)), x: of(riders(2, false)) }, driver = of(riders(0, true));
  const everyone = full.flatMap(run => run.riders);
  return {
    laps, cpu, driver,
    lap: least(everyone.flatMap(flying)),
    race: least(everyone.filter(rider => rode(rider, laps)).map(rider => rider.time)),
    over: least(everyone.filter(rider => rode(rider, laps)).map(rider => rider.time / (laps * least(flying(rider))))),
    quads: [...new Set(full.flatMap(run => run.roster.map(entry => entry.quad)))].sort((a, b) => a - b),
  };
}

if (args.includes('--verify')) {
  let accepted = 0;
  const refused = [];
  for (const run of [...runs.values()].flat()) {
    const player = run.riders.find(rider => rider.player);
    if (!run.trace || !run.autopilot || run.laps !== defaultLaps(run.course) || player.marks.length !== run.laps) continue;
    const marks = run.trace.marks;
    const lapMs = Math.min(...marks.slice(1).map((mark, i) => mark - marks[i]));
    const body = { name: 'DRIVER', vehicle: run.quad, course: run.course, expert: run.difficulty === 2, lapMs, raceMs: marks.at(-1), trace: run.trace };
    const { reason } = judge(body, Date.now());
    if (reason) refused.push(`${run.course} d${run.difficulty} quad ${run.quad}: ${reason}`); else accepted += 1;
  }
  console.log(`${accepted} measured runs accepted, ${refused.length} refused${refused.length ? `:\n${refused.join('\n')}` : ''}`);
  process.exit(refused.length ? 1 : 0);
}

// A rider's standing on a board: a fixed skill, a bonus on its own
// discipline and the board's own draw, so the leaders change from course
// to course without being random.
function standing(name, home, board) {
  const skill = stream(hashText(`skill:${name}`))();
  const form = stream(hashText(`form:${name}:${board}`))();
  return 0.45 * skill + (home ? 0.2 : 0) + 0.7 * form;
}

const boards = {}, measured = {}, daily = {}, report = {};
const row = (name, value, vehicle) => [RIDERS.findIndex(rider => rider[0] === name), value, vehicle];
const courses = [...runs.keys()].filter(course => parseBoard(`lap:${course}:n`)).sort();
for (const course of courses) {
  const a = anchors(course);
  if (!a.lap) continue;
  measured[course] = ms(a.lap);
  report[course] = a;
  // Where the grid stands ahead of the line a race takes less than laps x
  // the best lap; never ask more of a house rider than the measured least.
  const over = Math.max(0.97, Math.min(1, a.over ?? 1)) * RACE_OVER_LAPS;
  const vehicleOf = name => { const quads = a.quads.length ? a.quads : [1]; return quads[hashText(`quad:${name}:${course}`) % quads.length]; };
  for (const tag of ['n', 'x']) {
    const random = stream(hashText(`house:${course}:${tag}`));
    const field = RIDERS.map(([name, kind]) => {
      const lapRank = standing(name, kind === discipline(course), `${course}:${tag}`);
      return { name, lapRank, raceRank: lapRank + 0.25 * stream(hashText(`race:${name}:${course}:${tag}`))() };
    });
    const byLap = [...field].sort((x, y) => y.lapRank - x.lapRank).slice(0, FIELD);
    // The quickest house time is a little under the fastest measured one:
    // 1-2.2 % on Normal, 2.5-4 % on Expert (the quicker field is authored).
    const margin = () => (tag === 'x' ? 0.96 + random() * 0.015 : 0.978 + random() * 0.012);
    const lapTimes = spread(random, ms(a.lap) * margin(), FIELD);
    byLap.forEach((rider, i) => { rider.lap = lapTimes[i]; });
    boards[`lap:${course}:${tag}`] = byLap.slice(0, HOUSE_ROWS).map(rider => row(rider.name, rider.lap, vehicleOf(rider.name)));
    if (!a.race) continue;
    const raceTimes = spread(random, Math.max(ms(a.race) * margin(), a.laps * lapTimes[0] * over), FIELD);
    const byRace = [...byLap].sort((x, y) => y.raceRank - x.raceRank);
    byRace.forEach((rider, i) => {
      // No rider's race is quicker than its own best lap allows.
      rider.race = Math.max(raceTimes[i], Math.ceil(a.laps * rider.lap * (over + random() * 0.01)));
    });
    byRace.sort((x, y) => x.race - y.race);
    boards[`race:${course}:${tag}`] = byRace.slice(0, HOUSE_ROWS).map(rider => row(rider.name, rider.race, vehicleOf(rider.name)));
  }
  if (DAILY_POOL.includes(course) && a.race) daily[course] = [ms(a.race), a.quads.length ? a.quads : [1]];
}

// Freestyle: objective scores from short of the course's top Score Stunt
// Points level (a run that completes the objective ends just past it, plus
// 100 a second left) to about one and a half times it (a long combo or
// target run), on quads the CPU rosters ride.
const everyQuad = [...new Set(courses.flatMap(course => report[course]?.quads ?? []))].sort((a, b) => a - b);
for (const [series, list] of Object.entries(FREESTYLE)) {
  for (const course of list) {
    const random = stream(hashText(`house:score:${series}:${course}`));
    let score = TOP_LEVEL[course] * (1.38 + random() * 0.24);
    const quads = report[course]?.quads.length ? report[course].quads : everyQuad.length ? everyQuad : [1];
    const names = RIDERS.map(([name, kind]) => ({ name, rank: standing(name, kind === 'free', `${series}:${course}`) }))
      .sort((x, y) => y.rank - x.rank).slice(0, HOUSE_ROWS);
    boards[`score:${series}:${course}`] = names.map(({ name }) => {
      const value = Math.round(score / 5) * 5;
      score *= 1 - (0.025 + random() ** 1.3 * 0.11);
      return row(name, value, quads[hashText(`quad:${name}:${course}`) % quads.length]);
    });
  }
}

const lines = [
  '// Generated by tools/build_board_seeds.mjs from measured races of the game\'s race',
  '// path driver (research/WORLD_BOARDS.md "House riders"). Do not edit by hand:',
  '// change the generator or the measurements and rebuild.',
  '',
  '// The house riders\' names.',
  `export const HOUSE_NAMES = ${JSON.stringify(RIDERS.map(rider => rider[0]))};`,
  '// The `day` of a lap, race or score board\'s house rows.',
  `export const HOUSE_DAY = '${HOUSE_DAY}';`,
  '',
  '// Each measured course\'s fastest lap by the path driver (any CPU rider or the',
  '// driver on the player\'s vehicle, either difficulty), milliseconds: the lap',
  '// floors\' basis.',
  'export const MEASURED_LAP = {',
  ...Object.entries(measured).map(([course, value]) => `  ${course}: ${value},`),
  '};',
  '',
  '// The daily event\'s courses: the fastest measured full-length race,',
  '// milliseconds, and the quads the CPU roster rides there.',
  'export const DAILY = {',
  ...Object.entries(daily).map(([course, value]) => `  ${course}: ${JSON.stringify(value)},`),
  '};',
  '',
  '// House rows per board, best first: [name index, milliseconds or score, vehicle].',
  'export const HOUSE = {',
  ...Object.entries(boards).map(([board, rows]) => `  '${board}': ${JSON.stringify(rows)},`),
  '};',
  '',
];
writeFileSync(path.join(root, 'cloudflare/board_seeds.mjs'), lines.join('\n'));
if (option('--report')) writeFileSync(path.resolve(root, option('--report')), JSON.stringify(report, null, 1));
console.log(`${Object.keys(boards).length} boards, ${Object.keys(measured).length} measured courses, ${Object.keys(daily).length} daily courses`);
