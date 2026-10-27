// The original mini-games (research/GAMES.md): Single Player → Games plays
// Treasure Hunt and King of the Hills from their game archives' mode TOYs.
// A new profile is refused both locked games (0021add0) and the locked
// courses (Tracks.dat access); with the games bought, Treasure Hunt on
// Prospect Springs shows group 1's five treasures, `gameTest=visit` sets
// the ATV on each standing treasure so every group (with its 6 s "More
// Treasures in" wait) is collected through the real triggers, and "All
// Treasures Collected" ends it with the results; Restart Game (results)
// starts it over and Continue returns to the Games page. King of the Hills
// on Tioga Valley: `gameTest=hill` sets the ATV on hill 1 and lifts it off
// the hill through the real trigger enter/leave (armed), and a landed
// 1,500-point chain supplied by the hook captures it ("ATV 1 captured hill
// 1"; an ATV placed in the air opens no stunt of its own); Pause → Restart Game frees it again, and the
// 20 s clock of `gameTest=fast` ends the game with Time Up.
// ATV_GAMES_SHOTS=<dir> saves HUD and results captures.
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const shots = process.env.ATV_GAMES_SHOTS || '';
const state = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const game = async page => (await state(page)).ride?.game;
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
    const g = await game(page);
    if (g && predicate(g)) return g;
    if (Date.now() > deadline) throw new Error(`timed out: ${predicate} ${JSON.stringify(g)}`);
    await page.waitForTimeout(100);
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
async function select(page, action) {
  const front = (await state(page)).front;
  const at = front.choices.indexOf(action);
  for (let i = front.selection; i < at; i++) await key(page, 'ArrowDown');
  for (let i = front.selection; i > at; i--) await key(page, 'ArrowUp');
}
const label = async (page, action) => {
  const front = (await state(page)).front;
  return front.labels[front.choices.indexOf(action)];
};
async function toGames(page, query) {
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
  await choose(page, 'single');
  await onPage(page, 'single');
  const front = (await state(page)).front;
  const at = front.choices.indexOf('games');
  assert.ok(at > 0, `Games row in ${front.choices}`);
  assert.equal(front.labels[at], 'Games');
  await choose(page, 'games');
  await onPage(page, 'games');
}
async function setGame(page, name) {
  await select(page, 'gametype');
  for (let i = 0; i < 4 && !(await label(page, 'gametype')).startsWith(`Game: ${name}`); i++) await key(page, 'ArrowRight');
  assert.ok((await label(page, 'gametype')).startsWith(`Game: ${name}`));
}
async function start(page, venue) {
  await choose(page, 'gamestart');
  await page.waitForFunction(
    venue => {
      const s = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}');
      return s.ride?.environment === venue && s.ride?.game?.loaded;
    },
    venue,
    { timeout: 300000 }
  );
}
async function results(page) {
  await page.waitForFunction(
    () => (JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride?.results?.length ?? 0) === 2,
    null,
    { timeout: 120000 }
  );
}

