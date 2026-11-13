import assert from 'node:assert/strict';
import test from 'node:test';
import worker from './worker.mjs';
import { DAILY_POOL, ENDURO_LAP_FLOOR_FRACTION, FIRST_MOTORCYCLE, FREESTYLE, LAP_FLOOR_FRACTION, NAME, TOP, checkTrace, dailyPick, defaultLaps, eventLapFloorMs, judge, lapFloorMs, parseBoard } from './boards.mjs';
import { HOUSE_ROWS, houseRows } from './house.mjs';
import { DAILY, HOUSE, HOUSE_NAMES, MEASURED_LAP } from './board_seeds.mjs';
import { d1, limiter } from './testing.mjs';

function setup(limits = {}) {
  return {
    SAVES: d1(), GAME_ORIGINS: 'https://furyremastered.com',
    BOARD_READ_LIMIT: limiter(limits.read), BOARD_SUBMIT_LIMIT: limiter(limits.submit),
  };
}
function call(env, method, path, body, headers = {}) {
  const request = new Request(`https://furyremastered.com${path}`, {
    method, headers: { 'CF-Connecting-IP': '203.0.113.9', ...headers }, body: body === undefined ? undefined : typeof body === 'string' ? body : JSON.stringify(body),
  });
  return worker.fetch(request, env).then(async response => ({ status: response.status, body: await response.json(), headers: response.headers }));
}

// A race on a circular course: `laps` laps of `lapSeconds` each, one point a
// second around a loop of `feet`.
function race(laps, lapSeconds, feet = 4000) {
  const radius = feet / (2 * Math.PI), total = laps * lapSeconds, points = [];
  for (let t = 0; t <= total; t++) {
    const angle = (2 * Math.PI * t) / lapSeconds;
    points.push([Math.round(radius * Math.cos(angle)), Math.round(radius * Math.sin(angle))]);
  }
  return { points, marks: Array.from({ length: laps }, (_, i) => (i + 1) * lapSeconds * 1000) };
}
// nat09 is a valid id no course ships: no house riders and the event-type floor,
// so these runs exercise the riders' own rows alone.
const lapRun = (overrides = {}) => ({ name: 'ADA', vehicle: 3, course: 'nat09', lapMs: 60_000, raceMs: 300_000, trace: race(5, 60), ...overrides });
const NOW = Date.parse('2026-11-11T12:00:00Z');

test('board ids name known courses and kinds only', () => {
  assert.deepEqual(parseBoard('lap:nat01:n'), { kind: 'lap', course: 'nat01', expert: false });
  assert.deepEqual(parseBoard('race:env05ends:x'), { kind: 'race', course: 'env05ends', expert: true });
  assert.deepEqual(parseBoard('score:spstunt:free01'), { kind: 'score', series: 'spstunt', course: 'free01' });
  assert.equal(parseBoard('daily:2026-11-11').kind, 'daily');
  for (const bad of ['lap:nat01', 'lap:zzz01:n', 'score:spstunt:env05', 'daily:2026-13-45', 'lap:nat01:n;drop', '']) assert.equal(parseBoard(bad), null, bad);
});

test('the daily pick is a pure function of the UTC date', () => {
  const pick = dailyPick('2026-11-11');
  assert.deepEqual(dailyPick('2026-11-11'), pick);
  assert.ok(DAILY_POOL.includes(pick.course));
  // Shared with atv_core::world_board (its test pins the same dates).
  assert.deepEqual([dailyPick('2026-11-11'), dailyPick('2026-11-12'), dailyPick('2027-01-01')].map(p => `${p.course}/${p.vehicleClass}`), DAILY_EXPECTED);
  const seen = new Set(Array.from({ length: 365 }, (_, i) => dailyPick(new Date(NOW + i * 86_400_000).toISOString().slice(0, 10)).course));
  assert.ok(seen.size > 30, `a year visits most courses (${seen.size})`);
});
const DAILY_EXPECTED = ['env01endl/atv', 'env05/atv', 'env02/atv'];

