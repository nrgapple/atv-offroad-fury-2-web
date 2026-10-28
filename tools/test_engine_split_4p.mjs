// Local split screen for three and four players (research/SPLIT_SCREEN.md
// "Three and four players") in /play.html, with four stand-in standard
// gamepads (one per player; the keyboard halves stay players 1 and 2):
// - Split Screen's Players row cycles 2 → 3 → 4 (saved); three and four
//   players hide the two-player layout row and add Player 3/4 ATV and Rider
//   rows;
// - Freestyle for four players: quarter views with the original
//   quarter-screen camera presets, each player on its own controller;
// - a one-lap four-player race (no CPUs, manual p. 16; the race path driver
//   steers every player: ?raceLaps=1&autopilot=1 test tooling) to combined
//   results listing all four, held until the last has finished;
// - Treasure Hunt for four players (`gameTest=visit` places the players on
//   treasures in turn) to results listing all four.
// Host Chromium evidence only.
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { assertNoErrors, launch, newPage, nextFrames, unlockStore, url } from './browser/harness.mjs';

const SHOTS = process.env.ATV_SPLIT_SHOTS || '';
const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const wait = (page, predicate, arg, timeout = 120000) => page.waitForFunction(predicate, arg, { timeout });
const onFront = (page, name) =>
  wait(page, name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name, name);
async function key(page, code) {
  await page.keyboard.down(code);
  await nextFrames(page, 2);
  await page.keyboard.up(code);
  await nextFrames(page, 3);
}
async function choose(page, action) {
  const f = (await read(page)).front;
  const at = f.choices.indexOf(action);
  assert.ok(at >= 0, `${action} in ${f.choices}`);
  for (let i = f.selection; i < at; i++) await key(page, 'ArrowDown');
  for (let i = f.selection; i > at; i--) await key(page, 'ArrowUp');
  await key(page, 'Enter');
}
const split = async page => (await read(page)).split;
const distance = (a, b) => Math.hypot(a[0] - b[0], a[2] - b[2]);
async function shot(page, name) {
  if (!SHOTS) return;
  await mkdir(SHOTS, { recursive: true });
  await nextFrames(page, 3);
  await page.screenshot({ path: `${SHOTS}/${name}.png` });
}

async function open(browser, search, players) {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 }, serviceWorkers: 'block' }, errors);
  await unlockStore(page);
  await page.addInitScript(players => {
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.setItem('atv-player-selection', JSON.stringify({ quadId: 1, riderId: 0 }));
    localStorage.removeItem('atv-split-screen');
    for (const n of [2, 3, 4]) localStorage.removeItem(`atv-split-player${n}`);
    if (players) localStorage.setItem('atv-split-players', String(players));
    else localStorage.removeItem('atv-split-players');
    const make = index => {
      const buttons = Array.from({ length: 17 }, () => ({ pressed: false, touched: false, value: 0 }));
      const vibrationActuator = {
        type: 'dual-rumble',
        playEffect: () => { window.rumbles[index]++; return Promise.resolve('complete'); },
      };
      return { id: 'Stand-in pad (STANDARD GAMEPAD)', index, connected: true, mapping: 'standard', timestamp: 1,
        axes: [0, 0, 0, 0], buttons, vibrationActuator };
    };
    window.rumbles = [0, 0, 0, 0];
    const pads = [make(0), make(1), make(2), make(3)];
    window.padButton = (pad, index, value) => {
      pads[pad].buttons[index] = { pressed: value > 0.5, touched: value > 0, value };
      pads[pad].timestamp++;
    };
    navigator.getGamepads = () => pads;
  }, players);
  await page.goto(url(`/play.html?uiDiagnostics=1&weather=off${search}`));
  await onFront(page, 'title');
  await page.mouse.click(8, 8);
  await key(page, 'Enter');
  await onFront(page, 'main');
  await choose(page, 'multiplayer');
  await onFront(page, 'multiplayer');
  await choose(page, 'split');
  await onFront(page, 'split');
  return { page, errors };
}

