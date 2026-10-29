// Split Screen → Players (research/SPLIT_SCREEN.md "Players"): the
// original MultiATVRiderSelect seat page in /play.html with four stand-in
// standard gamepads. Freestyle's Start opens Players with the saved two
// players seated; player 3 joins with START on its own pad, moves to its
// Helmet row with the D-pad and changes it; players 2 and 3 ready up on
// their pads and player 1 on its row; once every seated player is ready
// the ride starts for three players, player 3 in its own helmet. A second
// pass seats players 1 and 4 only (player 4 leaves and rejoins), which
// rides two players. Host Chromium evidence only.
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

// Split Screen → Players (MultiATVRiderSelect): the saved players are
// seated; Start Event starts with them.
async function seated(page) {
  await onFront(page, 'splitseats');
  await choose(page, 'splitgo');
}
const split = async page => (await read(page)).split;
const seats = async page => {
  const s = await read(page);
  return (s.toolbar?.hud ?? s.hud)?.split ?? [];
};
const anchor = (seat, object) => seat.find(o => o.object === object);
// Each player's drawn objects lie in its own quarter of a 1280x720 page
// (anchors may sit up to 12 frame units over the divider: player 2's SCORE
// is placed at 308,63 and drawn from its 24-unit text offset).
function inQuarters(list, objects, players) {
  const slack = 12 * 360 / 224 + 1;
  assert.equal(list.length, players, JSON.stringify(list));
  list.forEach((seat, i) => {
    const [left, top] = [(i % 2) * 640, Math.floor(i / 2) * 360];
    for (const object of objects) {
      const at = anchor(seat, object);
      assert.ok(at, `player ${i + 1} draws object ${object}: ${JSON.stringify(seat)}`);
      assert.ok(at.x >= left - slack && at.x <= left + 640 + slack && at.y >= top && at.y <= top + 360,
        `player ${i + 1} object ${object} in its quarter: ${JSON.stringify(at)}`);
    }
  });
}
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
    for (const n of [2, 3, 4]) {
      localStorage.removeItem(`atv-split-player${n}`);
      localStorage.removeItem(`atv-split-look${n}`);
    }
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

const label = async (page, action) => {
  const f = (await read(page)).front;
  return f.labels[f.choices.indexOf(action)];
};
async function pad(page, index, button) {
  await page.evaluate(([p, b]) => window.padButton(p, b, 1), [index, button]);
  await nextFrames(page, 3);
  await page.evaluate(([p, b]) => window.padButton(p, b, 0), [index, button]);
  await nextFrames(page, 3);
}

const browser = await launch({ args: ['--autoplay-policy=no-user-gesture-required'] });
try {
  {
    const { page, errors } = await open(browser, '', 2);
    await choose(page, 'splitfree');
    await onFront(page, 'tracks');
    await key(page, 'Enter');
    await onFront(page, 'splitseats');
    let f = (await read(page)).front;
    assert.deepEqual(f.choices.slice(0, 3), ['splitgo', 'seat1ready', 'seat2ready'], JSON.stringify(f.choices));
    assert.ok(f.choices.includes('seat2helmet') && f.choices.includes('seat3join') && f.choices.includes('seat4join'),
      JSON.stringify(f.choices));
    await shot(page, 'players-page');
    // Player 3 joins on pad 3 and takes its Helmet row (D-pad down x3).
    await pad(page, 2, 9);
    f = (await read(page)).front;
    assert.ok(f.choices.includes('seat3ready') && f.choices.includes('seat3helmet'), JSON.stringify(f.choices));
    assert.match(await label(page, 'splitgo'), /3 Players/);
    for (let i = 0; i < 3; i++) await pad(page, 2, 13);
    assert.match(await label(page, 'seat3helmet'), /^> Player 3 Helmet: Original/);
    await pad(page, 2, 15);
    const helmet = await label(page, 'seat3helmet');
    assert.ok(!helmet.endsWith('Original'), helmet);
    const look3 = await page.evaluate(() => localStorage.getItem('atv-split-look3'));
    assert.ok(Number(look3.split(',')[0]) > 0, look3);
    await shot(page, 'players-page-joined');
    // Players 2 and 3 ready on their pads; player 1 on its row.
    await pad(page, 1, 9);
    await pad(page, 2, 9);
    assert.equal(await label(page, 'seat3ready'), 'Player 3: Ready');
    assert.equal((await read(page)).front.page, 'splitseats', 'player 1 is not ready yet');
    await choose(page, 'seat1ready');
    await wait(page, () => (JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').split?.others?.[1]?.steps ?? 0) > 30,
      null, 240000);
    const s = await split(page);
    assert.equal(s.players, 3);
    assert.equal(s.layout, 'Quad');
    assert.equal(s.others[1].gear[0], Number(look3.split(',')[0]), 'player 3 wears its helmet');
    assert.equal(await page.evaluate(() => localStorage.getItem('atv-split-players')), '3');
    assertNoErrors(errors);
    console.log(`  players 1-3 seated; player 3 helmet ${helmet}`);
    await page.close();
  }
  {
    const { page, errors } = await open(browser, '', 2);
    await choose(page, 'splitfree');
    await onFront(page, 'tracks');
    await key(page, 'Enter');
    await onFront(page, 'splitseats');
    // Player 2 leaves (Circle), player 4 joins, leaves and joins again.
    await pad(page, 1, 1);
    let f = (await read(page)).front;
    assert.ok(f.choices.includes('seat2join') && !f.choices.includes('splitgo'), JSON.stringify(f.choices));
    await pad(page, 3, 9);
    await pad(page, 3, 1);
    await pad(page, 3, 9);
    // Player 4 picks the female rider (row 2) and readies; Start Event.
    await pad(page, 3, 13);
    await pad(page, 3, 15);
    assert.match(await label(page, 'seat4rider'), /Female/);
    await choose(page, 'splitgo');
    await wait(page, () => (JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').split?.player2?.steps ?? 0) > 30,
      null, 240000);
    const s = await split(page);
    assert.equal(s.players, 2);
    assert.equal(s.player2.rider ?? s.others?.[0]?.rider, 1, 'player 4 rides seat 2 with its own rider');
    await shot(page, 'players-1-and-4');
    assertNoErrors(errors);
    console.log('  players 1 and 4 seated in pad order');
    await page.close();
  }
  console.log('split seats: players claim seats on their own pads and dress their riders');
} finally {
  await browser.close();
}