if (shots) await mkdir(shots, { recursive: true });
const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await page.addInitScript(() => {
    if (sessionStorage.getItem('games-test')) return;
    sessionStorage.setItem('games-test', '1');
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.setItem('fury-profile', '{}');
    localStorage.setItem('atv-player-selection', JSON.stringify({ quadId: 1, riderId: 0 }));
  });

  // A new profile: both games are locked (0021add0).
  await toGames(page, 'gameTest=visit');
  let front = (await state(page)).front;
  assert.deepEqual(front.choices, ['gametype', 'gamecourse', 'gametime', 'gamestart', 'back']);
  assert.equal(front.labels[0], 'Game: Treasure Hunt (Locked)');
  assert.equal(front.labels[1], 'Course: Prospect Springs');
  assert.equal(front.labels[2], 'Time Limit: 5:00');
  await choose(page, 'gamestart');
  front = (await state(page)).front;
  assert.equal(front.page, 'games');
  assert.match(front.notice ?? '', /To unlock this game\.\.\. Spend 500 Profile Points/);
  await shot(page, 'games-locked');

  // Buy both games; the time limit steps 2-20 minutes; a locked course is refused.
  await page.evaluate(() => localStorage.setItem('fury-profile', JSON.stringify({ store: { events: ['th', 'koth'] } })));
  await toGames(page, 'gameTest=visit');
  assert.equal(await label(page, 'gametype'), 'Game: Treasure Hunt');
  await select(page, 'gametime');
  await key(page, 'ArrowRight');
  assert.equal(await label(page, 'gametime'), 'Time Limit: 6:00');
  await key(page, 'ArrowLeft');
  await select(page, 'gamecourse');
  await key(page, 'ArrowRight');
  await key(page, 'ArrowRight');
  assert.equal(await label(page, 'gamecourse'), 'Course: Arlington Stream (Locked)');
  await choose(page, 'gamestart');
  assert.match((await state(page)).front.notice ?? '', /To unlock this course/);
  await select(page, 'gamecourse');
  await key(page, 'ArrowLeft');
  await key(page, 'ArrowLeft');
  assert.equal(await label(page, 'gamecourse'), 'Course: Prospect Springs');
  await shot(page, 'games-page');

  // Treasure Hunt on Prospect Springs: 25 treasures in five groups.
  await start(page, 'env05');
  let g = await game(page);
  assert.equal(g.kind, 'treasure');
  assert.equal(g.minutes, 5);
  assert.equal(g.rules.targets, 25);
  assert.equal(g.rules.groups, 5);
  assert.equal(g.markers.length, 5, 'group 1 shows five treasures');
  await until(page, g => g.started);
  await shot(page, 'treasure-start');
  g = await until(page, g => g.rules.wait > 0, 60000);
  assert.equal(g.scores[0], 5, 'group 1 collected');
  assert.equal(g.markers.length, 0, 'none shown while waiting');
  assert.equal(g.rules.remaining, 20);
  await shot(page, 'treasure-more');
  g = await until(page, g => g.rules.wait === 0 && g.markers.length === 5, 15000);
  assert.notEqual(g.rules.group, 0, 'a random unused group follows');
  g = await until(page, g => g.end === 'Complete', 180000);
  assert.equal(g.notice, 'All Treasures Collected');
  assert.equal(g.scores[0], 25);
  assert.equal(g.rules.usedGroups, 5);
  await shot(page, 'treasure-all');
  await results(page);
  g = await game(page);
  assert.equal(g.over, true);
  await shot(page, 'treasure-results');
  // Restart Game from the results: everything starts over.
  await key(page, 'Enter');
  g = await until(page, g => !g.over && g.scores[0] === 0 && g.markers.length === 5, 20000);
  assert.equal(g.rules.group, 0);
  assert.equal((await state(page)).ride?.results ?? null, null);
  g = await until(page, g => g.scores[0] >= 1, 30000);

  // King of the Hills on Tioga Valley: capture hill 1 with a stunt.
  await toGames(page, 'gameTest=hill');
  await setGame(page, 'King of the Hills');
  await select(page, 'gamecourse');
  await key(page, 'ArrowRight');
  assert.equal(await label(page, 'gamecourse'), 'Course: Tioga Valley');
  await start(page, 'env02');
  g = await game(page);
  assert.equal(g.kind, 'koth');
  assert.equal(g.rules.hills, 5);
  assert.equal(g.markers.length, 5);
  assert.ok(g.markers.every(m => m.owner === null));
  // Armed off hill 1 in the air (the real trigger enter and leave); the
  // hook lands a 1,500-point chain, which beats the hill's 0 (00220b50).
  g = await until(page, g => g.markers[0].owner === 0, 60000);
  assert.equal(g.scores[0], 1);
  assert.equal(g.markers[0].score, 1500);
  assert.equal(g.notice, 'ATV 1 captured hill 1');
  await shot(page, 'koth-captured');
  // Pause → Restart Game (0043a9e8, 00220d10): every hill unowned again.
  await key(page, 'Escape');
  await key(page, 'ArrowDown');
  await key(page, 'Enter');
  g = await until(page, g => g.scores[0] === 0 && g.markers.every(m => m.owner === null), 20000);

  // Time Up on the 20 s clock: the results, Continue to the Games page.
  await toGames(page, 'gameTest=fast');
  await setGame(page, 'King of the Hills');
  await start(page, 'env05');
  g = await until(page, g => g.started);
  await shot(page, 'koth-hud');
  g = await until(page, g => g.over, 60000);
  assert.equal(g.end, 'TimeUp');
  await results(page);
  await shot(page, 'koth-results');
  await key(page, 'ArrowDown');
  await key(page, 'Enter');
  await onPage(page, 'games', 120000);
  assertNoErrors(errors);
  console.log('games: Treasure Hunt and King of the Hills played end to end');
} finally {
  await browser.close();
}
