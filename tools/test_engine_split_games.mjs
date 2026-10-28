// Split-screen Games (research/SPLIT_SCREEN.md "Slice 3") in /play.html:
// Main Menu → Multiplayer → Split Screen → Games opens the Games page for
// two local players, and Start Game rides both in one game:
// - Treasure Hunt on Prospect Springs (`gameTest=visit` sets the players
//   on the next standing treasure in turn): each treasure counts for the
//   player who touched it, each view's panel reads its own TOKENS and
//   PLACE, both players start at the TOY's own start slots, and the
//   results list both players;
// - King of the Hills on Tioga Valley, side by side (`gameTest=hill2`
//   arms hill 1 for player 2 through the real trigger enter/leave and
//   lands a 1,500-point chain for it): "ATV 2 captured hill 1", player 2
//   holds it in its own colour, player 1 holds none;
// - Hockey: player 1 Red and player 2 Blue at their own face-off spots, to
//   results listing both players' goals.
// ATV_SPLIT_GAMES_SHOTS=<dir> saves captures. Host Chromium evidence.
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const SHOTS = process.env.ATV_SPLIT_GAMES_SHOTS || '';
const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const game = async page => (await read(page)).ride?.game;
const wait = (page, predicate, arg, timeout = 120000) => page.waitForFunction(predicate, arg, { timeout });
const onFront = (page, name) =>
  wait(page, name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name, name);
async function key(page, code) {
  await page.keyboard.down(code);
  await nextFrames(page, 2);
  await page.keyboard.up(code);
  await nextFrames(page, 3);
}
async function select(page, action) {
  const f = (await read(page)).front;
  const at = f.choices.indexOf(action);
  assert.ok(at >= 0, `${action} in ${f.choices}`);
  for (let i = f.selection; i < at; i++) await key(page, 'ArrowDown');
  for (let i = f.selection; i > at; i--) await key(page, 'ArrowUp');
}
async function choose(page, action) {
  await select(page, action);
  await key(page, 'Enter');
}
const label = async (page, action) => {
  const f = (await read(page)).front;
  return f.labels[f.choices.indexOf(action)];
};
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
  if (!SHOTS) return;
  await mkdir(SHOTS, { recursive: true });
  await nextFrames(page, 3);
  await page.screenshot({ path: `${SHOTS}/${name}.png` });
}
const distance = (a, b) => Math.hypot(a[0] - b[0], a[2] - b[2]);

async function open(browser, query, vertical) {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 }, serviceWorkers: 'block' }, errors);
  await page.addInitScript(vertical => {
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.setItem('fury-profile', JSON.stringify({ store: { events: ['th', 'koth'] } }));
    localStorage.setItem('atv-player-selection', JSON.stringify({ quadId: 1, riderId: 0 }));
    localStorage.setItem('atv-split-screen', vertical ? 'vertical' : 'horizontal');
    localStorage.setItem('atv-split-player2', '5,1');
  }, vertical);
  await page.goto(url(`/play.html?uiDiagnostics=1&weather=off&${query}`));
  await onFront(page, 'title');
  await page.mouse.click(8, 8);
  await key(page, 'Enter');
  await onFront(page, 'main');
  await wait(page, () => (JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.store?.atvs ?? 0) > 20);
  await choose(page, 'multiplayer');
  await onFront(page, 'multiplayer');
  await choose(page, 'split');
  await onFront(page, 'split');
  await choose(page, 'splitgames');
  await onFront(page, 'games');
  return { page, errors };
}
async function setGame(page, name) {
  await select(page, 'gametype');
  for (let i = 0; i < 4 && !(await label(page, 'gametype')).startsWith(`Game: ${name}`); i++) await key(page, 'ArrowRight');
  assert.ok((await label(page, 'gametype')).startsWith(`Game: ${name}`));
}
async function start(page, venue) {
  await choose(page, 'gamestart');
  await wait(page, venue => {
    const s = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}');
    return s.ride?.environment === venue && s.ride?.game?.loaded && s.split?.player2?.steps > 10;
  }, venue, 300000);
}

