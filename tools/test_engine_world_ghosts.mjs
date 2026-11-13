// World board ghosts end to end (research/WORLD_BOARDS.md "Ghosts"): a
// two-lap Supercross race ridden by the race path driver (`autopilot=1`)
// sets a local Fastest Lap; naming it sends the lap to the world boards and,
// once it placed, the lap's ghost. Main Menu → Hall of Fame → View: World
// then marks the row "Ghost" and offers Race This Ghost. A download that is
// not a usable ghost (cut short, a newer format, another lap's, not JSON, a
// missing row, no network) is reported on the page and starts nothing; the
// real one starts Lap Attack on the course with the ghost replaying from GO
// under the rider's name, drawn and moving.
//
// Nothing is mocked and production is never touched: the game's /api/boards
// and /api/ghosts requests are handed to the Worker itself
// (cloudflare/worker.mjs, run in this process) over a local SQLite D1 with
// the production migrations. The bad downloads are the stored ghost's real
// answer altered on its way back to the page.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { DatabaseSync } from 'node:sqlite';
import { assertNoErrors, base, launch, newPage, nextFrames, unlockStore, url } from './browser/harness.mjs';

const { default: worker } = await import('../cloudflare/worker.mjs');
const { MAX_GHOST_BYTES, checkGhost, decodeGhost, ghostHeader } = await import('../cloudflare/ghosts.mjs');

// A D1 stand-in over real SQLite with the production migrations applied.
function d1() {
  const db = new DatabaseSync(':memory:');
  for (const file of ['0001_cloud_saves.sql', '0002_world_boards.sql', '0003_board_ghosts.sql']) db.exec(readFileSync(new URL(`../cloudflare/migrations/${file}`, import.meta.url), 'utf8'));
  const statement = (sql, params = []) => ({
    bind: (...values) => statement(sql, values),
    run: async () => statement(sql, params).exec(),
    all: async () => statement(sql, params).exec(),
    first: async () => statement(sql, params).exec().results[0] ?? null,
    exec() {
      const prepared = db.prepare(sql);
      if (/^\s*SELECT/i.test(sql)) return { results: prepared.all(...params).map(row => ({ ...row })), meta: { changes: 0 } };
      return { results: [], meta: { changes: Number(prepared.run(...params).changes) } };
    },
  });
  return {
    db,
    prepare: sql => statement(sql),
    batch: async statements => {
      db.exec('BEGIN');
      try { const results = statements.map(item => item.exec()); db.exec('COMMIT'); return results; } catch (failure) { db.exec('ROLLBACK'); throw failure; }
    },
  };
}
const open = { limit: async () => ({ success: true }) };
const env = { SAVES: d1(), GAME_ORIGINS: new URL(base).origin, BOARD_READ_LIMIT: open, BOARD_SUBMIT_LIMIT: open };

// The local Worker. `tamper` alters the next ghost download's answer.
const calls = [];
let tamper = null;
async function api(route) {
  const request = route.request();
  const path = decodeURIComponent(new URL(request.url()).pathname);
  if (tamper === 'offline' && request.method() === 'GET' && path.startsWith('/api/ghosts/')) {
    tamper = null;
    calls.push({ method: 'GET', path, status: 0, body: '' });
    return route.abort('internetdisconnected');
  }
  const headers = { 'CF-Connecting-IP': '127.0.0.1' };
  for (const name of ['origin', 'content-type', 'sec-fetch-site']) if (request.headers()[name]) headers[name] = request.headers()[name];
  const response = await worker.fetch(new Request(request.url(), { method: request.method(), headers, body: request.postData() ?? undefined }), env);
  let body = await response.text(), status = response.status;
  if (tamper && request.method() === 'GET' && path.startsWith('/api/ghosts/')) {
    ({ status, body } = tamper({ status, body }));
    tamper = null;
  }
  calls.push({ method: request.method(), path, sent: request.postData(), status, body });
  await route.fulfill({ status, headers: { 'content-type': response.headers.get('Content-Type') ?? 'application/json' }, body });
}
const log = () => calls.map(c => `${c.method} ${c.path} ${c.status}`).join(', ');

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
    if (Date.now() > deadline) throw new Error(`timed out: ${what}; race ${JSON.stringify(s.ride?.raceLap ?? null)}; ghost ${JSON.stringify(s.ride?.ghost ?? null)}; notice ${JSON.stringify(s.front?.notice ?? null)}; calls ${log()}`);
    await page.waitForTimeout(250);
  }
}
const details = async page => ((await front(page)).profileBook?.details || []).map(([a, b]) => `${a}|${b}`);
const clock = ms => { const c = Math.floor(ms / 10); return `${String(Math.floor(c / 6000)).padStart(2, '0')}:${String(Math.floor(c / 100) % 60).padStart(2, '0')}.${String(c % 100).padStart(2, '0')}`; };

