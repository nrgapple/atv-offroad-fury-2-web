// A finished race reaches the World Hall of Fame end to end (research/
// WORLD_BOARDS.md): Single Player → Daily Event → Start Daily Event rides the
// day's pick with the race path driver (`autopilot=1`) to a real five-lap
// finish, the name entry names its records, the game sends its one
// submission, the results show the World line, and the Daily Event page and
// Main Menu → Hall of Fame → View: World show the rider's row among the house
// riders.
//
// Nothing is mocked and production is never touched: the game's /api/boards
// requests are handed to the Worker itself (cloudflare/worker.mjs, run in
// this process) over a local SQLite D1 with the production migrations
// (cloudflare/testing.mjs). The clock is fixed on 2026-11-12 (the page's and
// this process's, by one constant offset), a day whose pick is env05's National
// on an ATV: the shortest full-length race in the daily pool (about two minutes).
import assert from 'node:assert/strict';
import { assertNoErrors, base, launch, newPage, nextFrames, unlockStore, url } from './browser/harness.mjs';

const NOW = Date.parse('2026-11-12T12:00:00Z');
const offset = NOW - Date.now();
const realNow = Date.now.bind(Date);
Date.now = () => realNow() + offset;

const { default: worker } = await import('../cloudflare/worker.mjs');
const { dailyPick, defaultLaps } = await import('../cloudflare/boards.mjs');
const { HOUSE_ROWS, houseRows } = await import('../cloudflare/house.mjs');
const { d1, limiter } = await import('../cloudflare/testing.mjs');

const DATE = '2026-11-12';
const pick = dailyPick(DATE);
assert.deepEqual([pick.course, pick.vehicleClass], ['env05', 'atv'], 'the fixed day\'s pick');
const BOARDS = { lap: `lap:${pick.course}:n`, race: `race:${pick.course}:n`, daily: `daily:${DATE}` };
const house = board => houseRows(board, board.startsWith('daily:') ? pick : undefined);

// The local Worker: its real fetch handler, a local D1 and open limiters.
const env = { SAVES: d1(), GAME_ORIGINS: new URL(base).origin, BOARD_READ_LIMIT: limiter(), BOARD_SUBMIT_LIMIT: limiter() };
const calls = [];
async function api(route) {
  const request = route.request();
  const headers = { 'CF-Connecting-IP': '127.0.0.1' };
  for (const name of ['origin', 'content-type', 'sec-fetch-site']) if (request.headers()[name]) headers[name] = request.headers()[name];
  const response = await worker.fetch(new Request(request.url(), { method: request.method(), headers, body: request.postData() ?? undefined }), env);
  const body = await response.text();
  calls.push({ method: request.method(), path: decodeURIComponent(new URL(request.url()).pathname), sent: request.postData(), status: response.status, body });
  await route.fulfill({ status: response.status, headers: { 'content-type': response.headers.get('Content-Type') ?? 'application/json' }, body });
}

const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const front = async page => (await read(page)).front || {};
const onPage = (page, name, timeout = 90000) =>
  page.waitForFunction(name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name, name, { timeout });
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }
async function choose(page, action) {
  const f = await front(page);
  const at = f.choices.indexOf(action);
  assert.ok(at >= 0, `${action} in ${f.page}: ${f.choices}`);
  for (let i = f.selection; i < at; i++) await key(page, 'ArrowDown');
  for (let i = f.selection; i > at; i--) await key(page, 'ArrowUp');
  await key(page, 'Enter');
}
async function until(page, predicate, timeout, what) {
  const deadline = realNow() + timeout;
  for (;;) {
    const s = await read(page);
    if (predicate(s)) return s;
    if (realNow() > deadline) throw new Error(`timed out: ${what}; race ${JSON.stringify(s.ride?.raceLap ?? null)}; calls ${calls.map(c => `${c.method} ${c.path} ${c.status}`).join(', ')}`);
    await page.waitForTimeout(250);
  }
}
const details = async page => ((await front(page)).profileBook?.details || []).map(([a, b]) => `${a}|${b}`);
async function waitDetail(page, pattern) {
  try {
    await page.waitForFunction(source => {
      const book = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.profileBook;
      return book?.details?.some(([a, b]) => new RegExp(source).test(`${a}|${b}`));
    }, pattern.source, { timeout: 30000 });
  } catch {
    throw new Error(`${pattern} not shown; calls ${calls.map(c => `${c.method} ${c.path} ${c.status}`).join(', ')}; details:\n${(await details(page)).join('\n')}`);
  }
}
// "01:49.61" as the boards print milliseconds.
const clock = ms => { const c = Math.floor(ms / 10); return `${String(Math.floor(c / 6000)).padStart(2, '0')}:${String(Math.floor(c / 100) % 60).padStart(2, '0')}.${String(c % 100).padStart(2, '0')}`; };
const escaped = text => text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

