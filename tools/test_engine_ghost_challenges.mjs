// Ghost challenges end to end (research/GHOST_CHALLENGES.md): a two-lap
// Supercross race ridden by the race path driver (`autopilot=1`) earns its
// best lap's medal, which the results reveal; the lap places on the world
// board and keeps a personal best ghost. Single Race's course list then shows
// the medal, and Lap Attack's Ghost row offers Off, PB, the next medal's
// ghost, Rival and #1 from the board; Rival starts Lap Attack against the
// rival's ghost. Single Player → Weekly Challenge shows the week's pick, the
// rider's best and its band from the cron's cuts.
//
// Production is never touched: the game's /api/boards and /api/ghosts
// requests go to the Worker itself (cloudflare/worker.mjs, run in this
// process) over a local SQLite D1 with the production migrations
// (cloudflare/testing.mjs). Other riders' rows and ghosts are written straight
// into that D1; their ghosts are synthetic laps round a circle in the
// format the game reads (atv_core::world_ghost).
//
// ATV_GHOST_CHALLENGE_SHOTS=<dir> saves the medal reveal, the course list,
// the ghost picker, the race against the rival and the weekly page.
import assert from 'node:assert/strict';
import { assertNoErrors, base, launch, newPage, nextFrames, unlockStore, url } from './browser/harness.mjs';

const { default: worker } = await import('../cloudflare/worker.mjs');
const { d1 } = await import('../cloudflare/testing.mjs');
const { GHOST_HEADER, GHOST_KEY, GHOST_STRIDE } = await import('../cloudflare/ghosts.mjs');
const { isoWeekOf, refreshWeekly, weeklyPick } = await import('../cloudflare/weekly.mjs');

const open = { limit: async () => ({ success: true }) };
const env = { SAVES: d1(), GAME_ORIGINS: new URL(base).origin, BOARD_READ_LIMIT: open, BOARD_SUBMIT_LIMIT: open };
const shots = process.env.ATV_GHOST_CHALLENGE_SHOTS;
const shot = async (page, name) => { if (shots) { await nextFrames(page, 20); await page.screenshot({ path: `${shots}/${name}.png` }); } };

const calls = [];
async function api(route) {
  const request = route.request();
  const path = decodeURIComponent(new URL(request.url()).pathname);
  const headers = { 'CF-Connecting-IP': '127.0.0.1' };
  for (const name of ['origin', 'content-type', 'sec-fetch-site']) if (request.headers()[name]) headers[name] = request.headers()[name];
  const response = await worker.fetch(new Request(request.url(), { method: request.method(), headers, body: request.postData() ?? undefined }), env);
  const body = await response.text();
  calls.push({ method: request.method(), path, sent: request.postData(), status: response.status, body });
  await route.fulfill({ status: response.status, headers: { 'content-type': response.headers.get('Content-Type') ?? 'application/json' }, body });
}
const log = () => calls.map(c => `${c.method} ${c.path} ${c.status}`).join(', ');