test('plausibility: floors, traces, names, vehicles and scores', () => {
  assert.equal(judge(lapRun(), NOW).reason, undefined);
  assert.equal(judge(lapRun({ name: 'ada ' }), NOW).name, 'ADA');
  const reason = overrides => judge(lapRun(overrides), NOW).reason;
  assert.equal(reason({ name: '@0' }), 'name');
  assert.equal(reason({ name: 'TOOLONGNAME' }), 'name');
  assert.equal(reason({ name: 'A<b>' }), 'name');
  assert.equal(reason({ vehicle: 99 }), 'vehicle');
  assert.equal(reason({ course: 'mars01' }), 'board');
  // A 10 s lap is below the Nationals floor, whatever the trace says.
  assert.equal(reason({ lapMs: 10_000, raceMs: undefined, trace: race(1, 10, 1000) }), 'lap_floor');
  // The lap must be one of the trace's laps.
  assert.equal(reason({ lapMs: 59_000 }), 'lap_marks');
  // The trace covers the run, one point a second.
  assert.equal(reason({ trace: { ...race(5, 60), points: race(5, 60).points.slice(0, 100) } }), 'trace_length');
  // Teleporting is refused unless the client marks it as a reset.
  const jumped = race(5, 60); jumped.points[30] = [9000, 9000];
  assert.equal(reason({ trace: jumped }), 'trace_speed');
  // 40,000 ft laps in 60 s are 666 ft/s.
  assert.equal(reason({ trace: { ...race(5, 60, 40_000) } }), 'trace_speed');
  // Riding in place does not make a lap.
  assert.equal(reason({ trace: race(5, 60, 100) }), 'lap_short');
  assert.equal(reason({ raceMs: 290_000 }), 'race_marks');
  assert.equal(checkTrace({ points: [[0, 0], [1, 1]], marks: [], resets: [5] }, 1), 'trace');
  // A reset point skips its step.
  const reset = race(5, 60); reset.points[30] = [900, 900]; reset.points[31] = [900, 900];
  assert.equal(reason({ trace: { ...reset, resets: [30, 32] }, lapMs: 60_000 }), undefined);
  const freestyle = { name: 'ADA', vehicle: 3, series: 'spstunt', course: 'free01', score: 50_000, seconds: 120, trace: race(1, 120) };
  assert.deepEqual(judge(freestyle, NOW).entries, [['score:spstunt:free01', -50_000]]);
  assert.equal(judge({ ...freestyle, score: 5_000_000 }, NOW).reason, 'score');
  assert.equal(judge({ ...freestyle, seconds: 600 }, NOW).reason, 'seconds');
});

test('the daily board takes only the day\'s pick, today', () => {
  const pick = dailyPick('2026-11-11');
  const vehicle = pick.vehicleClass === 'atv' ? 3 : 30;
  const laps = /end[sl]$/.test(pick.course) ? 3 : 5;
  // Laps a little slower than the course's fastest measured one.
  const lap = Math.ceil(MEASURED_LAP[pick.course] / 1000) + 5;
  const run = { name: 'ADA', vehicle, course: pick.course, raceMs: laps * lap * 1000, trace: race(laps, lap, lap * 50), daily: '2026-11-11' };
  assert.deepEqual(judge(run, NOW).entries.map(e => e[0]), [`race:${pick.course}:n`, 'daily:2026-11-11']);
  assert.equal(judge({ ...run, daily: '2026-11-10' }, NOW).reason, 'daily_date');
  assert.equal(judge({ ...run, daily: '2026-11-10' }, Date.parse('2026-11-11T00:10:00Z')).reason, 'daily_pick', 'grace for yesterday, but its own pick');
  assert.equal(judge({ ...run, expert: true }, NOW).reason, 'daily_pick');
  assert.equal(judge({ ...run, vehicle: pick.vehicleClass === 'atv' ? 30 : 3 }, NOW).reason, 'daily_pick');
});

