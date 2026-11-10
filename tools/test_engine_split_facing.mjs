// Face to Face split screen (research/SPLIT_SCREEN.md "Face to Face") on a
// landscape touch phone lying flat between two players, in /play.html:
// - `?split=fh` (Screen: Face to Face) puts player 1 upright on the bottom
//   half and player 2 upside down on the top; `?split=fv` (Face to Face,
//   Ends) turns the left half toward the left edge and the right half
//   toward the right edge;
// - each player rides its own touch controls in its own half: one finger
//   in each half drives both riders at once (two fingers in all), and each
//   finger dragged toward its own player's right steers that rider right,
//   whichever way the half is turned.
// Host Chromium evidence with CDP touches (no physical device).
//   ATV_SPLIT_SHOTS=<dir> also saves a screenshot of each seating.
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { assertNoErrors, launch, newPage, url } from './browser/harness.mjs';

const SHOTS = process.env.ATV_SPLIT_SHOTS || '';
const VIEW = { width: 844, height: 390 };
const errors = [];
const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));

// Polled from Node: the page's CSP has no unsafe-eval for predicates.
async function until(page, predicate, what, timeout = 120000) {
  const deadline = Date.now() + timeout;
  for (;;) {
    const s = await read(page).catch(() => ({}));
    if (predicate(s)) return s;
    if (Date.now() > deadline) throw new Error(`timed out waiting for ${what}: ${JSON.stringify(s.split ?? {}).slice(0, 400)}`);
    await page.waitForTimeout(50); // Time-based on purpose: the poll interval.
  }
}

// The screen point `local` px from the centre of a view along its player's
// own right (`dx`) and toward its player (`dy`): [x, y, w, h, turns].
function point([x, y, w, h, turns], dx = 0, dy = 0) {
  const axes = [[1, 0], [0, 1], [-1, 0], [0, -1]][turns];
  const [ax, ay] = axes;
  return { x: x + w / 2 + dx * ax - dy * ay, y: y + h / 2 + dx * ay + dy * ax };
}

async function seating(browser, param, turns) {
  const page = await newPage(browser, { viewport: VIEW, hasTouch: true, isMobile: true }, errors);
  await page.addInitScript(() => {
    Object.defineProperty(document, 'hasFocus', { value: () => true });
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
  });
  await page.goto(url(`/play.html?venue=env01&weather=off&uiDiagnostics=1&split=${param}`));
  let s = await until(page, s => s.active === true && s.split?.touch?.frames?.length === 2, 'the split ride', 240000);
  assert.equal(s.split.facing, true, `${param}: Face to Face`);
  assert.deepEqual(s.split.turns, turns, `${param}: each view's turns`);
  const frames = s.split.touch.frames;
  assert.deepEqual(frames.map(f => f[4]), turns, `${param}: touch views turn with the scene`);
  const cdp = await page.context().newCDPSession(page);
  const touch = (type, points = []) =>
    cdp.send('Input.dispatchTouchEvent', {
      type,
      touchPoints: points.map(p => ({ radiusX: 6, radiusY: 6, force: 1, ...p }))
    });
  // One finger down in each half: both players drive.
  const start = s.split;
  const fingers = frames.map((f, i) => ({ id: i + 1, ...point(f) }));
  await touch('touchStart', fingers);
  s = await until(page, s => s.split?.touch?.owned?.every(Boolean), 'both fingers owned', 10000);
  s = await until(
    page,
    s => s.split.player1.throttle > 0 && s.split.player2.throttle > 0,
    'both players throttle',
    10000
  );
  // Each finger drags toward its own player's right.
  for (let i = 1; i <= 6; i++) {
    await touch('touchMove', frames.map((f, k) => ({ id: k + 1, ...point(f, i * 8) })));
  }
  s = await until(
    page,
    s => s.split.player1.steer > 0 && s.split.player2.steer > 0,
    'both players steering right',
    10000
  );
  await page.waitForTimeout(1500); // Time-based on purpose: let both riders gather speed.
  s = await read(page);
  const moved = [
    Math.hypot(s.split.player1.origin[0] - start.player1.origin[0], s.split.player1.origin[2] - start.player1.origin[2]),
    Math.hypot(s.split.player2.origin[0] - start.player2.origin[0], s.split.player2.origin[2] - start.player2.origin[2])
  ];
  assert.ok(moved[0] > 1 && moved[1] > 1, `${param}: both riders moved under their own fingers ${moved}`);
  assert.ok(s.split.player1.steer > 0 && s.split.player2.steer > 0, `${param}: each player steers right toward its own right: ${s.split.player1.steer} ${s.split.player2.steer}`);
  if (SHOTS) {
    await mkdir(SHOTS, { recursive: true });
    await page.screenshot({ path: `${SHOTS}/split-facing-${param}.png` });
  }
  await touch('touchEnd', fingers);
  s = await until(page, s => s.split?.touch?.owned?.every(o => !o), 'both fingers released', 10000);
  console.log(`  ${param}: turns ${turns}, moved ${moved.map(m => m.toFixed(1))} ft, released`);
  await page.close();
}

const browser = await launch();
try {
  await seating(browser, 'fh', [0, 2]);
  await seating(browser, 'fv', [1, 3]);
  assertNoErrors(errors);
  console.log('split facing: ok');
} finally {
  await browser.close();
}