// A synthetic lap of `lapMs` round a circle of `feet` (the Worker's checks
// and the game's decoder accept it).
function ghost(course, vehicle, lapMs, feet = 4000) {
  const steps = Math.round((lapMs * 60) / 1000), keys = Math.floor((steps - 1) / GHOST_STRIDE) + 1;
  const bytes = new Uint8Array(GHOST_HEADER + keys * GHOST_KEY), view = new DataView(bytes.buffer);
  bytes.set([0x46, 0x52, 0x47, 0x48, 1, GHOST_STRIDE]);
  view.setUint16(6, vehicle, true); view.setUint32(8, lapMs, true); view.setUint32(12, steps, true); view.setUint16(16, keys, true);
  bytes[18] = course.length;
  for (let i = 0; i < course.length; i++) bytes[19 + i] = course.charCodeAt(i);
  const radius = feet / (2 * Math.PI);
  for (let key = 0; key < keys; key++) {
    const angle = (2 * Math.PI * key * GHOST_STRIDE) / steps, at = GHOST_HEADER + key * GHOST_KEY;
    view.setInt32(at, Math.round(radius * Math.cos(angle) * 32), true);
    view.setInt32(at + 4, 12 * 32, true);
    view.setInt32(at + 8, Math.round(radius * Math.sin(angle) * 32), true);
    view.setUint32(at + 12, 0xc0080200, true);
  }
  return Buffer.from(bytes).toString('base64');
}
// Another rider's row on a board, with its ghost when `withGhost`.
function seed(board, course, name, value, withGhost) {
  env.SAVES.db.prepare('INSERT INTO board_entries (board, name, value, vehicle, day) VALUES (?, ?, ?, 3, ?)').run(board, name, value, '2026-11-14');
  if (withGhost) env.SAVES.db.prepare('INSERT INTO board_ghosts (board, name, value, data) VALUES (?, ?, ?, ?)').run(board, name, value, ghost(course, 3, value));
}

const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const front = async page => (await read(page)).front || {};
const onPage = (page, name, timeout = 90000) =>
  page.waitForFunction(name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name, name, { timeout });
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }
async function focus(page, action) {
  const f = await front(page);
  const at = f.choices.indexOf(action);
  assert.ok(at >= 0, `${action} in ${f.page}: ${f.choices}`);
  for (let i = f.selection; i < at; i++) await key(page, 'ArrowDown');
  for (let i = f.selection; i > at; i--) await key(page, 'ArrowUp');
}
async function choose(page, action) { await focus(page, action); await key(page, 'Enter'); }
const label = (f, action) => f.labels[f.choices.indexOf(action)];
async function until(page, predicate, timeout, what) {
  const deadline = Date.now() + timeout;
  for (;;) {
    const s = await read(page);
    if (predicate(s)) return s;
    if (Date.now() > deadline) throw new Error(`timed out: ${what}; race ${JSON.stringify(s.ride?.raceLap ?? null)}; front ${JSON.stringify({ page: s.front?.page, labels: s.front?.labels, notice: s.front?.notice })}; calls ${log()}`);
    await page.waitForTimeout(250);
  }
}
const details = async page => ((await front(page)).profileBook?.details || []).map(([a, b]) => `${a}|${b}`);
const clock = ms => { const c = Math.floor(ms / 10); return `${String(Math.floor(c / 6000)).padStart(2, '0')}:${String(Math.floor(c / 100) % 60).padStart(2, '0')}.${String(c % 100).padStart(2, '0')}`; };

// This week's challenge: a best time on this device, other riders on its
// board and the cron's cuts.
const week = isoWeekOf(Date.now()), pick = weeklyPick(week);
const WEEKLY = `weekly:${week}`;
for (let i = 0; i < 20; i++) seed(WEEKLY, pick.course, `W${i}`, 300_000 + i * 5_000, false);
await refreshWeekly(env, Date.now());
const weeklyBest = 341_000; // between the 50 % cut (the 10th, 345,000) and the 25 % one (5th, 320,000)

