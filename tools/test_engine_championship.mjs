// Racing championships in the browser (research/CHAMPIONSHIPS.md): Single
// Player → Championship lists the series with the saved progress; a profile
// on round 5 of Amateur Supercross starts that round's course (sx02, five
// laps, four CPUs; `raceLaps=1` shortens them to one lap for the test). The
// race path driver rides the player (`autopilot=1`).
// The finish records the round (7/4/2/1/0 points, 001f1db0); the advanced
// round offers Continue Championship, which loads the final round (sx06).
// Its finish awards the trophy and its first-time Profile Points (00322440,
// 00322650) and opens Pro Supercross (003238a0); Quit to Championship Menu
// returns to the page, which shows the new state.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, optionalScreenshot, url } from './browser/harness.mjs';

const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const onPage = (page, name, timeout = 60000) => page.waitForFunction(name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name, name, { timeout });
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }
const profile = page => page.evaluate(() => JSON.parse(localStorage.getItem('fury-profile') || '{}'));

const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  const start = {
    points: 40, races: 5, wins: 5,
    championships: { progress: { 'amsx-normal': { round: 4, points: [31, 20, 10, 5, 0] } }, opened: [] },
  };
  await page.addInitScript(start => {
    if (sessionStorage.getItem('champ-test')) return;
    sessionStorage.setItem('champ-test', '1');
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.setItem('fury-profile', JSON.stringify(start));
  }, start);
  await page.goto(url('/play.html?uiDiagnostics=1&autopilot=1&raceLaps=1&weather=off'));
  await onPage(page, 'title');
  await page.mouse.click(8, 8);
  await key(page, 'Enter'); await onPage(page, 'main');
  await key(page, 'Enter'); await onPage(page, 'single');
  let front = (await read(page)).front;
  const at = front.choices.indexOf('championship');
  assert.ok(at > 0, `Championship row: ${front.choices}`);
  for (let i = front.selection; i < at; i++) await key(page, 'ArrowDown');
  await key(page, 'Enter'); await onPage(page, 'championship');
  front = (await read(page)).front;
  const labels = Object.fromEntries(front.choices.map((c, i) => [c, front.labels[i]]));
  assert.equal(labels['champ:amsx'], 'Amateur Supercross · Round 5 of 6 · 31 pts');
  assert.equal(labels['champ:prosx'], 'Pro Supercross · Locked');
  await optionalScreenshot(page, 'ATV_CHAMPIONSHIP_SCREENSHOT', 'menu');
  for (let i = front.selection; i < front.choices.indexOf('champ:amsx'); i++) await key(page, 'ArrowDown');
  await key(page, 'Enter');
  const ride = async () => (await read(page)).ride ?? {};
  // Round 5 loads and races; the results record it once.
  await page.waitForFunction(() => JSON.parse(localStorage.getItem('fury-profile') || '{}').races === 6, null, { timeout: 600000 });
  let state = await ride();
  assert.equal(state.environment, 'sx02');
  assert.deepEqual(state.championship, [4, false]);
  const mid = (await profile(page)).championships.progress['amsx-normal'];
  assert.equal(mid.round, 5, `advanced ${JSON.stringify(mid)}`);
  assert.ok(mid.points[0] >= 31 + 1, `points ${mid.points}`);
  await nextFrames(page, 10);
  await optionalScreenshot(page, 'ATV_CHAMPIONSHIP_SCREENSHOT', 'round');
  // Continue → Continue Championship: the final round on sx06.
  await key(page, 'Enter');
  await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride?.results?.[0] === 35, null, { timeout: 10000 });
  await key(page, 'Enter');
  await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride?.environment === 'sx06', null, { timeout: 120000 });
  await page.waitForFunction(() => JSON.parse(localStorage.getItem('fury-profile') || '{}').races === 7, null, { timeout: 600000 });
  state = await ride();
  assert.deepEqual(state.championship, [4, false]);
  const after = await profile(page);
  const progress = after.championships.progress['amsx-normal'];
  assert.deepEqual([progress.round, progress.points], [0, [0, 0, 0, 0, 0]], `series restarts ${JSON.stringify(progress)}`);
  assert.ok(['Gold', 'Silver', 'Bronze'].includes(progress.trophy), `trophy ${JSON.stringify(progress)}`);
  const reward = { Gold: 500, Silver: 250, Bronze: 125 }[progress.trophy];
  const award = after.points - start.points - reward;
  assert.ok(award >= 0 && award <= 120, `course Profile Points ${award} (total ${after.points})`);
  assert.ok(after.championships.opened.includes('prosx-normal'), `opened ${after.championships.opened}`);
  assert.equal(after.championships.opened.includes('amsx-expert'), progress.trophy === 'Gold');
  await nextFrames(page, 10);
  await optionalScreenshot(page, 'ATV_CHAMPIONSHIP_SCREENSHOT', 'results');
  // Continue → Quit to Championship Menu (the only choice): the page.
  await key(page, 'Enter');
  assert.deepEqual((await ride()).results, [33]);
  await key(page, 'Enter');
  await onPage(page, 'championship', 120000);
  front = (await read(page)).front;
  const now = Object.fromEntries(front.choices.map((c, i) => [c, front.labels[i]]));
  assert.equal(now['champ:prosx'], 'Pro Supercross · Round 1 of 6 · 0 pts');
  assert.equal(now['champ:amsx'], `Amateur Supercross · Round 1 of 6 · 0 pts · ${progress.trophy}`);
  assertNoErrors(errors);
  console.log(`PASS: Rust engine championship: final Am SX round -> ${progress.trophy} (+${reward}), course award +${award}, Pro SX opened`);
} finally {
  await browser.close();
}
