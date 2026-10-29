// Split-screen Tag (research/SPLIT_SCREEN.md "Slice 4") in /play.html:
// Main Menu → Multiplayer → Split Screen → Games → Tag on Prospect Springs
// plays Tag on this console for both players, with no room: the ride is
// the original Tag course (E05_TG.RES: the mode TOY's start, `tagball.psg`),
// one `atv_core::tag` game holds both riders (player 1 slot 0, player 2
// slot 1), `tagReach=60` (a wide contact for automation) lets the parked
// players tag each other so both carry the ball, both views draw their own
// Tag panel, a 30 s clock (`tagTest=fast`) ends in Tag Results naming
// Player 1 and Player 2, and Restart Game starts both over.
// ATV_SPLIT_TAG_SHOTS=<dir> saves captures. Host Chromium evidence.
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const SHOTS = process.env.ATV_SPLIT_TAG_SHOTS || '';
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

// Split Screen → Players (MultiATVRiderSelect): the saved players are
// seated; Start Event starts with them.
async function seated(page) {
  await onFront(page, 'splitseats');
  await choose(page, 'splitgo');
}
const label = async (page, action) => {
  const f = (await read(page)).front;
  return f.labels[f.choices.indexOf(action)];
};
async function until(page, predicate, timeout = 60000) {
  const deadline = Date.now() + timeout;
  for (;;) {
    const t = (await read(page)).ride?.tag;
    if (t && predicate(t)) return t;
    if (Date.now() > deadline) throw new Error(`timed out: ${predicate} ${JSON.stringify(t)}`);
    await page.waitForTimeout(100);
  }
}
async function shot(page, name) {
  if (!SHOTS) return;
  await mkdir(SHOTS, { recursive: true });
  await nextFrames(page, 3);
  await page.screenshot({ path: `${SHOTS}/${name}.png` });
}

const browser = await launch({ args: ['--autoplay-policy=no-user-gesture-required'] });
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 }, serviceWorkers: 'block' }, errors);
  await page.addInitScript(() => {
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.setItem('fury-profile', JSON.stringify({ store: { events: ['tag'] } }));
    localStorage.setItem('atv-player-selection', JSON.stringify({ quadId: 1, riderId: 0 }));
    localStorage.setItem('atv-split-screen', 'horizontal');
    localStorage.setItem('atv-split-player2', '5,1');
  });
  await page.goto(url('/play.html?uiDiagnostics=1&weather=off&tagTest=fast&tagReach=60'));
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
  await select(page, 'gametype');
  for (let i = 0; i < 4 && !(await label(page, 'gametype')).startsWith('Game: Tag'); i++) await key(page, 'ArrowRight');
  assert.ok((await label(page, 'gametype')).startsWith('Game: Tag'));
  assert.equal(await label(page, 'gamecourse'), 'Course: Prospect Springs');
  await choose(page, 'gamestart');
  await seated(page);
  await wait(page, () => {
    const s = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}');
    return s.ride?.environment === 'env05' && s.ride?.tag && s.split?.player2?.steps > 10;
  }, null, 300000);
  let s = await read(page);
  // No room: the Tag course (its ball model) and both local riders.
  assert.equal(s.ride.tagBall, true, 'the Tag course draws tagball.psg');
  assert.equal(s.ride.tag.local, true);
  assert.equal(s.ride.tag.host, true);
  assert.deepEqual(s.ride.tag.roster, [0, 1]);
  assert.equal(s.ride.tag.timeLimit, 30);
  assert.ok(!s.multiplayer?.code, 'no room hosted');
  // Both players carry the ball (the wide reach tags the parked carrier).
  let t = await until(page, t => t.log.some(l => l === 'Gain 0') && t.log.some(l => l === 'Gain 1'), 30000);
  await shot(page, 'tag-split-play');
  t = await until(page, t => t.finished, 60000);
  await wait(page, () => (JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride?.results?.length ?? 0) > 0,
    null, 30000);
  t = (await read(page)).ride.tag;
  assert.match(t.summary, /1\. Player [12] \d+ {3}2\. Player [12] \d+/);
  assert.ok(t.scores[0] + t.scores[1] > 0, `points scored: ${t.scores}`);
  await shot(page, 'tag-split-results');
  console.log(`  split tag: ${t.summary} (log ${t.log.join(', ')})`);
  // Restart Game: a new game for both.
  await key(page, 'Enter');
  t = await until(page, t => !t.finished && t.scores.every(v => v === 0) && t.clock > 25, 20000);
  assert.ok(t.local);
  assertNoErrors(errors);
  await page.close();
  console.log('split tag: two local players on the Tag course to results and Restart Game');
} finally {
  await browser.close();
}