const browser = await launch({ args: ['--autoplay-policy=no-user-gesture-required'] });
try {
  // Treasure Hunt, top and bottom.
  {
    const { page, errors } = await open(browser, 'gameTest=visit', false);
    await setGame(page, 'Treasure Hunt');
    assert.equal(await label(page, 'gamecourse'), 'Course: Prospect Springs');
    await start(page, 'env05');
    let g = await game(page);
    assert.equal(g.kind, 'treasure');
    assert.equal(g.players, 2);
    assert.deepEqual(g.scores, [0, 0]);
    const s = (await read(page)).split;
    assert.equal(s.layout, 'Horizontal');
    // Each player at its own TOY start slot (Start1, Start2).
    await until(page, g => !g.started && g.countdown < 2.5, 30000);
    const p = (await read(page)).split;
    if (g.starts.length >= 2) {
      assert.ok(distance(p.player1.origin, g.starts[0].position) < 8, `P1 at Start1 ${p.player1.origin}`);
      assert.ok(distance(p.player2.origin, g.starts[1].position) < 8, `P2 at Start2 ${p.player2.origin}`);
    } else {
      assert.ok(distance(p.player1.origin, p.player2.origin) > 5, 'P2 beside P1');
    }
    await shot(page, 'treasure-split-start');
    // Both collect (the visit hook alternates players).
    g = await until(page, g => g.scores[0] >= 2 && g.scores[1] >= 2, 60000);
    assert.equal(g.progress[0], `${g.scores[0]} / 25`);
    assert.equal(g.progress[1], `${g.scores[1]} / 25`);
    assert.deepEqual([...g.places].sort(), [1, 2]);
    // Both panels and tags in their own views.
    const hud = (await read(page)).toolbar?.hud ?? (await read(page)).hud;
    assert.ok(hud && hud.tags >= 1, `treasure tags drawn: ${JSON.stringify(hud)}`);
    await shot(page, 'treasure-split-collecting');
    g = await until(page, g => g.end === 'Complete', 240000);
    assert.equal(g.scores[0] + g.scores[1], 25);
    assert.ok(g.scores[1] > 0 && g.scores[0] > 0, `both collected: ${g.scores}`);
    await wait(page, () => (JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride?.results?.length ?? 0) === 2,
      null, 60000);
    g = await game(page);
    assert.match(g.summary, /Player 1: Tokens Collected \d+/);
    assert.match(g.summary, /Player 2: Tokens Collected \d+/);
    await shot(page, 'treasure-split-results');
    console.log(`  treasure hunt: ${g.summary}`);
    // Restart Game: both players start over.
    await key(page, 'Enter');
    g = await until(page, g => !g.over && g.scores[0] === 0 && g.scores[1] === 0, 20000);
    assertNoErrors(errors);
    await page.close();
  }
  // King of the Hills, side by side: player 2 captures hill 1.
  {
    const { page, errors } = await open(browser, 'gameTest=hill2', true);
    await setGame(page, 'King of the Hills');
    await select(page, 'gamecourse');
    await key(page, 'ArrowRight');
    assert.equal(await label(page, 'gamecourse'), 'Course: Tioga Valley');
    await start(page, 'env02');
    let g = await game(page);
    assert.equal(g.kind, 'koth');
    assert.equal(g.players, 2);
    assert.equal((await read(page)).split.layout, 'Vertical');
    g = await until(page, g => g.started && g.lights === null, 30000);
    await shot(page, 'koth-split-start');
    g = await until(page, g => g.markers[0]?.owner === 1, 60000);
    assert.deepEqual(g.scores, [0, 1]);
    assert.equal(g.markers[0].score, 1500);
    assert.equal(g.notice, 'ATV 2 captured hill 1');
    assert.deepEqual(g.progress, ['0 / 5', '1 / 5']);
    assert.deepEqual(g.places, [2, 1]);
    // The compass shows player 2's hill in player 2's colour (002e76e0).
    await wait(page, () => {
      const s = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}');
      return (s.toolbar?.hud ?? s.hud)?.compass?.includes('radaricon_hill#1');
    }, null, 10000);
    await page.waitForTimeout(1500);
    await shot(page, 'koth-split-captured');
    // Pause → Restart Game frees the hill for both.
    await key(page, 'Escape');
    await key(page, 'ArrowDown');
    await key(page, 'Enter');
    g = await until(page, g => g.scores[0] === 0 && g.scores[1] === 0 && g.markers.every(m => m.owner === null), 20000);
    assertNoErrors(errors);
    console.log('  king of the hills: player 2 captured hill 1 with 1,500 points');
    await page.close();
  }
  // Hockey, top and bottom: player 1 Red, player 2 Blue at their own
  // face-off spots; the 20 s clock (hockeyTest=fast) runs to the results,
  // which list both players' goals.
  {
    const { page, errors } = await open(browser, 'hockeyTest=fast', false);
    await setGame(page, 'Hockey');
    await choose(page, 'gamestart');
    await wait(page, () => {
      const s = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}');
      return s.ride?.environment === 'game01' && s.ride?.hockey && s.split?.player2?.steps > 10;
    }, null, 300000);
    let s = await read(page);
    assert.equal(s.ride.hockey.split, true);
    assert.deepEqual(s.ride.hockey.teams, [0, 1]);
    // Opposite face-off spots: one each side of the centre line.
    await nextFrames(page, 10);
    s = await read(page);
    const gap = distance(s.split.player1.origin, s.split.player2.origin);
    assert.ok(gap > 20, `players face off apart: ${gap}`);
    await shot(page, 'hockey-split-faceoff');
    await wait(page, () => {
      const s = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}');
      return s.ride?.hockey?.finished && (s.ride?.results?.length ?? 0) > 0;
    }, null, 120000);
    s = await read(page);
    assert.match(s.ride.hockey.result, /Player 1 \(Red\) goals: \d+, Player 2 \(Blue\) goals: \d+/);
    await shot(page, 'hockey-split-results');
    console.log(`  hockey: ${s.ride.hockey.result}`);
    assertNoErrors(errors);
    await page.close();
  }
  console.log('split games: Treasure Hunt, King of the Hills and Hockey for two players');
} finally {
  await browser.close();
}
