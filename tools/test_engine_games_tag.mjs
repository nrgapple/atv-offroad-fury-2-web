// Relay gate: Single Player → Games → Tag (research/GAMES.md "Tag",
// "Games page"). With Tag bought, the Games page lists its Tracks.dat
// courses (a locked one is refused); Start Game on Prospect Springs with a
// 3:00 Time Limit hosts a multiplayer room, moves its ride onto the
// original Tag course (E05_TG.RES: the mode TOY's start, `tagball.psg`) and
// starts Tag with those minutes. A guest joins by the room's code, follows
// onto the Tag course and both see the same two-rider game.
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_games_tag.mjs
// ATV_GAMES_SHOTS=<dir> saves captures.
import assert from 'node:assert/strict';
import { assertNoErrors, base, launch, newPage, nextFrames } from './browser/harness.mjs';

const shots = process.env.ATV_GAMES_SHOTS || '';
const errors = [];
const state = p => p.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const onPage = (p, name, timeout = 60000) =>
  p.waitForFunction(
    name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name,
    name,
    { timeout }
  );
async function key(p, code) {
  await p.keyboard.down(code);
  await nextFrames(p, 2);
  await p.keyboard.up(code);
  await nextFrames(p, 3);
}
async function select(p, action) {
  const front = (await state(p)).front;
  const at = front.choices.indexOf(action);
  assert.ok(at >= 0, `${action} in ${front.choices}`);
  for (let i = front.selection; i < at; i++) await key(p, 'ArrowDown');
  for (let i = front.selection; i > at; i--) await key(p, 'ArrowUp');
}
async function choose(p, action) {
  await select(p, action);
  await key(p, 'Enter');
}
const label = async (p, action) => {
  const front = (await state(p)).front;
  return front.labels[front.choices.indexOf(action)];
};
async function untilTag(p, predicate, timeout, what) {
  const deadline = Date.now() + timeout;
  for (;;) {
    const ride = (await state(p)).ride;
    if (ride?.tag && predicate(ride)) return ride;
    if (Date.now() > deadline) throw new Error(`timed out (${what}): ${JSON.stringify(ride?.tag)}`);
    await p.waitForTimeout(100);
  }
}
async function shot(p, name) {
  if (!shots) return;
  await nextFrames(p, 3);
  await p.screenshot({ path: `${shots}/${name}.png` });
}

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
  await host.addInitScript(() => {
    if (sessionStorage.getItem('games-tag-test')) return;
    sessionStorage.setItem('games-tag-test', '1');
    localStorage.setItem('fury-profile', JSON.stringify({ store: { events: ['tag'] } }));
    localStorage.setItem('atv-player-selection', JSON.stringify({ quadId: 1, riderId: 0 }));
  });
  await host.goto(`${base}/play.html?uiDiagnostics=1&weather=off`);
  await onPage(host, 'title');
  await host.mouse.click(8, 8);
  await key(host, 'Enter');
  await onPage(host, 'main');
  await host.waitForFunction(
    () => (JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.store?.atvs ?? 0) > 20,
    null,
    { timeout: 60000 }
  );
  await choose(host, 'single');
  await onPage(host, 'single');
  await choose(host, 'games');
  await onPage(host, 'games');
  await select(host, 'gametype');
  for (let i = 0; i < 4 && !(await label(host, 'gametype')).startsWith('Game: Tag'); i++) await key(host, 'ArrowRight');
  assert.ok((await label(host, 'gametype')).startsWith('Game: Tag'), await label(host, 'gametype'));
  assert.equal(await label(host, 'gamecourse'), 'Course: Prospect Springs');
  // Tracks.dat [tag1]/[tag2] are open; [tag3] is locked on a new profile.
  await select(host, 'gamecourse');
  await key(host, 'ArrowRight');
  assert.equal(await label(host, 'gamecourse'), 'Course: Tioga Valley');
  await key(host, 'ArrowRight');
  assert.match(await label(host, 'gamecourse'), /\(Locked\)$/);
  await choose(host, 'gamestart');
  assert.equal((await state(host)).front.page, 'games', 'a locked Tag course is refused');
  await select(host, 'gamecourse');
  await key(host, 'ArrowLeft');
  await key(host, 'ArrowLeft');
  assert.equal(await label(host, 'gamecourse'), 'Course: Prospect Springs');
  await select(host, 'gametime');
  await key(host, 'ArrowLeft');
  await key(host, 'ArrowLeft');
  assert.equal(await label(host, 'gametime'), 'Time Limit: 3:00');
  await shot(host, 'games-tag-page');
  await choose(host, 'gamestart');

  // The hosted room moves onto the Tag course and starts Tag.
  const ride = await untilTag(host, r => r.tagBall && r.tag.model, 300000, 'host on the Tag course');
  assert.equal(ride.environment, 'env05');
  assert.equal(ride.tag.minutes, 3);
  assert.equal(ride.tag.timeLimit, 180);
  assert.equal(ride.tag.host, true);
  assert.deepEqual(ride.start.map(v => Math.round(v * 100) / 100), [1839.75, 135, 3389.18]);
  const code = await host.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.code, null, { timeout: 60000 }).then(h => h.jsonValue());
  assert.ok(code, 'the room has a code');

  const guest = await open();
  await guest.goto(`${base}/play.html?room=${code}&dropAfter=9999&name=TagGuest&uiDiagnostics=1&weather=off`);
  const g = await untilTag(guest, r => r.tagBall && r.tag.model && r.tag.roster.length === 2, 300000, 'guest joins');
  assert.equal(g.tag.me, 1);
  assert.equal(g.environment, 'env05');
  await untilTag(host, r => r.tag.roster.length === 2, 60000, 'host sees the guest');
  // The clock runs down on both pages from the 3:00 limit.
  const h = await untilTag(host, r => r.tag.clock < 178, 30000, 'host clock');
  assert.ok(h.tag.clock <= 180);
  await shot(guest, 'games-tag-guest');
  assertNoErrors(errors);
  console.log(`Games → Tag: room hosted on E05_TG.RES with a 3:00 limit; guest joined (code ${code})`);
} finally {
  await browser.close();
}