test('submit ranks, keeps each name\'s best and reads the cached board', async () => {
  const env = setup();
  let result = await call(env, 'POST', '/api/boards/submit', lapRun());
  assert.equal(result.status, 200);
  assert.deepEqual(result.body.boards['lap:nat09:n'], { rank: 1, placed: true });
  assert.deepEqual(result.body.boards['race:nat09:n'], { rank: 1, placed: true });
  // A slower run by the same name changes nothing.
  result = await call(env, 'POST', '/api/boards/submit', lapRun({ lapMs: 61_000, raceMs: 305_000, trace: race(5, 61) }));
  assert.deepEqual(result.body.boards['lap:nat09:n'], { rank: 1, placed: false });
  result = await call(env, 'POST', '/api/boards/submit', lapRun({ name: 'BOB', lapMs: 55_000, raceMs: 275_000, trace: race(5, 55) }));
  assert.equal(result.body.boards['lap:nat09:n'].rank, 1);
  result = await call(env, 'GET', '/api/boards/lap:nat09:n');
  assert.equal(result.status, 200);
  assert.match(result.headers.get('Cache-Control'), /public, max-age=60/);
  assert.deepEqual(result.body.entries.map(e => [e.rank, e.name, e.value, e.vehicle]), [[1, 'BOB', 55_000, 3], [2, 'ADA', 60_000, 3]]);
  assert.match(result.body.entries[0].day, /^\d{4}-\d{2}-\d{2}$/);
  assert.deepEqual(Object.keys(result.body.entries[0]).sort(), ['day', 'name', 'rank', 'value', 'vehicle']);
  result = await call(env, 'GET', '/api/boards/lap:nat09:x');
  assert.deepEqual(result.body.entries, []);
});

test('a board keeps the top 50', async () => {
  const env = setup();
  for (let i = 0; i < TOP + 5; i++) {
    const lap = 40 + i;
    const result = await call(env, 'POST', '/api/boards/submit', { name: `R${i}`, vehicle: 1, course: 'sx19', lapMs: lap * 1000, trace: race(1, lap) });
    assert.equal(result.status, 200, JSON.stringify(result.body));
  }
  const slow = await call(env, 'POST', '/api/boards/submit', { name: 'SLOW', vehicle: 1, course: 'sx19', lapMs: 200_000, trace: race(1, 200, 8000) });
  assert.deepEqual(slow.body.boards['lap:sx19:n'], { rank: null, placed: false });
  const fast = await call(env, 'POST', '/api/boards/submit', { name: 'FAST', vehicle: 1, course: 'sx19', lapMs: 30_000, trace: race(1, 30) });
  assert.deepEqual(fast.body.boards['lap:sx19:n'], { rank: 1, placed: true });
  const count = env.SAVES.db.prepare("SELECT COUNT(*) AS n FROM board_entries WHERE board = 'lap:sx19:n'").get().n;
  assert.equal(count, TOP);
  const board = await call(env, 'GET', '/api/boards/lap:sx19:n');
  assert.equal(board.body.entries.length, TOP);
  assert.equal(board.body.entries.at(-1).name, 'R48');
});

test('freestyle boards rank the highest score first', async () => {
  const env = setup();
  const run = (name, score) => call(env, 'POST', '/api/boards/submit', { name, vehicle: 2, series: 'unspstunt', course: 'env05', score, seconds: 120, trace: race(1, 120) });
  await run('LOW', 1000);
  // The house riders' scores sit between the two.
  assert.deepEqual((await run('HIGH', 900_000)).body.boards['score:unspstunt:env05'], { rank: 1, placed: true });
  const board = await call(env, 'GET', '/api/boards/score:unspstunt:env05');
  assert.deepEqual(board.body.entries.filter(e => !e.house).map(e => [e.rank, e.name, e.value]), [[1, 'HIGH', 900_000], [HOUSE_ROWS + 2, 'LOW', 1000]]);
  const scores = board.body.entries.map(e => e.value);
  assert.deepEqual(scores, [...scores].sort((a, b) => b - a), 'highest first');
});

