// The original Hockey mini-game (research/GAMES.md): Main Menu → Hockey
// rides Coonce Arena. The rider is held at the face-off for 2 s, the puck
// drops, and `hockeyGoal=1` slides the first dropped puck off the rider into
// the far goal: the score and the scorer's goal count rise, "Red scores!" shows,
// and after the 4 s pause the rider is back at the face-off. Pause → Restart
// Game zeroes the score; `hockeyTest=fast` (a 20 s clock) then runs the game
// to "Game over!", the HockeyOver results page (Quit to Main Menu) and the
// result on the main menu. ATV_HOCKEY_SOUNDS=1 also requires the hockey
// one-shots (a content package with them). Keyboard driving moves the
// rider off the face-off once the puck drops. Tag's rules are core-only
// (crates/atv-core/src/tag.rs unit tests) until its CPU riders land.
// ATV_GAMES_SHOTS=<dir> saves HUD captures.
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const shots = process.env.ATV_GAMES_SHOTS || '';
const state = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const hockey = async page => (await state(page)).ride?.hockey;
const onPage = (page, name, timeout = 60000) => page.waitForFunction(name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name, name, { timeout });
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }
async function until(page, predicate, timeout = 60000) {
  const deadline = Date.now() + timeout;
  for (;;) {
    const h = await hockey(page);
    if (h && predicate(h)) return h;
    if (Date.now() > deadline) throw new Error(`timed out: ${predicate} ${JSON.stringify(h)}`);
    await page.waitForTimeout(50);
  }
}
async function shot(page, name) {
  if (!shots) return;
  await nextFrames(page, 3);
  await page.screenshot({ path: `${shots}/${name}.png` });
}
async function choose(page, action) {
  const front = (await state(page)).front;
  const at = front.choices.indexOf(action);
  assert.ok(at >= 0, `${action} in ${front.choices}`);
  for (let i = front.selection; i < at; i++) await key(page, 'ArrowDown');
  for (let i = front.selection; i > at; i--) await key(page, 'ArrowUp');
  await key(page, 'Enter');
}
async function toMain(page, query) {
  await page.goto(url(`/play.html?uiDiagnostics=1&weather=off&${query}`));
  await onPage(page, 'title');
  await page.mouse.click(8, 8);
  await key(page, 'Enter'); await onPage(page, 'main');
}
async function startHockey(page) {
  const front = (await state(page)).front;
  const at = front.choices.indexOf('hockey');
  assert.ok(at > 0, `Hockey row in ${front.choices}`);
  assert.equal(front.labels[at], 'Hockey');
  assert.equal(front.choices[at + 1], 'training', 'Hockey follows the ride modes');
  await choose(page, 'hockey');
  await page.waitForFunction(() => {
    const s = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}');
    return s.ride?.environment === 'game01' && s.ride?.hockey;
  }, null, { timeout: 300000 });
}
async function pauseChoose(page, action) {
  await key(page, 'Escape');
  await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').toolbar?.menu?.page === 'pause', null, { timeout: 30000 });
  const menu = (await state(page)).toolbar.menu;
  const at = menu.choices.indexOf(action);
  assert.ok(at >= 0, `${action} in ${menu.choices}`);
  for (let i = 0; i < at; i++) await key(page, 'ArrowDown');
  await key(page, 'Enter');
  return menu;
}
const FACE_OFF = [384, 90.1, 354];
const flat = (a, b) => Math.hypot(a[0] - b[0], a[2] - b[2]);

if (shots) await mkdir(shots, { recursive: true });
const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await page.addInitScript(() => {
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
  });

  // A real face-off, drop and keyboard drive.
  await toMain(page, '');
  await startHockey(page);
  let h = await until(page, h => h.state.startsWith('FaceOff'));
  assert.deepEqual(h.scores, [0, 0]);
  assert.equal(h.clock, 120);
  assert.ok(flat(h.origin, FACE_OFF) < 3, `held at the Red face-off spot ${h.origin}`);
  await shot(page, 'hockey-face-off');
  h = await until(page, h => h.state === 'Play', 30000);
  assert.ok(h.puck[1] > 90 && flat(h.puck, [384, 0, 384]) <= 10.5, `puck dropped near centre ${h.puck}`);
  await page.keyboard.down('ArrowUp');
  await page.waitForTimeout(1500);
  await page.keyboard.up('ArrowUp');
  h = await hockey(page);
  assert.ok(h.clock < 120, `clock runs in play: ${h.clock}`);
  await shot(page, 'hockey-play');
  await pauseChoose(page, 'main');
  await onPage(page, 'main', 60000);

  // A goal off the rider's stick (`hockeyGoal=1`, the first drop), the pause and the next
  // face-off, then Restart Game.
  await toMain(page, 'hockeyGoal=1');
  await startHockey(page);
  h = await until(page, h => h.scores[0] === 1, 60000);
  assert.deepEqual(h.goals, [1]);
  assert.equal(h.banner, 'Red scores!');
  assert.ok(h.state.startsWith('Pause'), h.state);
  assert.ok(h.log.some(l => l.startsWith('Goal')), JSON.stringify(h.log));
  assert.ok(h.log.includes('Siren'));
  await shot(page, 'hockey-goal');
  h = await until(page, h => h.state.startsWith('FaceOff'), 10000);
  assert.equal(h.scores[0], 1);
  const menu = await pauseChoose(page, 'reset');
  assert.equal(menu.labels?.[menu.choices.indexOf('reset')] ?? 'Restart Game', 'Restart Game');
  h = await until(page, h => h.scores[0] === 0 && h.clock === 120, 10000);
  await pauseChoose(page, 'main');
  await onPage(page, 'main', 60000);

  // To the end: a 20 s clock, one goal, Game over! and the HockeyOver
  // result on the main menu.
  await toMain(page, 'hockeyGoal=1&hockeyTest=fast');
  await startHockey(page);
  h = await until(page, h => h.banner === 'Game over!', 120000);
  assert.deepEqual(h.scores, [1, 0]);
  assert.ok(h.log.includes('GameOverSound') && h.log.includes('Crowd(4)'), JSON.stringify(h.log));
  await shot(page, 'hockey-game-over');
  // The HockeyOver page: Restart Game and Quit to Main Menu.
  await page.waitForFunction(() => {
    const ride = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride;
    return ride?.hockey?.finished && ride?.results?.length === 2;
  }, null, { timeout: 30000 });
  await shot(page, 'hockey-results');
  // The original sounds play when the hosted package carries them
  // (siren02, gameover and Crowd\hockey; tools/package_content.py).
  const shots_ = (await state(page)).ride.oneShots ?? [];
  if (process.env.ATV_HOCKEY_SOUNDS === '1') {
    for (const name of ['siren02.vag', 'gameover.vag', 'crowd/hockey/hockyend.vag']) {
      assert.ok(shots_.includes(name), `${name} in ${shots_}`);
    }
  }
  console.log(`one-shots: ${shots_.join(', ')}`);
  await key(page, 'ArrowDown');
  await key(page, 'Enter');
  await onPage(page, 'main', 60000);
  const notice = (await state(page)).front?.notice ?? '';
  assert.equal(notice, 'Hockey: Red 1 – 0 Blue. Red wins! Your goals: 1.');
  await shot(page, 'hockey-result');
  assertNoErrors(errors);
  console.log('Hockey: face-off, drop, goal, pause, restart and result verified');
} finally {
  await browser.close();
}