const browser = await launch();
try {
  const context = await browser.newContext({ viewport: { width: 1280, height: 720 } });
  await context.route('**/api/boards/**', api);
  await context.route('**/api/ghosts/**', api);
  const errors = [];
  const page = await newPage(context, {}, errors);
  await unlockStore(page);
  await page.addInitScript(() => {
    if (sessionStorage.getItem('world-ghosts-test')) return;
    sessionStorage.setItem('world-ghosts-test', '1');
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.removeItem('fury-hall-of-fame');
    localStorage.removeItem('atv-player-selection');
  });
  await page.goto(url('/play.html?uiDiagnostics=1&autopilot=1&weather=off'));
  await onPage(page, 'title');
  await page.mouse.click(8, 8);
  await key(page, 'Enter'); await onPage(page, 'main');

  // Single Race: two laps of a Supercross course (the shortest laps; its
  // first lap from the grid is not judged, so lap 2 is the best lap).
  await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.choices?.includes('single'), null, { timeout: 60000 });
  await choose(page, 'single'); await onPage(page, 'single');
  await choose(page, 'race'); await onPage(page, 'race');
  let f = await front(page);
  assert.equal(label(f, 'raceevent'), 'Event: Single Race');
  await key(page, 'PageUp');
  await focus(page, 'racelaps');
  for (let i = 0; i < 12 && label(await front(page), 'racelaps') !== 'Laps: 2'; i++) await key(page, 'ArrowLeft');
  f = await front(page);
  assert.equal(label(f, 'racelaps'), 'Laps: 2');
  const course = f.race.course;
  assert.ok(course.startsWith('sx'), course);
  await choose(page, 'racestart');
  let s = await until(page, s => s.ride?.raceLap?.phase === 'racing', 240000, 'the race starts');
  s = await until(page, s => s.ride?.raceLap?.phase === 'finished' && s.ride?.fame, 420000, 'the finish and the name entry');
  assert.ok(s.ride.fame.broken.some(record => /Fastest Lap Time/.test(record)), JSON.stringify(s.ride.fame));
  assert.equal(calls.filter(c => c.method === 'POST').length, 0, 'nothing is sent before the name is entered');
  for (let i = 0; i < 8; i++) await key(page, 'Backspace');
  for (const code of ['KeyA', 'KeyC', 'KeyE']) await key(page, code);
  await key(page, 'Enter');

  // The submission, then the placed lap's ghost.
  await until(page, () => calls.some(c => c.method === 'POST' && c.path.startsWith('/api/ghosts/')), 30000, 'the ghost upload');
  const posts = calls.filter(c => c.method === 'POST');
  assert.deepEqual(posts.map(c => `${c.path} ${c.status}`), ['/api/boards/submit 200', `/api/ghosts/lap:${course}:n 200`], posts.map(c => c.body).join('\n'));
  const sent = JSON.parse(posts[0].sent), placed = JSON.parse(posts[0].body);
  const BOARD = `lap:${course}:n`;
  assert.deepEqual([sent.name, sent.course, sent.expert], ['ACE', course, false]);
  assert.equal(placed.boards[BOARD].placed, true, posts[0].body);
  assert.deepEqual(JSON.parse(posts[1].body), { board: BOARD, name: 'ACE', kept: true });
  const upload = JSON.parse(posts[1].sent);
  assert.equal(upload.name, 'ACE');
  const bytes = decodeGhost(upload.ghost), header = ghostHeader(bytes);
  assert.deepEqual([header.course, header.vehicle, header.lapMs, header.stride], [course, sent.vehicle, sent.lapMs, 6]);
  assert.equal(checkGhost(bytes, { course, vehicle: sent.vehicle, lapMs: sent.lapMs }), null);
  assert.ok(bytes.length < MAX_GHOST_BYTES && bytes.length === 32 + 16 * header.keys);
  // The measured size of this lap (research/WORLD_BOARDS.md quotes it).
  console.log(`ghost: ${course} lap ${clock(sent.lapMs)} = ${header.steps} steps, ${header.keys} keys, ${bytes.length} bytes (${upload.ghost.length} as base64)`);
  assert.deepEqual(env.SAVES.db.prepare('SELECT board, name, value FROM board_ghosts').all().map(row => ({ ...row })), [{ board: BOARD, name: 'ACE', value: sent.lapMs }]);

  // Continue → Quit (hud/results.rs buttons): back to Single Race's setup.
  const CONTINUE = 30, QUIT = 33;
  s = await until(page, s => s.ride?.results?.includes(CONTINUE), 15000, 'the results buttons');
  await key(page, 'Enter');
  s = await until(page, s => s.ride?.results?.includes(QUIT), 15000, 'the Retry / Quit menu');
  for (let i = 0; i < s.ride.results.indexOf(QUIT); i++) await key(page, 'ArrowDown');
  await key(page, 'Enter');
  await onPage(page, 'race', 120000);
  for (const back of ['single', 'main']) { await key(page, 'Escape'); await onPage(page, back); }

  // Hall of Fame → View: World on the course: the row offers its ghost.
  await choose(page, 'halloffame'); await onPage(page, 'hall-of-fame');
  await choose(page, 'fameworld');
  // Only this course's lap board has a ghost row: step the Event Type and
  // Track rows until the page offers it.
  const offered = async () => { await page.waitForTimeout(400); return (await front(page)).choices.includes('fameghostrace'); };
  search: for (let group = 0; group < 10; group++) {
    await focus(page, 'famecourse');
    const first = label(await front(page), 'famecourse');
    for (let i = 0; i < 24; i++) {
      if (await offered()) break search;
      await key(page, 'ArrowRight');
      if (label(await front(page), 'famecourse') === first) break;
    }
    await focus(page, 'fameevent');
    await key(page, 'ArrowRight');
  }
  assert.ok(await offered(), `the course's lap board with the ghost row: ${log()}`);
  assert.ok(calls.some(c => c.method === 'GET' && c.path === `/api/boards/${BOARD}` && JSON.parse(c.body).entries.some(e => e.name === 'ACE' && e.ghost)), log());
  f = await front(page);
  assert.equal(label(f, 'fameghost'), `Ghost: ACE · ${clock(sent.lapMs)}`);
  assert.equal(label(f, 'fameghostrace'), 'Race This Ghost');
  const row = new RegExp(`^▶ \\d+\\. ACE · .* · Ghost\\|${clock(sent.lapMs).replace('.', '\\.')}$`);
  assert.ok((await details(page)).some(l => row.test(l)), (await details(page)).join('\n'));
  if (process.env.ATV_WORLD_GHOST_SHOTS) { await nextFrames(page, 20); await page.screenshot({ path: `${process.env.ATV_WORLD_GHOST_SHOTS}/world-ghost-row.png` }); }

  // A download that is not a usable ghost fails with a message: the page
  // stays, no ride starts and nothing throws.
  // `edit` changes the ghost's bytes in place, or returns other text.
  const edited = edit => ({ status, body }) => {
    const answer = JSON.parse(body), ghost = Buffer.from(answer.ghost, 'base64');
    const text = edit(ghost) ?? ghost.toString('base64');
    return { status, body: JSON.stringify({ ...answer, ghost: text }) };
  };
  const bad = [
    ['cut short', edited(ghost => ghost.subarray(0, ghost.length - 7).toString('base64')), /could not be read/],
    ['a newer format', edited(ghost => { ghost[4] = 9; }), /newer version/],
    ['another lap\'s', edited(ghost => { ghost.writeUInt32LE(ghost.readUInt32LE(8) + 1, 8); }), /another lap/],
    ['another course\'s', edited(ghost => { ghost[21] ^= 1; }), /another lap|could not be read/],
    ['a wild position', edited(ghost => { ghost.writeInt32LE(0x7fffffff, 32 + 16 * 5); }), /could not be read/],
    ['not base64', edited(() => '*not base64*'), /could not be read/],
    ['not JSON', () => ({ status: 200, body: '<html>' }), /could not be read/],
    ['an empty answer', () => ({ status: 200, body: '{}' }), /could not be read/],
    ['a missing row', () => ({ status: 404, body: '{"error":"not_found"}' }), /no longer on the board/],
    ['a server error', () => ({ status: 503, body: '{"error":"unavailable"}' }), /could not be loaded/],
    ['no network', 'offline', /Offline/],
  ];
  for (const [what, change, message] of bad) {
    const before = calls.length;
    // A missing row drops the board, which is read again before the next try.
    await until(page, s => s.front?.choices?.includes('fameghostrace'), 15000, `Race This Ghost before ${what}`);
    tamper = change;
    await choose(page, 'fameghostrace');
    s = await until(page, s => message.test(s.front?.notice ?? ''), 20000, `${what}: ${message}`);
    assert.equal(s.front.page, 'hall-of-fame', what);
    assert.ok(!s.ride?.environment, `${what}: no ride starts`);
    assert.ok(calls.slice(before).some(c => c.method === 'GET' && c.path === `/api/ghosts/${BOARD}/ACE/${sent.lapMs}`), `${what}: ${log()}`);
  }
  assertNoErrors(errors, 'bad ghosts');

  // The real ghost: Lap Attack on the course, the ghost replaying from GO.
  await until(page, s => s.front?.choices?.includes('fameghostrace'), 15000, 'Race This Ghost');
  await choose(page, 'fameghostrace');
  s = await until(page, s => s.ride?.lapAttack === true && s.ride?.raceLap?.phase === 'racing', 240000, 'Lap Attack against the ghost');
  assert.equal(s.ride.environment.slice(0, 4), course.slice(0, 4));
  assert.equal(s.ride.raceLap.field, 1, 'Lap Attack races no CPUs');
  s = await until(page, s => s.ride?.ghost?.playing === true && s.ride.ghost.drawn === 1, 30000, 'the ghost playing and drawn');
  const ghost = s.ride.ghost;
  assert.equal(ghost.rival, 'ACE', 'the ghost rides under its rider\'s name');
  assert.ok(Math.abs(ghost.best - sent.lapMs / 1000) < 1e-3, `ghost lap ${ghost.best} vs ${sent.lapMs} ms`);
  assert.equal(ghost.samples, header.steps, 'every step of the lap replays');
  assert.ok(Math.abs(ghost.opacity - 0.4) < 1e-6, 'GhostOpacity 0.4');
  await nextFrames(page, 90);
  s = await read(page);
  assert.equal(s.ride.ghost.playing, true);
  const moved = Math.hypot(...s.ride.ghost.origin.map((v, i) => v - ghost.origin[i]));
  assert.ok(moved > 5, `the ghost moves along its lap: ${moved} ft in 90 frames`);
  assert.equal(s.ride.ghost.drawn, 1, 'the ghost stays drawn');
  if (process.env.ATV_WORLD_GHOST_SHOTS) await page.screenshot({ path: `${process.env.ATV_WORLD_GHOST_SHOTS}/world-ghost-race.png` });
  const downloads = calls.filter(c => c.method === 'GET' && c.path.startsWith('/api/ghosts/') && c.status === 200);
  assert.equal(JSON.parse(downloads.at(-1).body).ghost, upload.ghost, 'the download is the uploaded ghost');
  assertNoErrors(errors);
  console.log('world ghosts: a placed lap uploads its ghost, the World view offers it, bad downloads fail with a message and Lap Attack races the real one');
} finally {
  await browser.close();
}