test('daily board reads carry the pick and yesterday\'s winner', async () => {
  const env = setup();
  // Quicker than any house rider's race.
  env.SAVES.db.prepare("INSERT INTO board_entries VALUES ('daily:2026-11-10', 'WIN', 23456, 4, '2026-11-10')").run();
  const board = await call(env, 'GET', '/api/boards/daily:2026-11-11');
  assert.equal(board.status, 200);
  assert.deepEqual(board.body.pick, dailyPick('2026-11-11'));
  assert.deepEqual(board.body.yesterday, { date: '2026-11-10', name: 'WIN', value: 23456, vehicle: 4 });
});

test('refusals: method, size, JSON, origin, rate limit and storage failure', async () => {
  const env = setup({ submit: 3 });
  assert.equal((await call(env, 'GET', '/api/boards/submit')).status, 405);
  assert.equal((await call(env, 'POST', '/api/boards/lap:nat01:n')).status, 405);
  assert.equal((await call(env, 'GET', '/api/boards/lap:nowhere:n')).status, 404);
  assert.equal((await call(env, 'POST', '/api/boards/submit', 'x'.repeat(30_000))).status, 413);
  assert.equal((await call(env, 'POST', '/api/boards/submit', '{nope')).status, 400);
  assert.equal((await call(env, 'POST', '/api/boards/submit', lapRun({ lapMs: 1 }))).body.error, 'implausible');
  assert.equal((await call(env, 'POST', '/api/boards/submit', lapRun(), { Origin: 'https://evil.example' })).status, 403);
  const limited = await call(env, 'POST', '/api/boards/submit', lapRun());
  assert.equal(limited.status, 429);
  assert.equal(limited.headers.get('Retry-After'), '60');
  const broken = { ...setup(), SAVES: { prepare() { throw new Error('down'); }, batch() { throw new Error('down'); } } };
  assert.equal((await call(broken, 'GET', '/api/boards/lap:nat01:n')).status, 503);
  assert.equal((await call({ ...setup(), BOARD_READ_LIMIT: undefined }, 'GET', '/api/boards/lap:nat01:n')).status, 503);
});

// --- House riders (house.mjs, board_seeds.mjs) ---

// A race of `laps` laps of `lapMs` each on a loop ridden at 60 ft/s.
function ride(laps, lapMs) {
  const feet = (lapMs / 1000) * 60, radius = feet / (2 * Math.PI), points = [];
  for (let t = 0; t <= Math.floor((laps * lapMs) / 1000); t++) {
    const angle = (2 * Math.PI * t * 1000) / lapMs;
    points.push([Math.round(radius * Math.cos(angle)), Math.round(radius * Math.sin(angle))]);
  }
  return { points, marks: Array.from({ length: laps }, (_, i) => (i + 1) * lapMs) };
}
const raceBoards = () => Object.keys(MEASURED_LAP).flatMap(course => ['lap', 'race'].flatMap(kind => ['n', 'x'].map(tag => `${kind}:${course}:${tag}`)));
const scoreBoards = () => Object.entries(FREESTYLE).flatMap(([series, courses]) => courses.map(course => `score:${series}:${course}`));

