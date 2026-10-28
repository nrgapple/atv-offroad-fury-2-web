// Local two-player split screen (research/SPLIT_SCREEN.md) in /play.html:
// - Split Screen's Player 2 ATV and Rider rows choose player 2's own ATV
//   and rider, which its ride uses; player 2 raises its own wheel dust and
//   the terrain keeps a fine patch around each player;
// - Main Menu → Multiplayer → Split Screen → Freestyle rides two players
//   top and bottom with the original horizontal split camera presets;
//   player 1 drives with the keyboard's left side (W) and the first
//   controller, player 2 with the second controller and then the arrow
//   keys, and each moves only by its own input;
// - Split Screen → Screen: Side by Side → Race → Start Race races both
//   players against the CPUs (one lap, the race path driver steering both:
//   ?raceLaps=1&autopilot=1 test tooling) to combined results that list
//   Player 2, held until both have finished.
// Host Chromium evidence with two stand-in standard gamepads.
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
// Menu pages: Down (or Up) to the row for `action`, then Enter.
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
  await page.screenshot({ path: `${SHOTS}/${name}.png` });
}

async function open(browser, search) {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 }, serviceWorkers: 'block' }, errors);
  await unlockStore(page);
  await page.addInitScript(() => {
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.removeItem('atv-split-screen');
    localStorage.removeItem('atv-split-player2');
    const make = (index, id) => {
      const buttons = Array.from({ length: 17 }, () => ({ pressed: false, touched: false, value: 0 }));
      // Each pad counts its own dual-rumble effects (per-controller rumble).
      const vibrationActuator = {
        type: 'dual-rumble',
        playEffect: () => { window.rumbles[index]++; return Promise.resolve('complete'); },
      };
      return { id, index, connected: true, mapping: 'standard', timestamp: 1, axes: [0, 0, 0, 0], buttons, vibrationActuator };
    };
    window.rumbles = [0, 0];
    // Two identical controllers (same id): players and rumble go by slot.
    const pads = [make(0, 'Stand-in pad (STANDARD GAMEPAD)'), make(1, 'Stand-in pad (STANDARD GAMEPAD)')];
    window.padButton = (pad, index, value) => {
      pads[pad].buttons[index] = { pressed: value > 0.5, touched: value > 0, value };
      pads[pad].timestamp++;
    };
    window.padAxis = (pad, index, value) => {
      pads[pad].axes[index] = value;
      pads[pad].timestamp++;
    };
    navigator.getGamepads = () => pads;
  });
  await page.goto(url(`/play.html?uiDiagnostics=1${search}`));
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
  // Freestyle, top and bottom.
  {
    const { page, errors } = await open(browser, '&weather=off');
    // Player 2's own garage: the next ATV, and the female rider.
    await choose(page, 'split2quad');
    await onFront(page, 'split');
    await choose(page, 'split2rider');
    await onFront(page, 'split');
    const saved = await page.evaluate(() => localStorage.getItem('atv-split-player2'));
    const [quad2, rider2] = saved.split(',').map(Number);
    assert.ok(quad2 !== 2 && rider2 === 1, `player 2 choice saved: ${saved}`);
    await choose(page, 'splitfree');
    await onFront(page, 'tracks');
    await key(page, 'Enter');
    await wait(page, () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').split?.player2?.steps > 30,
      null, 180000);
    let s = await split(page);
    assert.equal(s.layout, 'Horizontal');
    const [top, bottom] = s.views;
    assert.ok(top[1] === 0 && bottom[1] > top[3] && top[2] === bottom[2], JSON.stringify(s.views));
    assert.deepEqual(s.pads, [true, true]);
    assert.ok(s.player2.quad === quad2 && s.player2.rider === rider2, `player 2 rides its own choice: ${JSON.stringify(s.player2)}`);
    assert.ok(s.player2.quad !== s.player1.quad, 'player 1 keeps the Garage ATV');
    const start1 = s.player1.origin;
    const start2 = s.player2.origin;
    assert.ok(distance(start1, start2) > 5, `player 2 starts beside player 1: ${start1} ${start2}`);
    // Player 1: W on the keyboard. Player 2: idle.
    await page.keyboard.down('KeyW');
    await wait(page, () => JSON.parse(document.querySelector('#ui-state').textContent).split.player1.speed > 15, null, 30000);
    s = await split(page);
    assert.ok(s.player2.throttle === 0 && s.player2.speed < 3, `player 2 holds still: ${JSON.stringify(s.player2)}`);
    await page.keyboard.up('KeyW');
    // Player 2: the second controller's right trigger and left stick.
    await page.evaluate(() => { window.padButton(1, 7, 1); window.padAxis(1, 0, -0.6); });
    await wait(page, () => JSON.parse(document.querySelector('#ui-state').textContent).split.player2.speed > 15, null, 30000);
    s = await split(page);
    assert.ok(s.player2.throttle > 0.9 && s.player2.steer < -0.2, JSON.stringify(s.player2));
    assert.ok(s.player1.throttle === 0, `player 1 ignores pad 2: ${JSON.stringify(s.player1)}`);
    // Player 2's own wheel effects, and a fine terrain patch for each player
    // (merged into one while they ride close together).
    assert.ok(s.player2.effects && s.player2.effects.emitted > 0, `player 2 dust: ${JSON.stringify(s.player2.effects)}`);
    assert.ok(s.terrainPatches >= 1 && s.terrainPatches <= 2, `terrain patches ${s.terrainPatches}`);
    // Distinct chase cameras, each behind its own rider.
    const [c1, c2] = s.cameras;
    assert.ok(Math.hypot(c1[0] - c2[0], c1[2] - c2[2]) > 5, JSON.stringify(s.cameras));
    assert.ok(distance([c1[0], 0, c1[2]], s.player1.origin) < 60 && distance([c2[0], 0, c2[2]], s.player2.origin) < 60);
    // The original horizontal presets (FOV 80) at the half-height reference.
    assert.ok(c1[5] > 2 && c2[5] > 2, `split focal lengths ${c1[5]} ${c2[5]}`);
    await shot(page, 'split-freestyle-horizontal');
    await page.evaluate(() => { window.padButton(1, 7, 0); window.padAxis(1, 0, 0); });
    // Player 2's keyboard half: the arrows (player 1 does not see them).
    const before = (await split(page)).player2.origin;
    await page.keyboard.down('ArrowUp');
    await nextFrames(page, 90);
    s = await split(page);
    await page.keyboard.up('ArrowUp');
    assert.ok(s.player2.throttle > 0.9 && s.player1.throttle === 0, JSON.stringify([s.player1, s.player2]));
    assert.ok(distance(before, s.player2.origin) > 3, 'player 2 drives on the arrow keys');
    // Pull player 1's pad: player 2's pad now plays slot 0 (player 1).
    await page.evaluate(() => {
      const pads = navigator.getGamepads();
      pads[0].connected = false;
      navigator.getGamepads = () => [null, pads[1]];
      window.dispatchEvent(Object.assign(new Event('gamepaddisconnected'), { gamepad: pads[0] }));
    });
    await nextFrames(page, 20);
    s = await split(page);
    assert.deepEqual(s.pads, [true, false]);
    assertNoErrors(errors);
    console.log(`  freestyle: players ${Math.round(distance(start1, s.player1.origin))} ft and ` +
      `${Math.round(distance(start2, s.player2.origin))} ft from their starts; cameras ${Math.round(Math.hypot(c1[0] - c2[0], c1[2] - c2[2]))} ft apart`);
    await page.close();
  }
  // A one-lap race, side by side.
  {
    const { page, errors } = await open(browser, '&weather=off&raceLaps=1&autopilot=1');
    await choose(page, 'splitlayout');
    assert.ok((await read(page)).front.choices.includes('splitlayout'));
    await choose(page, 'splitrace');
    await onFront(page, 'race');
    await choose(page, 'racestart');
    await wait(page, () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').split?.player2?.race?.phase === 'racing',
      null, 180000);
    let s = await split(page);
    assert.equal(s.layout, 'Vertical');
    const [left, right] = s.views;
    assert.ok(left[0] === 0 && right[0] > left[2] && left[3] === right[3], JSON.stringify(s.views));
    const field = s.player1.race.field;
    assert.equal(s.player2.race.field, field);
    assert.ok(s.player1.race.standings.some(r => r.name === 'Player 2'), JSON.stringify(s.player1.race.standings));
    // Player 1's CPUs ride against player 2 (a human peer to avoid).
    assert.equal(s.player1.race.linkedHuman, true);
    await nextFrames(page, 120);
    await shot(page, 'split-race-vertical');
    // Player 2's finish sign shows its combined place (not its own race's).
    await wait(page, () => JSON.parse(document.querySelector('#ui-state').textContent).split.player2.race.phase === 'finished',
      null, 420000);
    s = await split(page);
    const p2 = s.player2.race;
    const p2Row = s.player1.race.standings.find(r => r.name === 'Player 2');
    assert.equal(p2.position, p2Row.position, JSON.stringify([p2, s.player1.race.standings]));
    if (p2.sign >= 13 && p2.sign <= 15) assert.equal(p2.sign - 12, p2.position, `finish sign ${p2.sign} for place ${p2.position}`);
    else assert.ok(p2.position > 3 || p2.sign === null || p2.sign < 13, `sign ${p2.sign} place ${p2.position}`);
    await shot(page, 'split-race-p2-finish');
    // Both finish; the results wait for the second.
    await wait(page, () => {
      const s = JSON.parse(document.querySelector('#ui-state').textContent).split;
      return s.player1.race.phase === 'finished' && s.player2.race.phase === 'finished' && s.resultsShown;
    }, null, 420000);
    s = await split(page);
    assert.equal(s.resultsHeld, false);
    const rows = s.resultsRows.map(r => r[0]);
    assert.ok(rows.some(r => r.includes('Player 2')) && rows.length === field, JSON.stringify(rows));
    // The race ended when both humans had finished: CPUs that finished
    // before the last human have real (not estimated) times.
    const standings = s.player1.race.standings;
    const lastHuman = Math.max(...standings.filter(r => r.player || r.name === 'Player 2').map(r => r.time));
    for (const r of standings.filter(r => !r.player && r.name !== 'Player 2' && r.time < lastHuman - 0.05)) {
      assert.equal(r.estimated, false, `CPU ${r.name} finished at ${r.time} before ${lastHuman}: ${JSON.stringify(standings)}`);
    }
    const times = s.player1.race.standings.map(r => r.time);
    assert.ok(times.every((t, i) => t !== null && (i === 0 || t >= times[i - 1])), JSON.stringify(s.player1.race.standings));
    await shot(page, 'split-race-results');
    // Player 2's impacts rumbled the second pad (identical stand-ins: by slot,
    // not by name).
    const rumbles = await page.evaluate(() => window.rumbles);
    if (s.player2.rumbles > 0) assert.ok(rumbles[1] > 0, `pad 2 rumbles ${JSON.stringify(rumbles)} for ${s.player2.rumbles} pulses`);
    console.log(`  rumble: pads ${JSON.stringify(rumbles)}, player 2 pulses ${s.player2.rumbles}`);
    assertNoErrors(errors);
    console.log(`  race: ${rows.join(' | ')}`);
    await page.close();
  }
  console.log('PASS: split screen menu, two-player Freestyle and a two-player race to combined results');
} finally {
  await browser.close();
}