const browser = await launch();
try {
  const context = await browser.newContext({ viewport: { width: 1280, height: 720 } });
  await context.route('**/api/boards/**', api);
  const errors = [];
  const page = await newPage(context, {}, errors);
  await unlockStore(page);
  await page.addInitScript(offset => {
    const real = Date.now.bind(Date);
    Date.now = () => real() + offset;
    if (sessionStorage.getItem('world-submit-test')) return;
    sessionStorage.setItem('world-submit-test', '1');
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.removeItem('fury-hall-of-fame');
    localStorage.removeItem('atv-player-selection');
  }, offset);
  await page.goto(url('/play.html?uiDiagnostics=1&autopilot=1&weather=off'));
  await onPage(page, 'title');
  await page.mouse.click(8, 8);
  await key(page, 'Enter'); await onPage(page, 'main');

  // The Daily Event page: today's board is the house riders alone.
  await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.choices?.includes('single'), null, { timeout: 60000 });
  await choose(page, 'single'); await onPage(page, 'single');
  await choose(page, 'daily'); await onPage(page, 'daily');
  const leader = house(BOARDS.daily)[0];
  await waitDetail(page, new RegExp(`^1\\. ${escaped(leader.name)} · .*\\|${escaped(clock(leader.value))}$`));
  let lines = await details(page);
  assert.equal(lines[0], `Daily Event · ${DATE}|UTC`);
  assert.equal(lines.filter(l => /^\d+\. /.test(l)).length, HOUSE_ROWS, lines.join('\n'));
  assert.ok(!lines.some(l => l.startsWith('▶')), 'no row is the rider\'s yet');
  assert.ok(lines.some(l => /^Yesterday's Winner:\|\S+ · \d\d:\d\d\.\d\d$/.test(l)), lines.join('\n'));

  // Ride the event to its finish.
  await choose(page, 'dailystart');
  let s = await until(page, s => s.ride?.raceLap?.phase === 'racing', 240000, 'the race starts');
  assert.equal(s.ride.raceLap.laps, defaultLaps(pick.course));
  s = await until(page, s => s.ride?.raceLap?.phase === 'finished' && s.ride?.fame, 420000, 'the finish and the name entry');
  assert.ok(s.ride.fame.broken.some(record => /Fastest Lap Time/.test(record)) && s.ride.fame.broken.some(record => /Fastest 5 Lap Race Time/.test(record)), JSON.stringify(s.ride.fame));
  assert.equal(calls.filter(c => c.method === 'POST').length, 0, 'nothing is sent before the name is entered');
  for (let i = 0; i < 8; i++) await key(page, 'Backspace');
  for (const code of ['KeyA', 'KeyC', 'KeyE']) await key(page, code);
  await key(page, 'Enter');
  s = await until(page, s => /^World #/.test(s.ride?.world ?? ''), 30000, 'the World line on the results');

  // One accepted submission, ranked among the house riders.
  const posts = calls.filter(c => c.method === 'POST');
  assert.equal(posts.length, 1, 'one submission');
  assert.equal(posts[0].path, '/api/boards/submit');
  assert.equal(posts[0].status, 200, posts[0].body);
  const sent = JSON.parse(posts[0].sent), answer = JSON.parse(posts[0].body);
  assert.deepEqual([sent.name, sent.course, sent.expert, sent.daily], ['ACE', pick.course, false, DATE]);
  assert.ok(sent.vehicle >= 1 && sent.vehicle < 27, `an ATV: ${sent.vehicle}`);
  assert.equal(sent.trace.marks.length, defaultLaps(pick.course));
  assert.equal(sent.raceMs, sent.trace.marks.at(-1));
  const value = { lap: sent.lapMs, race: sent.raceMs, daily: sent.raceMs };
  const expected = {};
  for (const [kind, board] of Object.entries(BOARDS)) {
    const ahead = house(board).filter(row => row.value < value[kind]).length;
    expected[kind] = ahead + 1;
    assert.deepEqual(answer.boards[board], { rank: ahead + 1, placed: true }, `${board}: ${value[kind]} ms behind ${ahead} house riders`);
  }
  // The path driver is no quicker than the quickest house rider, so the
  // ranks are not all first: the house riders were counted.
  assert.ok(Object.values(expected).some(rank => rank > 1), JSON.stringify(expected));
  const world = [[expected.daily, 'Daily Event'], [expected.lap, 'Fastest Lap'], [expected.race, 'Fastest Race']]
    .map(([rank, what]) => [rank, `World #${rank} ${what}!`]).sort((a, b) => a[0] - b[0] || (a[1] < b[1] ? -1 : 1)).map(line => line[1]).join(' · ');
  assert.equal(s.ride.world, world);
  const stored = env.SAVES.db.prepare('SELECT board, name, value, vehicle, day FROM board_entries ORDER BY board').all().map(row => ({ ...row }));
  assert.deepEqual(stored, [
    { board: BOARDS.daily, name: 'ACE', value: sent.raceMs, vehicle: sent.vehicle, day: DATE },
    { board: BOARDS.lap, name: 'ACE', value: sent.lapMs, vehicle: sent.vehicle, day: DATE },
    { board: BOARDS.race, name: 'ACE', value: sent.raceMs, vehicle: sent.vehicle, day: DATE },
  ]);
  if (process.env.ATV_WORLD_SUBMIT_SHOTS) { await nextFrames(page, 20); await page.screenshot({ path: `${process.env.ATV_WORLD_SUBMIT_SHOTS}/results.png` }); }

  // Continue → Quit: back in the menus the boards show the row.
  await key(page, 'Enter');
  await until(page, s => s.ride?.results?.length === 2 && !s.ride.fame, 15000, 'the Retry / Quit menu');
  await key(page, 'ArrowDown');
  await key(page, 'Enter');
  await onPage(page, 'main', 120000);
  await choose(page, 'single'); await onPage(page, 'single');
  await choose(page, 'daily'); await onPage(page, 'daily');
  const row = (rank, ms) => new RegExp(`^▶ ${rank}\\. ACE · .*\\|${escaped(clock(ms))}$`);
  await waitDetail(page, row(expected.daily, sent.raceMs));
  lines = await details(page);
  // The top ten, and the rider's own row below them when it ranks lower.
  const shown = rank => 10 + (rank > 10 ? 1 : 0);
  assert.equal(lines.filter(l => /^(▶ )?\d+\. /.test(l)).length, shown(expected.daily), lines.join('\n'));
  if (process.env.ATV_WORLD_SUBMIT_SHOTS) { await nextFrames(page, 20); await page.screenshot({ path: `${process.env.ATV_WORLD_SUBMIT_SHOTS}/daily.png` }); }

  // Hall of Fame → View: World on the course: its lap and race boards.
  await key(page, 'Escape'); await onPage(page, 'single');
  await key(page, 'Escape'); await onPage(page, 'main');
  await choose(page, 'halloffame'); await onPage(page, 'hall-of-fame');
  await choose(page, 'fameworld');
  const asked = board => calls.some(c => c.method === 'GET' && c.path === `/api/boards/${board}` && c.status === 200 && JSON.parse(c.body).entries.some(e => e.name === 'ACE' && !e.house));
  // The second row cycles the course; stop on the event's.
  let f = await front(page);
  for (let i = f.selection; i < 1; i++) await key(page, 'ArrowDown');
  for (let i = 0; i < 12 && !(asked(BOARDS.lap) && asked(BOARDS.race)); i++) {
    await key(page, 'ArrowRight');
    await page.waitForTimeout(400);
  }
  assert.ok(asked(BOARDS.lap) && asked(BOARDS.race), `the course's boards: ${calls.filter(c => c.method === 'GET').map(c => c.path).join(', ')}`);
  await waitDetail(page, row(expected.lap, sent.lapMs));
  await waitDetail(page, row(expected.race, sent.raceMs));
  lines = await details(page);
  assert.ok(lines.some(l => l === 'Fastest Lap|') && lines.some(l => l === 'Fastest 5-Lap Race|'), lines.join('\n'));
  // Each board's ten rows, the rider's among the house riders'.
  assert.equal(lines.filter(l => /^(▶ )?\d+\. /.test(l)).length, shown(expected.lap) + shown(expected.race), lines.join('\n'));
  if (process.env.ATV_WORLD_SUBMIT_SHOTS) { await nextFrames(page, 20); await page.screenshot({ path: `${process.env.ATV_WORLD_SUBMIT_SHOTS}/hall-of-fame.png` }); }

  assertNoErrors(errors);
  console.log(`PASS: world submission end to end on ${pick.course}: lap ${clock(sent.lapMs)} #${expected.lap}, race ${clock(sent.raceMs)} #${expected.race}, daily #${expected.daily} among ${HOUSE_ROWS} house riders; "${world}"`);
} finally {
  await browser.close();
}
