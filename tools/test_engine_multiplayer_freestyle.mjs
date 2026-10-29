// Host-only gate: Freestyle in a multiplayer room (research/MULTIPLAYER.md
// "Room Freestyle"). A Rust host opens Main Menu → Multiplayer → Host Game,
// sets Room: Freestyle and rides a course: the room starts a Freestyle
// session whose start holds ("Waiting for riders") until a Rust guest has
// joined by code and loaded the course, then one shared 3-2-1. Each rider
// scores itself (`freestyleTest=score` lands one different two-stunt chain
// each second worth 1000 + 100 x slot through the ride-score tracker) and
// reports to the host, whose table every page shows: the guest leads. A
// third rider joining after GO spectates and is not in the results. Time
// Up (`freestyleTest=fast`: 25 s) shows the same results with every name on
// every page. The host's Play Again starts session 2 for both; the host then
// leaves mid-session and the guest takes the room over and finishes it.
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_multiplayer_freestyle.mjs
// ATV_FREESTYLE_SHOTS=<dir> saves captures.
import assert from 'node:assert/strict';
import { assertNoErrors, base, launch, newPage, nextFrames } from './browser/harness.mjs';

const shots = process.env.ATV_FREESTYLE_SHOTS || '';
const errors = [];
const code = `FS${String(Date.now()).slice(-6)}`;
const ui = p => p.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const ride = async p => (await ui(p)).ride ?? {};
async function until(p, predicate, timeout = 60000, what = '') {
  const deadline = Date.now() + timeout;
  for (;;) {
    const r = await ride(p).catch(() => ({}));
    if (r.roomFreestyle && predicate(r.roomFreestyle, r)) return r;
    if (Date.now() > deadline) throw new Error(`timed out ${what}: ${JSON.stringify(r.roomFreestyle)} errors ${errors.slice(0, 5).join(' | ')}`);
    await p.waitForTimeout(200);
  }
}
async function key(page, k) { await page.keyboard.down(k); await nextFrames(page, 2); await page.keyboard.up(k); await nextFrames(page, 3); }
async function shot(p, name) {
  if (!shots) return;
  await nextFrames(p, 3);
  await p.screenshot({ path: `${shots}/${name}.png` });
}
const flags = 'uiDiagnostics=1&weather=off&freestyleTest=fast&freestyleTest=score';
const browser = await launch();
try {
  const open = async () => {
    const p = await newPage(browser, { viewport: { width: 960, height: 540 } }, errors);
    await p.addInitScript(() => {
      localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
      Object.defineProperty(document, 'hasFocus', { value: () => true });
    });
    return p;
  };
  const host = await open();
  const front = async () => (await ui(host)).front ?? {};
  const onPage = name => host.waitForFunction(n => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === n, name, { timeout: 60000 });
  async function pick(choice) {
    const f = await front();
    const at = f.choices.indexOf(choice);
    assert.ok(at >= 0, `${choice} in ${f.choices}`);
    for (let i = f.selection; i < at; i++) await key(host, 'ArrowDown');
    for (let i = f.selection; i > at; i--) await key(host, 'ArrowUp');
  }
  await host.addInitScript(() => {
    localStorage.setItem('atv-player-selection', JSON.stringify({ quadId: 1, riderId: 0, playerName: 'RustHost' }));
  });
  await host.goto(`${base}/play.html?${flags}`);
  await onPage('title');
  await host.mouse.click(8, 8);
  await key(host, 'Enter');
  await onPage('main');
  await pick('multiplayer');
  await key(host, 'Enter');
  await onPage('multiplayer');
  await pick('create-lobby');
  await key(host, 'Enter');
  await onPage('create-lobby');
  await host.getByLabel('Lobby code (8 letters/numbers, optional)').fill(code.toLowerCase());
  await host.keyboard.press('Enter');
  await nextFrames(host, 3);
  await host.mouse.click(8, 8);
  // Host Game → Room: Freestyle → Select Track → Ride.
  await pick('hostmode');
  await key(host, 'Enter');
  let f = await front();
  assert.equal(f.labels[f.choices.indexOf('hostmode')], 'Room: Freestyle (timed stunts)');
  await shot(host, 'FS-host-form');
  await pick('hosttrack');
  await key(host, 'Enter');
  await onPage('tracks');
  await key(host, 'Enter');
  await host.waitForFunction(c => JSON.parse(window.furyMultiplayer ?? 'null')?.code === c, code, { timeout: 180000 });
  // The session holds its start for an invitee.
  let h = await until(host, s => s.host && s.phase === 0 && s.hold && s.locked, 240000, 'the host holds for riders');
  await nextFrames(host, 30);
  assert.equal((await ride(host)).roomFreestyle.phase, 0, 'still holding with nobody else in the room');
  await shot(host, 'FS-host-waiting');

  const guest = await open();
  await guest.goto(`${base}/play.html?room=${code}&dropAfter=9999&name=RustGuest&${flags}`);
  await guest.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 1, null, { timeout: 180000 });
  await host.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 1, null, { timeout: 60000 });
  let g = await until(guest, s => !s.host && s.serial === h.roomFreestyle.serial, 120000, 'the guest joins the session');
  assert.equal(g.roomFreestyle.me, 1);
  // One shared start, then play: both riders' scores reach the host's table.
  await until(guest, s => s.phase === 1 && !s.spectating, 90000, 'the guest plays');
  h = await until(host, s => s.phase === 1 && s.players === 0b11, 30000, 'the host plays');
  h = await until(host, s => s.rows[0].score >= 2000 && s.rows[1].score >= 2200 && s.rows[1].combos >= 2, 60000, 'the host table fills');
  assert.ok(h.roomFreestyle.accepted >= 2, JSON.stringify(h.roomFreestyle));
  assert.equal(h.roomFreestyle.standings[0].name, 'RustGuest', 'the guest scores more each second');
  g = await until(guest, s => s.rows[0].score >= 2000, 30000, 'the guest mirrors the host score');
  // Remote riders are drawn in the venue.
  assert.ok((await guest.evaluate(() => JSON.parse(window.furyMultiplayer).drawn)) >= 1);
  await shot(guest, 'FS-guest-play');
  await shot(host, 'FS-host-play');

  // A rider joining after GO spectates and is not in the results.
  const late = await open();
  await late.goto(`${base}/play.html?room=${code}&dropAfter=9999&name=RustLate&uiDiagnostics=1&weather=off`);
  const l = await until(late, s => s.serial === h.roomFreestyle.serial && s.spectating, 240000, 'the late rider spectates');
  assert.equal(l.roomFreestyle.me, 2);
  assert.equal((await ride(host)).roomFreestyle.players, 0b11, 'the players are fixed at GO');
  await shot(late, 'FS-late-spectator');

  // Time Up: the same final table and results on every page.
  for (const p of [host, guest, late]) {
    await until(p, s => s.phase === 3 && s.reported, 120000, 'results');
    await p.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride?.results?.length >= 1, null, { timeout: 60000 });
  }
  h = await ride(host);
  g = await ride(guest);
  const ll = await ride(late);
  assert.equal(g.roomFreestyle.summary, h.roomFreestyle.summary, 'agreed results');
  assert.equal(ll.roomFreestyle.summary, h.roomFreestyle.summary, 'the spectator sees the same results');
  assert.match(h.roomFreestyle.summary, /^Freestyle on .*: Time Up\. 1\. RustGuest: Score [\d,]+, Best Combo 1,100, Combos \d+\. 2\. RustHost: Score [\d,]+, Best Combo 1,000/);
  assert.doesNotMatch(h.roomFreestyle.summary, /RustLate/);
  // The course by its Tracks.dat name on every page, not its venue code.
  assert.doesNotMatch(g.roomFreestyle.summary, /^Freestyle on (env|free|nat|sx|train)\w*:/);
  await shot(host, 'FS-host-results');
  await shot(guest, 'FS-guest-results');
  const first = h.roomFreestyle.summary;
  await late.close();

  // Results → Continue → Play Again: session 2 for both.
  await host.bringToFront();
  await key(host, 'Enter');
  await key(host, 'Enter');
  const serial = h.roomFreestyle.serial;
  h = await until(host, s => s.serial === serial + 1 && s.phase === 1, 120000, 'session 2 starts');
  await until(guest, s => s.serial === serial + 1 && s.phase === 1 && !s.spectating, 60000, 'the guest rides session 2');
  // The host leaves: the guest takes the room over and finishes the session.
  await host.close();
  await guest.bringToFront();
  g = await until(guest, s => s.host && !s.present[0], 90000, 'the guest takes over');
  g = await until(guest, s => s.phase === 3 && s.reported, 120000, 'session 2 results');
  assert.equal(g.roomFreestyle.serial, serial + 1, 'the same session finishes');
  assert.match(g.roomFreestyle.summary, /RustGuest/);
  assert.doesNotMatch(g.roomFreestyle.summary, /RustHost/, 'the leaver is dropped');
  assert.equal(g.roomFreestyle.standings.length, 1);
  // The handoff does not stretch the 25 s session (one chain a second).
  assert.ok(g.roomFreestyle.standings[0].combos <= 27, JSON.stringify(g.roomFreestyle.standings));
  await shot(guest, 'FS-after-host-left');
  assertNoErrors(errors);
  console.log(`Multiplayer Freestyle: menu-hosted held start, shared table and results (${first}), a spectating late joiner, Play Again and a host handoff (${g.roomFreestyle.summary}) verified`);
} finally {
  await browser.close();
}
