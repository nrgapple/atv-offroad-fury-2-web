// Host-only gate: Quick Play (research/QUICK_PLAY.md) against a local
// fury-server. One rider queues from Main Menu → Quick Play → Race (a
// landscape page), another straight from `?quickplay=race` (a 360 px
// portrait phone). The server seats both in one QPR room with CPU seats,
// both press GO, both vote the race card, the pick counts down, the
// server's driver starts the race (CPU-filled, one lap with `raceLaps=1`
// and the race path driver), the server's checked podium reaches both, and
// Rematch from both starts the next vote.
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_quickplay.mjs
// ATV_QP_SHOTS=<dir> saves a capture of each screen.
import assert from 'node:assert/strict';
import { assertNoErrors, base, launch, newPage, nextFrames } from './browser/harness.mjs';

const shots = process.env.ATV_QP_SHOTS || '';
const errors = [];
const ui = p => p.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const mp = p => p.evaluate(() => JSON.parse(window.furyMultiplayer ?? 'null'));
const panel = async p => (await ui(p)).toolbar?.quickPlay;
const loop = async p => (await mp(p))?.roomLoop;
const PHASES = ['Lobby', 'Vote', 'Countdown', 'Play', 'Results'];
async function until(p, read, predicate, timeout, label) {
  const deadline = Date.now() + timeout;
  for (;;) {
    const value = await read(p).catch(() => null);
    if (value && predicate(value)) return value;
    if (Date.now() > deadline) {
      throw new Error(`timed out: ${label}: ${JSON.stringify(value)} errors ${errors.slice(0, 5).join(' | ')}`);
    }
    await p.waitForTimeout(200);
  }
}
async function key(page, k) {
  await page.keyboard.down(k);
  await nextFrames(page, 2);
  await page.keyboard.up(k);
  await nextFrames(page, 3);
}
/// Taps a Quick Play button by its hit id (atv_ui::hud::quickplay).
async function tap(page, id) {
  const hit = (await ui(page)).toolbar.hits.find(h => h.id === id);
  assert.ok(hit, `button ${id} drawn`);
  assert.ok(hit.height >= 48, `button ${id} is a 48 px target: ${hit.height}`);
  await page.mouse.click(hit.x + hit.width / 2, hit.y + hit.height / 2);
  await nextFrames(page, 3);
}
async function shot(p, name) {
  if (!shots) return;
  await nextFrames(p, 4);
  await p.screenshot({ path: `${shots}/${name}.png` });
}

