// Host-only gate: a private room's host-run loop (research/QUICK_PLAY.md
// "Private rooms") against a local fury-server. A Rust host opens a room
// on Supercross sx01 and a phone guest joins by code. The guest's Ready
// starts the lobby's 10 s countdown, which opens the vote: the room's own
// track leads three race cards and Random; both vote it, so it is picked
// at once. The host drives the race (one lap with `raceLaps=1` and the
// race path driver). A third rider joins mid-race, watches it with the
// follow camera ("Next round", cycling riders) while parked (no steps,
// drawn by no racer), sees the podium, and after everyone's Rematch takes a
// seat in the next vote.
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_private_loop.mjs
// ATV_PL_SHOTS=<dir> saves a capture of each screen.
import assert from 'node:assert/strict';
import { assertNoErrors, base, launch, newPage, nextFrames } from './browser/harness.mjs';

const shots = process.env.ATV_PL_SHOTS || '';
const errors = [];
const code = `PL${String(Date.now()).slice(-6)}`;
const ui = p => p.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const mp = p => p.evaluate(() => JSON.parse(window.furyMultiplayer ?? 'null'));
const panel = async p => (await ui(p)).toolbar?.quickPlay;
const loop = async p => (await mp(p))?.roomLoop;
const roomRace = async p => (await ui(p)).ride?.roomRace;
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
/// Taps a panel button by its hit id (atv_ui::hud::quickplay).
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
const START = 90;
const MODE = 91;
const CPUS = 92;
const SKIP = 93;
const WATCH = 94;
const SPECTATING = 8;
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
  // 1. The host's room and a phone guest: the loop's lobby on both.
  const host = await open({ width: 1280, height: 720 });
  await host.goto(`${base}/play.html?host=1&venue=sx01&room=${code}&name=PLHost&${flags}`);
  await until(host, mp, m => m.code === code && m.serves?.roomloop, 180000, 'host room');
  const guest = await open({ width: 360, height: 740 });
  await guest.goto(`${base}/play.html?room=${code}&dropAfter=9999&name=PLGuest&${flags}`);
  for (const p of [host, guest]) {
    await until(p, panel, s => s.private && s.phase === 'Lobby' && s.seats.length === 2, 240000, 'lobby panel');
  }
  let h = await panel(host);
  assert.equal(h.host, true, 'the host hosts');
  assert.equal(h.mode, 'Race');
  assert.equal(h.seconds, null, 'no countdown before everyone is ready');
  assert.ok(h.seats.find(s => s.name === 'PLHost')?.host, 'the host seat is tagged');
  const hits = (await ui(host)).toolbar.hits.map(x => x.id);
  for (const id of [START, MODE, CPUS]) assert.ok(hits.includes(id), `host button ${id}`);
  let g = await panel(guest);
  assert.equal(g.host, false);
  await shot(host, 'pl-lobby-host');
  await shot(guest, 'pl-lobby-guest-phone');

  // 2. The guest's Ready: a tick on the host's panel and a 10 s countdown.
  await tap(guest, READY);
  h = await until(host, panel, s => s.seats.find(x => x.name === 'PLGuest')?.ready && s.seconds > 0, 15000, 'ready tick');
  assert.ok(h.seconds <= 10, `a ten second countdown: ${h.seconds}`);
  await shot(host, 'pl-lobby-ready');
  // 3. The vote: the room's own track first, races only, then Random.
  const vote = await until(host, panel, s => s.phase === 'Vote', 20000, 'vote');
  assert.equal(vote.cards.length, 4);
  assert.equal(vote.cards[3].mode, 'Random');
  assert.ok(vote.cards.slice(0, 3).every(c => c.mode === 'Race'), `race cards: ${JSON.stringify(vote.cards)}`);
  let l = await loop(host);
  const sx01 = l.options[0].map;
  await tap(guest, CARD);
  await until(host, panel, s => s.cards[0]?.votes === 1, 15000, 'live vote count');
  await shot(guest, 'pl-vote-guest-phone');
  await shot(host, 'pl-vote-host');
  await tap(host, CARD);
  await until(host, loop, x => x.phase >= 2, 15000, 'picked');
  l = await loop(host);
  assert.equal(l.pick === 0 || l.phase === 3, true, `the voted card: ${l.pick}`);
  assert.equal(l.setup.round.map, sx01, 'the room track');
  assert.equal(l.setup.round.mode, 3);

  // 4. The host drives the race; both race it.
  for (const p of [host, guest]) {
    await until(p, roomRace, r => r.loaded && r.phase >= 1, 240000, 'race loaded');
  }
  const r = await roomRace(host);
  assert.equal(r.host, true);
  assert.equal(r.laps, 1);
  await until(host, roomRace, x => x.phase >= 2, 90000, 'racing');

  // 5. A late joiner watches: "Next round" and the follow camera.
  const late = await open({ width: 1280, height: 720 });
  await late.goto(`${base}/play.html?room=${code}&name=PLLate&${flags}`);
  const watching = await until(late, panel, s => s.phase === 'Play' && s.spectating && s.watching, 240000, 'spectating');
  assert.ok(['PLHost', 'PLGuest'].includes(watching.watching), `follows a racer: ${watching.watching}`);
  // Parked meanwhile: no steps, and no racer draws (or touches) it.
  const parked = await until(late, ui, s => s.parked === true, 10000, 'parked');
  // The room's arrival placement may still set it beside a rider once
  // (a placement, not a step); then it holds still.
  let before = parked;
  let still = parked;
  for (let i = 0; i < 10; i++) {
    await late.waitForTimeout(1500);
    still = await ui(late);
    if (JSON.stringify(still.origin) === JSON.stringify(before.origin)) break;
    before = still;
  }
  assert.equal(still.step, parked.step, 'a parked ride takes no steps');
  assert.deepEqual(still.origin, before.origin, 'nor moves');
  for (const p of [host, guest]) {
    const m = await mp(p);
    assert.ok(m.drawn <= 1, `racers draw only each other: ${m.drawn}`);
  }
  await shot(late, 'pl-spectate-follow');
  await tap(late, WATCH);
  await until(late, panel, s => s.watching && s.watching !== watching.watching, 10000, 'next rider');
  await shot(late, 'pl-spectate-next');

  // 6. The host's podium reaches everyone, the watcher too.
  for (const p of [host, guest, late]) {
    await until(p, panel, s => s.phase === 'Results' && s.rows.length >= 2, 600000, 'podium');
  }
  l = await loop(host);
  assert.equal(l.results.length, 2, `two racers ranked: ${JSON.stringify(l.results)}`);
  assert.equal(l.verified, false, 'a private room is its host\'s word');
  assert.deepEqual((await loop(late)).results, l.results, 'one podium');
  await shot(host, 'pl-podium-host');
  await shot(guest, 'pl-podium-guest-phone');

  // 7. Rematch from all three: the next vote seats the late joiner.
  for (const p of [host, guest, late]) await tap(p, REMATCH);
  const next = await until(host, loop, x => x.phase === 1 && x.setup.number === l.setup.number, 30000, 'next vote');
  const seated = next.seats.filter(s => !(s.flags & 1));
  assert.equal(seated.length, 3, `three riders: ${JSON.stringify(next.seats)}`);
  assert.ok(seated.every(s => !(s.flags & SPECTATING)), 'nobody watching');
  await until(late, ui, s => s.parked === false, 10000, 'unparked for the next round');
  assert.equal((await mp(late)).code, code, 'the room stays together');
  await until(late, panel, s => s.phase === 'Vote' && s.cards.length === 4, 15000, 'late joiner votes');
  await shot(late, 'pl-rematch-vote-late');
  // The host's Skip ends the vote at once: its own card wins.
  await tap(host, CARD + 1);
  await tap(host, SKIP);
  const skipped = await until(host, loop, x => x.phase >= 2, 10000, 'skipped');
  assert.equal(skipped.setup.round.map, next.options[1].map, 'the host\'s card');
  assertNoErrors(errors);
  console.log(`Private room loop: host lobby, ready tick and 10 s start, vote on the room track, host-driven race, late joiner follow camera, host podium (${l.results.map(x => x.value).join(', ')} ms) and rematch with the late joiner seated verified`);
} finally {
  await browser.close();
}
