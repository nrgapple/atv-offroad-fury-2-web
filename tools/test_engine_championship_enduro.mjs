// Enduro championships in the browser (research/CHAMPIONSHIPS.md): a profile
// on the final round of Amateur Enduro (round 5 of 5) starts that round's
// course, Freeport Harbor's short Enduro (env04ends: three laps, four CPUs;
// `raceLaps=1` shortens it for the test), raced gate by gate (RACING.md
// "Enduro") by the race path driver (`autopilot=1`). The finish awards the
// trophy and its first-time Profile Points and opens Pro Enduro (003238a0),
// whose first round is Prospect Springs' long Enduro.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, optionalScreenshot, url } from './browser/harness.mjs';

const manifest = await (await fetch(url('/content/manifest.json'))).json();
if (!manifest.files.some(f => f.path === '/TRACKS/TRACKSA/ENV04/E04_ENDS.RES')) {
  console.log('SKIP: the content package has no Enduro race archives yet');
  process.exit(0);
}
const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const onPage = (page, name, timeout = 60000) => page.waitForFunction(name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name, name, { timeout });
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }
const profile = page => page.evaluate(() => JSON.parse(localStorage.getItem('fury-profile') || '{}'));

const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  const start = {
    points: 40, races: 4, wins: 4,
    championships: { progress: { 'send-normal': { round: 4, points: [28, 16, 8, 4, 0] } }, opened: [] },
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
  for (let i = front.selection; i < at; i++) await key(page, 'ArrowDown');
  await key(page, 'Enter'); await onPage(page, 'championship');
  front = (await read(page)).front;
  let labels = Object.fromEntries(front.choices.map((c, i) => [c, front.labels[i]]));
  assert.equal(labels['champ:send'], 'Amateur Enduro · Round 5 of 5 · 28 pts');
  assert.equal(labels['champ:lend'], 'Pro Enduro · Locked');
  for (let i = front.selection; i < front.choices.indexOf('champ:send'); i++) await key(page, 'ArrowDown');
  await key(page, 'Enter');
  await page.waitForFunction(() => JSON.parse(localStorage.getItem('fury-profile') || '{}').races === 5, null, { timeout: 600000 });
  const state = (await read(page)).ride ?? {};
  assert.equal(state.environment, 'env04');
  assert.deepEqual(state.championship, [0, false]);
  const after = await profile(page);
  const progress = after.championships.progress['send-normal'];
  assert.deepEqual([progress.round, progress.points], [0, [0, 0, 0, 0, 0]], `series restarts ${JSON.stringify(progress)}`);
  assert.ok(['Gold', 'Silver', 'Bronze'].includes(progress.trophy), `trophy ${JSON.stringify(progress)}`);
  const reward = { Gold: 500, Silver: 250, Bronze: 125 }[progress.trophy];
  const award = after.points - start.points - reward;
  assert.ok(award >= 0 && award <= 120, `course Profile Points ${award} (total ${after.points})`);
  assert.ok(after.championships.opened.includes('lend-normal'), `opened ${after.championships.opened}`);
  await nextFrames(page, 10);
  await optionalScreenshot(page, 'ATV_CHAMPIONSHIP_SCREENSHOT', 'enduro-results');
  // Continue → Quit to Championship Menu: Pro Enduro is open.
  await key(page, 'Enter');
  await key(page, 'Enter');
  await onPage(page, 'championship', 120000);
  front = (await read(page)).front;
  labels = Object.fromEntries(front.choices.map((c, i) => [c, front.labels[i]]));
  assert.equal(labels['champ:lend'], 'Pro Enduro · Round 1 of 5 · 0 pts');
  assertNoErrors(errors);
  console.log(`PASS: Rust engine Enduro championship: final Am Enduro round on env04ends -> ${progress.trophy} (+${reward}), course award +${award}, Pro Enduro opened`);
} finally {
  await browser.close();
}
