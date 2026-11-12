// Authored CPU riders in the local mini-games (research/GAMES.md "CPU
// riders", a port adaptation): Single Player → Games offers CPU Riders
// Off / 1 / 2 / 3 (none for an online room), and each CPU is a whole ride
// that scores through the game's own rules:
// - Treasure Hunt on Prospect Springs with two CPUs: a CPU collects a
//   treasure (its own TOKENS count), and the CPUs ride to treasures;
// - Hockey with one CPU: the CPU skates Blue, held at its face-off spot,
//   and chases the dropped puck off centre;
// - Tag on Prospect Springs with two CPUs (no room): the CPUs chase the
//   ball, a CPU takes it, and the results list the CPU riders.
// King of the Hills CPUs run hills too, but a capture needs a real airborne
// exit, so this test only checks that they ride to a hill.
// ATV_GAME_CPUS_SHOTS=<dir> saves captures.
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const shots = process.env.ATV_GAME_CPUS_SHOTS || '';
const state = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const ride = async page => (await state(page)).ride;
const onPage = (page, name, timeout = 60000) =>
  page.waitForFunction(
    name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name,
    name,
    { timeout }
  );
async function key(page, code) {
  await page.keyboard.down(code);
  await nextFrames(page, 2);
  await page.keyboard.up(code);
  await nextFrames(page, 3);
}
async function until(page, predicate, timeout = 60000) {
  const deadline = Date.now() + timeout;
  for (;;) {
    const r = await ride(page);
    if (r && predicate(r)) return r;
    if (Date.now() > deadline) throw new Error(`timed out: ${predicate} ${JSON.stringify(r?.cpus)}`);
    await page.waitForTimeout(100);
  }
}
async function shot(page, name) {
  if (!shots) return;
  await nextFrames(page, 3);
  await page.screenshot({ path: `${shots}/${name}.png` });
}
async function select(page, action) {
  const front = (await state(page)).front;
  const at = front.choices.indexOf(action);
  assert.ok(at >= 0, `${action} in ${front.choices}`);
  for (let i = front.selection; i < at; i++) await key(page, 'ArrowDown');
  for (let i = front.selection; i > at; i--) await key(page, 'ArrowUp');
}
const label = async (page, action) => {
  const front = (await state(page)).front;
  return front.labels[front.choices.indexOf(action)];
};
async function toGames(page, query = '') {
  await page.goto(url(`/play.html?uiDiagnostics=1&weather=off&${query}`));
  await onPage(page, 'title');
  await page.mouse.click(8, 8);
  await key(page, 'Enter');
  await onPage(page, 'main');
  await page.waitForFunction(
    () => (JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.store?.atvs ?? 0) > 20,
    null,
    { timeout: 60000 }
  );
  await select(page, 'single');
  await key(page, 'Enter');
  await onPage(page, 'single');
  await select(page, 'games');
  await key(page, 'Enter');
  await onPage(page, 'games');
}
async function setGame(page, name) {
  await select(page, 'gametype');
  for (let i = 0; i < 4 && !(await label(page, 'gametype')).startsWith(`Game: ${name}`); i++) await key(page, 'ArrowRight');
  assert.ok((await label(page, 'gametype')).startsWith(`Game: ${name}`));
}
async function setCpus(page, n) {
  await select(page, 'gamecpus');
  for (let i = 0; i < 4 && (await label(page, 'gamecpus')) !== `CPU Riders: ${n}`; i++) await key(page, 'ArrowRight');
  assert.equal(await label(page, 'gamecpus'), `CPU Riders: ${n}`);
}
async function start(page, ready) {
  await select(page, 'gamestart');
  await key(page, 'Enter');
  await until(page, ready, 300000);
}
const flat = (a, b) => Math.hypot(a[0] - b[0], a[2] - b[2]);