const browser = await launch();
try {
  const context = await browser.newContext({ viewport: { width: 1280, height: 720 } });
  await context.route('**/api/boards/**', api);
  await context.route('**/api/ghosts/**', api);
  const errors = [];
  const page = await newPage(context, {}, errors);
  await unlockStore(page);
  await page.addInitScript(([week, best]) => {
    if (sessionStorage.getItem('ghost-challenges-test')) return;
    sessionStorage.setItem('ghost-challenges-test', '1');
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    for (const key of ['fury-hall-of-fame', 'atv-player-selection']) localStorage.removeItem(key);
    localStorage.setItem('fury-medals', JSON.stringify({ weeks: { [week]: { value: best } } }));
  }, [week, weeklyBest]);
  await page.goto(url('/play.html?uiDiagnostics=1&autopilot=1&weather=off'));
  await onPage(page, 'title');
  await page.mouse.click(8, 8);
  await key(page, 'Enter'); await onPage(page, 'main');
  await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.choices?.includes('single'), null, { timeout: 60000 });

  // Single Race: two laps of the first Supercross course (lap 2 is judged).
  await choose(page, 'single'); await onPage(page, 'single');
  await choose(page, 'race'); await onPage(page, 'race');
  await key(page, 'PageUp');
  await focus(page, 'racelaps');
  for (let i = 0; i < 12 && label(await front(page), 'racelaps') !== 'Laps: 2'; i++) await key(page, 'ArrowLeft');
  let f = await front(page);
  const course = f.race.course, BOARD = `lap:${course}:n`;
  assert.ok(course.startsWith('sx'), course);
  await choose(page, 'racestart');
  let s = await until(page, s => s.ride?.raceLap?.phase === 'finished' && s.ride?.fame, 600000, 'the finish and the name entry');
  for (let i = 0; i < 8; i++) await key(page, 'Backspace');
  for (const code of ['KeyA', 'KeyC', 'KeyE']) await key(page, code);
  await key(page, 'Enter');

  // The results reveal the best lap's medal (new: nothing was held).
  s = await until(page, s => s.ride?.medal?.reveal >= 1, 30000, 'the medal reveal');
  const medal = s.ride.medal.medal;
  assert.ok(['Bronze', 'Silver', 'Gold', 'Author'].includes(medal), JSON.stringify(s.ride.medal));
  assert.equal(s.ride.medal.new, true);
  await until(page, () => calls.some(c => c.method === 'POST' && c.path.startsWith('/api/ghosts/')), 30000, 'the ghost upload');
  await shot(page, 'medal-reveal');
  const sent = JSON.parse(calls.find(c => c.path === '/api/boards/submit').sent);
  const lapMs = sent.lapMs;
  const book = JSON.parse(await page.evaluate(() => localStorage.getItem('fury-medals')));
  assert.equal(book.laps[course], lapMs, 'the medal book keeps the lap');
  assert.equal(book.ghosts[course], lapMs, 'and its personal best ghost');
  assert.equal(book.weeks[week].value, weeklyBest, 'the seeded weekly best survives');
  assert.ok(await page.evaluate(c => Boolean(localStorage.getItem(`fury-pb-ghost:${c}`)), course));

  // Other riders ahead of ACE on the lap board, with ghosts: a rival just
  // ahead and a leader far ahead.
  seed(BOARD, course, 'RIVAL', lapMs - 1_500, true);
  seed(BOARD, course, 'LEADER', Math.round(lapMs * 0.8), true);
  seed(BOARD, course, 'NOGHOST', lapMs - 800, false);

  // Continue → Quit: back to Single Race's setup.
  const CONTINUE = 30, QUIT = 33;
  s = await until(page, s => s.ride?.results?.includes(CONTINUE), 15000, 'the results buttons');
  await key(page, 'Enter');
  s = await until(page, s => s.ride?.results?.includes(QUIT), 15000, 'the Retry / Quit menu');
  for (let i = 0; i < s.ride.results.indexOf(QUIT); i++) await key(page, 'ArrowDown');
  await key(page, 'Enter');
  await onPage(page, 'race', 120000);

  // The course list shows the medal (the picker sheet, opened by its All
  // Courses button under the photo), then the setup again.
  await focus(page, 'racetrack');
  await page.mouse.click(967, 451);
  await nextFrames(page, 30);
  await shot(page, 'course-medal');
  await key(page, 'Escape');
  if ((await front(page)).page !== 'race') { await onPage(page, 'single'); await choose(page, 'race'); }
  await onPage(page, 'race');

  // Lap Attack: the Ghost row and its choices.
  await focus(page, 'raceevent');
  for (let i = 0; i < 4 && label(await front(page), 'raceevent') !== 'Event: Lap Attack'; i++) await key(page, 'ArrowLeft');
  assert.equal(label(await front(page), 'raceevent'), 'Event: Lap Attack');
  assert.equal((await front(page)).race.course, course);
  await until(page, () => calls.some(c => c.method === 'GET' && c.path === `/api/boards/${BOARD}`), 15000, 'the lap board');
  await focus(page, 'raceghost');
  const labels = [];
  for (let i = 0; i < 6; i++) {
    labels.push(label(await front(page), 'raceghost'));
    await key(page, 'ArrowRight');
  }
  const offered = [...new Set(labels)];
  assert.equal(offered[0], 'Ghost: Off', offered.join());
  assert.ok(offered.includes(`Ghost: PB ${clock(lapMs)}`), offered.join());
  assert.ok(offered.includes(`Ghost: Rival ${clock(lapMs - 1_500)}`), offered.join());
  assert.ok(offered.includes(`Ghost: #1 ${clock(Math.round(lapMs * 0.8))}`), offered.join());
  // The next medal's ghost: one of the stored ghosts, "~" when not within 1 %.
  assert.ok(offered.some(l => /^Ghost: ~?(Bronze|Silver|Gold|Author) /.test(l)) || medal === 'Author', offered.join());
  for (let i = 0; i < 8 && label(await front(page), 'raceghost') !== `Ghost: Rival ${clock(lapMs - 1_500)}`; i++) await key(page, 'ArrowRight');
  await shot(page, 'ghost-picker');
  await choose(page, 'racestart');
  s = await until(page, s => s.ride?.lapAttack === true && s.ride?.raceLap?.phase === 'racing', 240000, 'Lap Attack against the rival');
  s = await until(page, s => s.ride?.ghost?.playing === true, 30000, 'the rival ghost playing');
  assert.equal(s.ride.ghost.rival, 'RIVAL');
  assert.ok(Math.abs(s.ride.ghost.best - (lapMs - 1_500) / 1000) < 1e-3, `ghost lap ${s.ride.ghost.best}`);
  assert.ok(calls.some(c => c.method === 'GET' && c.path === `/api/ghosts/${BOARD}/RIVAL/${lapMs - 1_500}` && c.status === 200), log());
  await nextFrames(page, 60);
  await shot(page, 'rival-race');

  // Pause → Quit to Main Menu, then Single Player → Weekly Challenge.
  await key(page, 'Escape');
  await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').toolbar?.menu?.page === 'pause', null, { timeout: 30000 });
  const menu = (await read(page)).toolbar.menu;
  for (let i = 0; i < menu.choices.indexOf('main'); i++) await key(page, 'ArrowDown');
  await key(page, 'Enter');
  await onPage(page, 'main', 60000);
  await choose(page, 'single'); await onPage(page, 'single');
  f = await front(page);
  assert.ok(f.choices.includes('weekly'), `the Weekly Challenge row: ${f.choices}`);
  assert.equal(label(f, 'weekly'), 'Weekly Challenge');
  await choose(page, 'weekly'); await onPage(page, 'weekly');
  await until(page, s => (s.front?.profileBook?.details || []).some(([a]) => /^1\. /.test(a)), 20000, 'the weekly board');
  const lines = await details(page);
  assert.ok(lines[0].startsWith(`Weekly · ${week}|`) && / left$/.test(lines[0]), lines.join('\n'));
  assert.ok(lines.some(l => l.endsWith('· No CPUs')), lines.join('\n'));
  assert.ok(lines.includes(`Your Best:|${clock(weeklyBest)} · Top 50% · 20 riders`), lines.join('\n'));
  await shot(page, 'weekly-page');
  assertNoErrors(errors);
  console.log(`ghost challenges: ${course} lap ${clock(lapMs)} earned ${medal}; picker offered ${offered.join(' / ')}; Rival raced; weekly ${week} on ${pick.course}`);
} finally {
  await browser.close();
}