const browser = await launch({ args: ['--autoplay-policy=no-user-gesture-required'] });
try {
  // The Players row, then Freestyle for four players.
  {
    const { page, errors } = await open(browser, '', 0);
    let f = (await read(page)).front;
    assert.ok(f.choices.includes('splitlayout') && !f.choices.includes('split3quad'), JSON.stringify(f.choices));
    await choose(page, 'splitplayers');
    f = (await read(page)).front;
    assert.ok(f.labels[f.choices.indexOf('splitplayers')].includes('Players: 3'));
    assert.ok(!f.choices.includes('splitlayout') && f.choices.includes('split3rider') && !f.choices.includes('split4quad'),
      JSON.stringify(f.choices));
    await choose(page, 'splitplayers');
    f = (await read(page)).front;
    assert.ok(f.choices.includes('split4quad') && f.choices.length === 10, JSON.stringify(f.choices));
    assert.equal(await page.evaluate(() => localStorage.getItem('atv-split-players')), '4');
    // Player 4: the female rider.
    await choose(page, 'split4rider');
    assert.equal((await page.evaluate(() => localStorage.getItem('atv-split-player4'))).split(',')[1], '1');
    await choose(page, 'splitfree');
    await onFront(page, 'tracks');
    await key(page, 'Enter');
    await wait(page, () => (JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').split?.others?.[2]?.steps ?? 0) > 30,
      null, 240000);
    let s = await split(page);
    assert.equal(s.layout, 'Quad');
    assert.equal(s.players, 4);
    assert.equal(s.views.length, 4);
    const [a, b, c, d] = s.views;
    assert.ok(a[0] === 0 && a[1] === 0 && b[0] > a[2] && b[1] === 0 && c[0] === 0 && c[1] > a[3] && d[0] > c[2] && d[1] === c[1],
      JSON.stringify(s.views));
    assert.deepEqual(s.pads, [true, true, true, true]);
    assert.equal(s.others[2].rider, 1, 'player 4 rides its own rider choice');
    // Four distinct starts, one chase camera behind each rider.
    const origins = [s.player1.origin, ...s.others.map(o => o.origin)];
    for (let i = 0; i < 4; i++) for (let j = i + 1; j < 4; j++) {
      assert.ok(distance(origins[i], origins[j]) > 5, `players ${i + 1} and ${j + 1} apart: ${JSON.stringify(origins)}`);
    }
    // The quarter presets keep the 4:3 reference (focal scale 1).
    assert.ok(s.cameras.length === 4 && s.cameras.every(cam => cam[5] > 0.5 && cam[5] < 2.5), JSON.stringify(s.cameras));
    assert.ok(s.cameras.every((cam, i) => distance([cam[0], 0, cam[2]], origins[i]) < 60), JSON.stringify(s.cameras));
    // Players 3 and 4: their own controllers (right trigger) only.
    await page.evaluate(() => window.padButton(2, 7, 1));
    await wait(page, () => JSON.parse(document.querySelector('#ui-state').textContent).split.others[1].speed > 10, null, 30000);
    s = await split(page);
    assert.ok(s.others[1].throttle > 0.9, JSON.stringify(s.others[1]));
    assert.ok(s.player1.throttle === 0 && s.others[0].throttle === 0 && s.others[2].throttle === 0,
      `only player 3 drives: ${JSON.stringify([s.player1.throttle, ...s.others.map(o => o.throttle)])}`);
    await page.evaluate(() => { window.padButton(2, 7, 0); window.padButton(3, 7, 1); });
    await wait(page, () => JSON.parse(document.querySelector('#ui-state').textContent).split.others[2].speed > 10, null, 30000);
    s = await split(page);
    assert.ok(s.others[2].throttle > 0.9 && s.others[1].throttle === 0, JSON.stringify(s.others));
    // Player 3's dust, and a terrain patch per player (merged while close).
    assert.ok(s.others[1].effects && s.others[1].effects.emitted > 0, JSON.stringify(s.others[1].effects));
    assert.ok(s.terrainPatches >= 1 && s.terrainPatches <= 4, `terrain patches ${s.terrainPatches}`);
    await nextFrames(page, 30);
    await shot(page, 'split-4p-freestyle');
    await page.evaluate(() => window.padButton(3, 7, 0));
    assertNoErrors(errors);
    console.log(`  freestyle: 4 quarter views ${JSON.stringify(s.views)}; players 3/4 drove on pads 3/4`);
    await page.close();
  }
  // A one-lap race for four players: no CPUs, results list all four.
  {
    const { page, errors } = await open(browser, '&raceLaps=1&autopilot=1', 4);
    await choose(page, 'splitrace');
    await onFront(page, 'race');
    await choose(page, 'racestart');
    await wait(page, () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').split?.others?.[2]?.race?.phase === 'racing',
      null, 240000);
    let s = await split(page);
    assert.equal(s.layout, 'Quad');
    assert.equal(s.player1.race.field, 4, 'four humans and no CPUs');
    assert.ok(s.others.every(o => o.race.field === 4), JSON.stringify(s.others.map(o => o.race.field)));
    const names = s.player1.race.standings.map(r => r.name).sort();
    assert.deepEqual(names.filter(n => n.startsWith('Player')), ['Player 2', 'Player 3', 'Player 4']);
    assert.equal(s.player1.race.linkedHumans, 3);
    await nextFrames(page, 60);
    await shot(page, 'split-4p-race');
    await wait(page, () => {
      const s = JSON.parse(document.querySelector('#ui-state').textContent).split;
      return s.player1.race.phase === 'finished' && s.others.every(o => o.race.phase === 'finished') && s.resultsShown;
    }, null, 600000);
    s = await split(page);
    assert.equal(s.resultsHeld, false);
    const rows = s.resultsRows.map(r => r[0]);
    assert.ok(rows.length === 4 && ['Player 2', 'Player 3', 'Player 4'].every(n => rows.some(r => r.includes(n))), JSON.stringify(rows));
    const places = [s.player1.race.position, ...s.others.map(o => o.race.position)].sort();
    assert.deepEqual(places, [1, 2, 3, 4]);
    await shot(page, 'split-4p-race-results');
    assertNoErrors(errors);
    console.log(`  race: ${rows.join(' | ')}`);
    await page.close();
  }
  // Treasure Hunt for four players.
  {
    const { page, errors } = await open(browser, '&gameTest=visit', 4);
    await choose(page, 'splitgames');
    await onFront(page, 'games');
    const label = async action => {
      const f = (await read(page)).front;
      return f.labels[f.choices.indexOf(action)];
    };
    const f = (await read(page)).front;
    for (let i = f.selection; i < f.choices.indexOf('gametype'); i++) await key(page, 'ArrowDown');
    for (let i = 0; i < 4 && !(await label('gametype')).startsWith('Game: Treasure Hunt'); i++) await key(page, 'ArrowRight');
    assert.ok((await label('gametype')).startsWith('Game: Treasure Hunt'));
    await choose(page, 'gamestart');
    await wait(page, () => {
      const s = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}');
      return s.ride?.game?.loaded && (s.split?.others?.[2]?.steps ?? 0) > 10;
    }, null, 300000);
    const game = async () => (await read(page)).ride?.game;
    let g = await game();
    assert.equal(g.players, 4);
    assert.equal(g.scores.length, 4);
    const deadline = Date.now() + 240000;
    while (!(g.scores.every(n => n >= 1))) {
      if (Date.now() > deadline) throw new Error(`every player collects: ${JSON.stringify(g.scores)}`);
      await page.waitForTimeout(250);
      g = await game();
    }
    await shot(page, 'split-4p-treasure');
    while (!g.over && g.end !== 'Complete') {
      if (Date.now() > deadline + 240000) throw new Error(`Treasure Hunt ends: ${JSON.stringify(g)}`);
      await page.waitForTimeout(500);
      g = await game();
    }
    await wait(page, () => (JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride?.results?.length ?? 0) > 0,
      null, 60000);
    g = await game();
    for (const n of [1, 2, 3, 4]) assert.match(g.summary, new RegExp(`Player ${n}: Tokens Collected \\d+`));
    assertNoErrors(errors);
    console.log(`  treasure hunt: ${g.summary}`);
    await page.close();
  }
  console.log('PASS: split screen for four players: menu, Freestyle, a race and Treasure Hunt');
} finally {
  await browser.close();
}