test('every board has ten house riders with rider names, irregular times and fitting vehicles', () => {
  // Every course the game races was measured (51: 8 venues with National,
  // long and short Enduro, 3 Short Tracks, 8 Pro Nationals, 16 Supercross).
  assert.equal(Object.keys(MEASURED_LAP).length, 51);
  for (const course of DAILY_POOL) assert.ok(DAILY[course], `daily course ${course}`);
  const leaders = new Set();
  for (const board of [...raceBoards(), ...scoreBoards()]) {
    assert.ok(parseBoard(board), board);
    const rows = houseRows(board);
    assert.equal(rows.length, HOUSE_ROWS, board);
    for (const row of rows) {
      assert.match(row.name, NAME, board);
      assert.ok(row.house && row.vehicle >= 1 && row.vehicle < FIRST_MOTORCYCLE && Number.isSafeInteger(row.value), `${board} ${JSON.stringify(row)}`);
    }
    assert.equal(new Set(rows.map(row => row.name)).size, HOUSE_ROWS, `${board}: one row a name`);
    const values = rows.map(row => row.value);
    assert.deepEqual(values, [...values].sort((a, b) => a - b), `${board}: best first`);
    const gaps = values.slice(1).map((value, i) => value - values[i]);
    assert.ok(gaps.every(gap => gap > 0) && new Set(gaps).size >= HOUSE_ROWS - 2, `${board}: uneven gaps ${gaps}`);
    // Times are not round: at most one row on a whole second.
    if (!board.startsWith('score:')) assert.ok(values.filter(value => value % 1000 === 0).length <= 1, `${board}: ${values}`);
    leaders.add(rows[0].name);
  }
  assert.ok(leaders.size >= HOUSE_NAMES.length / 2, `the leaders vary by board (${leaders.size} names lead)`);
  assert.equal(Object.keys(HOUSE).length, raceBoards().length + scoreBoards().length);
});

test('house laps clear the floors and house races agree with the house laps', () => {
  for (const course of Object.keys(MEASURED_LAP)) {
    const laps = defaultLaps(course);
    assert.equal(lapFloorMs(course), Math.floor(MEASURED_LAP[course] * (/end[sl]$/.test(course) ? ENDURO_LAP_FLOOR_FRACTION : LAP_FLOOR_FRACTION)));
    for (const tag of ['n', 'x']) {
      const lap = houseRows(`lap:${course}:${tag}`), race = houseRows(`race:${course}:${tag}`);
      // The leader is a little quicker than the fastest measured lap, never
      // near the floor.
      assert.ok(lap[0].value < MEASURED_LAP[course] && lap[0].value > 0.94 * MEASURED_LAP[course], `${course}:${tag} ${lap[0].value} against ${MEASURED_LAP[course]}`);
      // A race is no quicker than its laps: laps x the best lap, less only
      // where the grid stands ahead of the line (at most 3 %).
      assert.ok(race[0].value >= 0.97 * laps * lap[0].value, `${course}:${tag} race ${race[0].value} against ${laps} x ${lap[0].value}`);
      for (const row of race) {
        const own = lap.find(other => other.name === row.name);
        if (own) assert.ok(row.value >= 0.97 * laps * own.value, `${course}:${tag} ${row.name}`);
        if (own) assert.equal(row.vehicle, own.vehicle, `${course}:${tag} ${row.name} rides one vehicle`);
      }
      // A house rider's own run would be accepted.
      const run = { name: lap[0].name, vehicle: lap[0].vehicle, course, expert: tag === 'x', lapMs: lap[0].value, trace: ride(2, lap[0].value) };
      assert.equal(judge(run, NOW).reason, undefined, `${course}:${tag}`);
    }
    // Expert's house leader is the quicker one.
    assert.ok(houseRows(`lap:${course}:x`)[0].value < houseRows(`lap:${course}:n`)[0].value, course);
  }
});

test('lap floors: half the fastest measured lap (0.4 for Enduro), the event-type value elsewhere', () => {
  // The old event-type floors refused laps the game's own CPU riders set.
  assert.ok(MEASURED_LAP.env05 < eventLapFloorMs('env05') && MEASURED_LAP.env01st < eventLapFloorMs('env01st'));
  assert.equal(lapFloorMs('nat09'), 25_000);
  assert.equal(lapFloorMs('sx19'), 15_000);
  const lap = fraction => { const lapMs = Math.round(MEASURED_LAP.sx01 * fraction); return judge({ name: 'ADA', vehicle: 3, course: 'sx01', lapMs, trace: ride(2, lapMs) }, NOW).reason; };
  assert.equal(lap(0.9), undefined);
  assert.equal(lap(0.55), undefined);
  assert.equal(lap(0.45), 'lap_floor');
  const enduro = fraction => { const lapMs = Math.round(MEASURED_LAP.env05ends * fraction); return judge({ name: 'ADA', vehicle: 3, course: 'env05ends', lapMs, trace: ride(2, lapMs) }, NOW).reason; };
  assert.equal(enduro(0.45), undefined);
  assert.equal(enduro(0.35), 'lap_floor');
});