const CARD = 80;
const READY = 84;
const REMATCH = 85;
const flags = 'uiDiagnostics=1&weather=off&autopilot=1&raceLaps=1';
const browser = await launch();
try {
  const open = async viewport => {
    const p = await newPage(browser, { viewport, hasTouch: true }, errors);
    await p.addInitScript(() => {
      localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
      Object.defineProperty(document, 'hasFocus', { value: () => true });
    });
    return p;
  };
  // 1. Main Menu leads with Quick Play once the server says it runs it.
  const one = await open({ width: 1280, height: 720 });
  // No `name=`: with a name the link is a ride link and skips the menus.
  await one.goto(`${base}/play.html?${flags}`);
  await until(one, ui, s => s.front?.page === 'title', 120000, 'title');
  await one.mouse.click(8, 8);
  await key(one, 'Enter');
  await until(one, ui, s => s.front?.page === 'main' && s.front.quickplay, 60000, 'Quick Play advertised');
  const main = (await ui(one)).front;
  assert.equal(main.choices[0], 'quickplay', `main menu ${main.choices}`);
  await shot(one, 'qp-main-menu');
  await key(one, 'Enter');
  await until(one, ui, s => s.front?.page === 'quickplay', 30000, 'Quick Play page');
  assert.deepEqual((await ui(one)).front.choices.slice(0, 2), ['qp-race', 'qp-stunts']);
  await shot(one, 'qp-tiles');
  await key(one, 'Enter');
  let a = await until(one, mp, m => m.code?.startsWith('QPR') && m.roomLoop, 240000, 'one queued');

  // 2. A phone rider queues straight from the link and lands in the same room.
  const two = await open({ width: 360, height: 740 });
  await two.goto(`${base}/play.html?${flags}&name=QPTwo&quickplay=race`);
  const b = await until(two, mp, m => m.code && m.roomLoop, 240000, 'two queued');
  assert.equal(b.code, a.code, 'one room for both');
  for (const p of [one, two]) {
    await until(p, loop, l => l.seats.filter(s => !(s.flags & 1)).length === 2, 60000, 'two riders seated');
  }
  let l = await loop(two);
  assert.equal(l.phase, 0, 'still in the lobby');
  assert.equal(l.seats.length, 6, 'CPU seats fill the race to six');
  const q = await until(two, panel, s => s.phase === 'Lobby' && s.seats.length === 6, 30000, 'lobby panel');
  assert.ok(q.seconds > 0 && q.seconds <= 20, `a live countdown: ${q.seconds}`);
  assert.equal(q.seats.filter(s => s.cpu).length, 4, 'CPU tags');
  await shot(two, 'qp-lobby-phone');

  // 3. GO from both skips the wait; the vote deals 3 cards and Random.
  await tap(one, READY);
  await tap(two, READY);
  const vote = await until(one, panel, s => s.phase === 'Vote', 30000, 'vote');
  assert.equal(vote.cards.length, 4);
  assert.equal(vote.cards[3].mode, 'Random');
  const race = vote.cards.findIndex(c => c.mode === 'Race');
  assert.ok(race >= 0, `a race card: ${JSON.stringify(vote.cards)}`);
  await tap(one, CARD + race);
  await until(two, panel, s => s.cards[race]?.votes === 1, 15000, 'live vote count');
  await shot(two, 'qp-vote-phone');
  await shot(one, 'qp-vote');
  await tap(two, CARD + race);
  // Both voted: the roulette lands on the race card and counts down.
  await until(one, panel, s => s.phase === 'Countdown' || s.phase === 'Play', 15000, 'countdown');
  l = await loop(one);
  assert.equal(l.pick, race, 'every ticket on the race card');
  assert.equal(l.setup.round.mode, 3);
  await shot(one, 'qp-countdown');

  // 4. The server's driver starts the CPU-filled race; both race it.
  const roomRace = async p => (await ui(p)).ride?.roomRace;
  for (const p of [one, two]) {
    await until(p, roomRace, r => r.loaded && r.phase >= 1, 240000, 'race loaded');
  }
  const r = await roomRace(one);
  assert.equal(r.laps, 1);
  assert.ok(r.grid.filter(s => s > 0).length === 2, `two riders on the grid: ${r.grid}`);
  await until(one, roomRace, x => x.phase >= 2, 90000, 'racing');
  await shot(two, 'qp-racing-phone');

  // 5. The server's podium reaches both, with server-checked times.
  for (const p of [one, two]) {
    await until(p, panel, s => s.phase === 'Results' && s.rows.length >= 2, 600000, 'podium');
  }
  l = await loop(one);
  const humans = l.results.filter(row => !(row.flags & 1));
  assert.equal(humans.length, 2, `both riders ranked: ${JSON.stringify(l.results)}`);
  assert.ok(l.verified, 'the server checked the race');
  assert.deepEqual((await loop(two)).results, l.results, 'one podium');
  await shot(one, 'qp-podium');
  await shot(two, 'qp-podium-phone');

  // 6. Rematch from both: the next vote, same room.
  await tap(one, REMATCH);
  await tap(two, REMATCH);
  for (const p of [one, two]) {
    await until(p, loop, x => x.phase === 1 && x.setup.number === 1, 30000, 'next vote');
  }
  assert.equal((await mp(two)).code, a.code, 'the room stays together');
  assertNoErrors(errors);
  console.log(`Quick Play: menu row and tiles, one room for two riders with CPU seats, GO, vote and roulette, driver-started race, checked podium (${humans.map(h => h.value).join(', ')} ms) and rematch verified`);
} finally {
  await browser.close();
}
