// Host-only gate: Hockey in a multiplayer room (research/GAMES.md
// "Online"). A Rust host rides a room on Coonce Arena (`?host=1&venue=game01`)
// and runs the game; a Rust guest joins by code. The host's state reaches
// the guest over the relay: both see the same score, clock and puck, the
// guest faces off at the Blue spot, and a goal (`hockeyGoal=1` on the host
// slides the first dropped puck into goal 1) shows on both. A 20 s clock
// (`hockeyTest=fast`) runs both to the Hockey Results page.
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_multiplayer_hockey.mjs
// ATV_GAMES_SHOTS=<dir> saves captures.
import assert from 'node:assert/strict';
import { assertNoErrors, base, launch, newPage, nextFrames } from './browser/harness.mjs';

const shots = process.env.ATV_GAMES_SHOTS || '';
const errors = [];
const code = `HK${String(Date.now()).slice(-6)}`;
const ui = p => p.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const hockey = async p => (await ui(p)).ride?.hockey;
async function until(p, predicate, timeout = 60000, label = '') {
  const deadline = Date.now() + timeout;
  for (;;) {
    const h = await hockey(p);
    if (h && predicate(h)) return h;
    if (Date.now() > deadline) throw new Error(`timed out ${label}: ${predicate} ${JSON.stringify(h)}`);
    await p.waitForTimeout(100);
  }
}
async function shot(p, name) {
  if (!shots) return;
  await nextFrames(p, 3);
  await p.screenshot({ path: `${shots}/${name}.png` });
}
const flat = (a, b) => Math.hypot(a[0] - b[0], a[2] - b[2]);

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
  await host.goto(`${base}/play.html?host=1&venue=game01&room=${code}&name=RustHost&uiDiagnostics=1&hockeyGoal=1&hockeyTest=fast&weather=off`);
  await host.waitForFunction(c => JSON.parse(window.furyMultiplayer ?? 'null')?.code === c, code, { timeout: 180000 });
  await until(host, h => h.roster?.length === 1, 120000, 'host game');

  const guest = await open();
  await guest.goto(`${base}/play.html?room=${code}&dropAfter=9999&name=RustGuest&uiDiagnostics=1&weather=off`);
  // The host rebuilds the game for two riders and faces off again.
  let h = await until(host, h => h.roster?.length === 2, 180000, 'host roster');
  assert.deepEqual(h.teams, [0, 1]);
  let g = await until(guest, g => g.roster?.length === 2 && g.serial !== 4294967295, 180000, 'guest state');
  assert.equal(g.me, 1);
  assert.deepEqual(g.teams, [0, 1]);
  // The guest faced off at the Blue spot (z 414) once it heard a face-off.
  g = await until(guest, g => g.state.startsWith('FaceOff') || g.state === 'Play', 60000, 'guest phase');
  await until(guest, g => flat(g.origin, [384, 0, 414]) < 4 || g.scores[0] > 0, 60000, 'blue spot');
  await shot(guest, 'mp-hockey-guest-face-off');
  // The host's goal reaches the guest.
  h = await until(host, h => h.scores[0] === 1, 90000, 'host goal');
  g = await until(guest, g => g.scores[0] === 1, 30000, 'guest goal');
  assert.equal(g.banner === 'Red scores!' || g.state.startsWith('Pause') || g.state.startsWith('FaceOff'), true, JSON.stringify(g));
  await shot(guest, 'mp-hockey-guest-goal');
  await shot(host, 'mp-hockey-host-goal');
  // The puck the guest draws follows the host's.
  h = await hockey(host);
  g = await hockey(guest);
  assert.ok(flat(h.puck, g.puck) < 15, `puck ${h.puck} vs ${g.puck}`);
  // The clock runs out on both: Hockey Results.
  for (const p of [host, guest]) {
    await p.waitForFunction(() => {
      const ride = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride;
      return ride?.hockey?.finished && ride?.results?.length === 2;
    }, null, { timeout: 120000 });
  }
  g = await hockey(guest);
  assert.deepEqual(g.scores, [1, 0]);
  await shot(guest, 'mp-hockey-guest-results');
  assertNoErrors(errors);
  console.log('Multiplayer Hockey: host state, guest face-off, shared goal and results verified');
} finally {
  await browser.close();
}