if (shots) await mkdir(shots, { recursive: true });
const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await page.addInitScript(() => {
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.setItem('fury-profile', JSON.stringify({ store: { events: ['th', 'koth'] } }));
    localStorage.setItem('atv-player-selection', JSON.stringify({ quadId: 1, riderId: 0 }));
  });

  // The setup row: Off by default, 1-3, gone for an online room.
  await toGames(page);
  assert.equal(await label(page, 'gametype'), 'Game: Treasure Hunt');
  assert.equal(await label(page, 'gamecpus'), 'CPU Riders: Off');
  await select(page, 'gameriders');
  await key(page, 'ArrowRight');
  assert.ok(!(await state(page)).front.choices.includes('gamecpus'), 'no CPU riders online');
  await key(page, 'ArrowRight');
  await setCpus(page, 2);

  // Treasure Hunt: the CPUs ride to the treasures and score through the
  // real triggers.
  await start(page, r => r.environment === 'env05' && r.game?.loaded && r.cpus?.length === 2);
  let r = await until(page, r => r.game.started);
  assert.equal(r.game.scores.length, 3, 'three riders in the rules');
  const startAt = r.cpus.map(c => c.origin);
  r = await until(page, r => r.cpus.every((c, i) => flat(c.origin, startAt[i]) > 30), 30000);
  await shot(page, 'treasure-cpus');
  r = await until(page, r => r.game.scores[1] + r.game.scores[2] >= 1, 150000);
  console.log(`Treasure Hunt scores ${JSON.stringify(r.game.scores)}`);
  assert.equal(r.game.scores[0], 0, 'the idle player collected nothing');

  // King of the Hills: the CPUs head for a hill.
  await toGames(page);
  await setGame(page, 'King of the Hills');
  await setCpus(page, 1);
  await start(page, r => r.game?.loaded && r.cpus?.length === 1);
  r = await until(page, r => r.game.started);
  const hills = r.game.markers.map(m => m.position);
  const near = c => Math.min(...hills.map(h => flat(h, c.origin)));
  const before = near(r.cpus[0]);
  r = await until(page, r => near(r.cpus[0]) < Math.min(before - 40, 60), 60000);
  console.log(`King of the Hills: CPU within ${near(r.cpus[0]).toFixed(0)} ft of a hill`);

  // Hockey: one CPU skates Blue and chases the dropped puck.
  await toGames(page, 'hockeyTest=fast');
  await setGame(page, 'Hockey');
  await setCpus(page, 1);
  await start(page, r => r.environment === 'game01' && r.hockey && r.cpus?.length === 1);
  r = await until(page, r => r.hockey.state.startsWith('FaceOff'));
  assert.deepEqual(r.hockey.teams, [0, 1]);
  assert.ok(r.cpus[0].locked, 'held at the face-off');
  const faceOff = r.cpus[0].origin;
  r = await until(page, r => r.hockey.state === 'Play', 30000);
  r = await until(page, r => flat(r.cpus[0].origin, faceOff) > 10 && r.hockey.puckSpeed > 0.5, 20000);
  await shot(page, 'hockey-cpu');
  console.log(`Hockey: CPU off its spot, puck at ${r.hockey.puck.map(v => v.toFixed(0))}`);

  // Tag with two CPUs, played locally: a CPU takes the ball and the
  // results name the CPU riders.
  await toGames(page, 'tagTest=fast');
  await setGame(page, 'Tag');
  await setCpus(page, 2);
  await start(page, r => r.tag?.local && r.cpus?.length === 2);
  r = await until(page, r => r.tag.carrier !== null && r.tag.carrier > 0, 30000);
  await shot(page, 'tag-cpu');
  r = await until(page, r => r.tag.finished, 60000);
  console.log(`Tag: ${r.tag.summary}`);
  assert.match(r.tag.summary, /CPU 1/);
  assert.ok(r.tag.scores[1] + r.tag.scores[2] > 0, `CPU tag scores ${r.tag.scores}`);
  assertNoErrors(errors);
  console.log('CPU riders: setup row, Treasure Hunt, King of the Hills, Hockey and Tag verified');
} finally {
  await browser.close();
}