test('a served board merges the house riders without reading more rows', async () => {
  const env = setup();
  const empty = await call(env, 'GET', '/api/boards/lap:sx01:n');
  assert.equal(env.SAVES.prepared, 1, 'one D1 query a board read');
  assert.equal(empty.body.entries.length, HOUSE_ROWS);
  assert.deepEqual(empty.body.entries.map(e => e.rank), Array.from({ length: HOUSE_ROWS }, (_, i) => i + 1));
  assert.ok(empty.body.entries.every(e => e.house === true && /^\d{4}-\d{2}-\d{2}$/.test(e.day)));
  assert.equal(env.SAVES.db.prepare('SELECT COUNT(*) AS n FROM board_entries').get().n, 0, 'house riders are never stored');
  const house = houseRows('lap:sx01:n');
  // Slower than every house rider: 11th. Between the third and fourth: 4th.
  const submit = (name, lapMs) => call(env, 'POST', '/api/boards/submit', { name, vehicle: 7, course: 'sx01', lapMs, trace: ride(2, lapMs) });
  assert.deepEqual((await submit('SLOW', house.at(-1).value + 5000)).body.boards['lap:sx01:n'], { rank: HOUSE_ROWS + 1, placed: true });
  const fourth = Math.round((house[2].value + house[3].value) / 2);
  assert.deepEqual((await submit('ADA', fourth)).body.boards['lap:sx01:n'], { rank: 4, placed: true });
  // A rider may use a house rider's name: both rows stay.
  assert.deepEqual((await submit(house[0].name, house[0].value - 1)).body.boards['lap:sx01:n'], { rank: 1, placed: true });
  // A slower run by ADA keeps the 5th place it now holds.
  assert.deepEqual((await submit('ADA', fourth + 20_000)).body.boards['lap:sx01:n'], { rank: 5, placed: false });
  const board = await call(env, 'GET', '/api/boards/lap:sx01:n');
  assert.equal(board.body.entries.length, HOUSE_ROWS + 3);
  assert.deepEqual(board.body.entries.filter(e => !e.house).map(e => [e.rank, e.name]), [[1, house[0].name], [5, 'ADA'], [HOUSE_ROWS + 3, 'SLOW']]);
  assert.deepEqual(Object.keys(board.body.entries[0]).sort(), ['day', 'name', 'rank', 'value', 'vehicle']);
  assert.deepEqual(Object.keys(board.body.entries[1]).sort(), ['day', 'house', 'name', 'rank', 'value', 'vehicle']);
  assert.equal(board.body.entries[1].name, house[0].name);
  assert.equal(env.SAVES.db.prepare('SELECT COUNT(*) AS n FROM board_entries').get().n, 3);
});

test('the house riders count toward the top 50', async () => {
  const env = setup();
  const slowest = houseRows('lap:sx02:n').at(-1).value;
  const submit = i => call(env, 'POST', '/api/boards/submit', { name: `R${i}`, vehicle: 1, course: 'sx02', lapMs: slowest + (i + 1) * 1000, trace: ride(2, slowest + (i + 1) * 1000) });
  for (let i = 0; i < TOP - HOUSE_ROWS; i++) assert.deepEqual((await submit(i)).body.boards['lap:sx02:n'], { rank: HOUSE_ROWS + i + 1, placed: true });
  assert.deepEqual((await submit(TOP)).body.boards['lap:sx02:n'], { rank: null, placed: false });
  const board = await call(env, 'GET', '/api/boards/lap:sx02:n');
  assert.equal(board.body.entries.length, TOP);
  assert.equal(board.body.entries.at(-1).name, `R${TOP - HOUSE_ROWS - 1}`);
  // A quicker rider still enters, and the board stays at 50.
  const fast = await call(env, 'POST', '/api/boards/submit', { name: 'FAST', vehicle: 1, course: 'sx02', lapMs: slowest - 1, trace: ride(2, slowest - 1) });
  assert.deepEqual(fast.body.boards['lap:sx02:n'], { rank: HOUSE_ROWS, placed: true });
  assert.equal((await call(env, 'GET', '/api/boards/lap:sx02:n')).body.entries.length, TOP);
});

test('daily boards get house riders from their date', async () => {
  const names = new Set(), courses = new Set();
  for (let i = 0; i < 365; i++) {
    const date = new Date(NOW + i * 86_400_000).toISOString().slice(0, 10), pick = dailyPick(date);
    const rows = houseRows(`daily:${date}`, pick);
    assert.equal(rows.length, HOUSE_ROWS, date);
    assert.deepEqual(houseRows(`daily:${date}`, pick), rows, 'a pure function of the date');
    const values = rows.map(row => row.value);
    assert.deepEqual(values, [...values].sort((a, b) => a - b));
    assert.equal(new Set(rows.map(row => row.name)).size, HOUSE_ROWS);
    for (const row of rows) {
      assert.equal(row.vehicle >= FIRST_MOTORCYCLE, pick.vehicleClass === 'motorcycle', `${date} ${row.vehicle}`);
      assert.equal(row.day, date);
    }
    // The day's leader is within 1.5 % of the course's fastest measured race.
    const [race] = DAILY[pick.course];
    assert.ok(Math.abs(values[0] / race - 1) <= 0.0151, `${date} ${values[0]} against ${race}`);
    // A full race at the house leader's time is a valid daily run.
    const laps = defaultLaps(pick.course);
    if (i < 40) {
      const run = { name: 'ADA', vehicle: rows[0].vehicle, course: pick.course, raceMs: values[0], trace: ride(laps, Math.round(values[0] / laps)), daily: date };
      run.trace.marks[laps - 1] = values[0];
      assert.equal(judge(run, NOW + i * 86_400_000).reason, undefined, date);
    }
    names.add(rows[0].name); courses.add(pick.course);
  }
  assert.ok(names.size > 20, `daily leaders vary (${names.size})`);
  const env = setup();
  const board = await call(env, 'GET', '/api/boards/daily:2026-11-11');
  assert.equal(env.SAVES.prepared, 2, 'the board and yesterday\'s winner, as before');
  assert.deepEqual(board.body.entries.map(e => [e.name, e.value]), houseRows('daily:2026-11-11', dailyPick('2026-11-11')).map(row => [row.name, row.value]));
  // With no rider on yesterday's board its house leader won.
  const [leader] = houseRows('daily:2026-11-10', dailyPick('2026-11-10'));
  assert.deepEqual(board.body.yesterday, { date: '2026-11-10', name: leader.name, value: leader.value, vehicle: leader.vehicle, house: true });
  // Today's race on the pick ranks among the day's house riders (the Worker
  // reads the real clock).
  const today = new Date().toISOString().slice(0, 10), pick = dailyPick(today), laps = defaultLaps(pick.course);
  const house = houseRows(`daily:${today}`, pick);
  const raceMs = house[4].value + 1;
  const trace = ride(laps, Math.round(raceMs / laps)); trace.marks[laps - 1] = raceMs;
  const vehicle = pick.vehicleClass === 'motorcycle' ? 30 : 3;
  const result = await call(env, 'POST', '/api/boards/submit', { name: 'ADA', vehicle, course: pick.course, raceMs, trace, daily: today });
  assert.deepEqual(result.body.boards[`daily:${today}`], { rank: 6, placed: true }, JSON.stringify(result.body));
  const mine = (await call(env, 'GET', `/api/boards/daily:${today}`)).body.entries[5];
  assert.deepEqual([mine.rank, mine.name, mine.value, mine.house], [6, 'ADA', raceMs, undefined]);
});
