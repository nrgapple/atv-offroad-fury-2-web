// World Hall of Fame and the Daily Event (research/WORLD_BOARDS.md): Main
// Menu → Hall of Fame → View: World fetches the selection's lap and race
// boards once, shows the lap board's rows (the loaded profile's name marked)
// and the race board's offline state, and View: This Device returns to the
// local table without fetching again. Single Player → Daily Event shows the
// day's pick with its board and yesterday's winner. Event Type: Amateur
// Freestyle lists the course's score board. A Freestyle objective whose score
// earned a local Hall of Fame place is then sent once its name is entered
// (series, course, score, seconds and the 1 Hz trace) and the results show
// the World rank.
//
// The board API is mocked through Playwright routes: lap and score boards
// answer, race boards fail to connect (offline), daily boards answer with a
// winner and a submission is recorded and answered with a placing. No
// request reaches the real server.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const ui = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front || {});
const onPage = (page, name, timeout = 90000) =>
  page.waitForFunction(name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name, name, { timeout });
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }
async function choose(page, action) {
  const f = await ui(page);
  const at = f.choices.indexOf(action);
  assert.ok(at >= 0, `${action} in ${f.page}: ${f.choices}`);
  for (let i = f.selection; i < at; i++) await key(page, 'ArrowDown');
  for (let i = f.selection; i > at; i--) await key(page, 'ArrowUp');
  await key(page, 'Enter');
}
const details = async page => (await ui(page)).profileBook.details.map(([a, b]) => `${a}|${b}`);
async function waitDetail(page, pattern) {
  try {
    await page.waitForFunction(source => {
      const front = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front;
      return front?.profileBook?.details?.some(([a, b]) => new RegExp(source).test(`${a}|${b}`));
    }, pattern.source, { timeout: 30000 });
  } catch (failure) {
    const f = await ui(page);
    throw new Error(`${pattern} not shown on ${f.page}; requests ${requests.join()}; details:\n${(f.profileBook?.details || []).map(d => d.join('|')).join('\n')}`);
  }
}

const requests = [], submissions = [];
async function api(route) {
  const { pathname } = new URL(route.request().url());
  const board = decodeURIComponent(pathname.slice('/api/boards/'.length));
  const json = body => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(body) });
  if (board === 'submit') {
    assert.equal(route.request().method(), 'POST');
    const body = route.request().postDataJSON();
    submissions.push(body);
    return json({ name: body.name, boards: { [`score:${body.series}:${body.course}`]: { rank: 2, placed: true } } });
  }
  requests.push(board);
  if (board.startsWith('race:')) return route.abort('internetdisconnected');
  const entries = [
    { rank: 1, name: 'ACE', value: 61_230, vehicle: 3, day: '2026-11-11' },
    { rank: 2, name: 'PLAYER1', value: 62_000, vehicle: 3, day: '2026-11-11' },
  ];
  if (board.startsWith('daily:')) return json({ board, entries, yesterday: { date: '2026-11-11', name: 'WIN', value: 300_120, vehicle: 4 } });
  return json({ board, entries });
}

const browser = await launch();
const context = await browser.newContext({ viewport: { width: 1280, height: 720 } });
await context.route('**/api/boards/**', api);
const errors = [];
const page = await newPage(context, {}, errors);
// A profile on Amateur Freestyle's last round (env01), as
// test_engine_freestyle_objectives.mjs starts.
const done = Object.fromEntries(['env05', 'env02', 'env08', 'env06', 'env04'].map(c => [`unspstunt-normal:${c}`, 63]));
const start = { points: 40, races: 5, wins: 5, championships: { progress: { 'unspstunt-normal': { round: 5, points: [0, 0, 0, 0, 0] } }, opened: [], objectives: done } };
await page.addInitScript(start => {
  if (sessionStorage.getItem('world-boards-test')) return;
  sessionStorage.setItem('world-boards-test', '1');
  localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
  localStorage.setItem('fury-profile', JSON.stringify(start));
}, start);
await page.goto(url('/play.html?uiDiagnostics=1'));
await onPage(page, 'title');
await page.mouse.click(8, 8);
await key(page, 'Enter'); await onPage(page, 'main');

await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.choices?.includes('halloffame'), null, { timeout: 60000 });
await choose(page, 'halloffame'); await onPage(page, 'hall-of-fame');
let f = await ui(page);
assert.match(f.labels[f.choices.indexOf('fameworld')], /View: This Device/);
assert.equal(requests.length, 0, 'the local view fetches nothing');
await choose(page, 'fameworld');
await waitDetail(page, /^1\. ACE · .*\|01:01\.23$/);
await waitDetail(page, /^Offline/);
f = await ui(page);
assert.match(f.labels[f.choices.indexOf('fameworld')], /View: World/);
assert.deepEqual(requests.map(r => r.split(':')[0]).sort(), ['lap', 'race'], requests.join());
assert.ok((await details(page)).some(l => /Fastest Lap/.test(l)), (await details(page)).join('\n'));
// Back to this device's table; returning to World uses what was fetched.
await choose(page, 'fameworld');
assert.ok(!(await details(page)).some(l => /ACE/.test(l)));
await choose(page, 'fameworld');
await waitDetail(page, /^1\. ACE/);
assert.equal(requests.length, 2, requests.join());

// Event Type: Amateur Freestyle shows the course's score board (one board
// per series and course), its values as grouped scores.
f = await ui(page);
for (let i = f.selection; i > f.choices.indexOf('fameevent'); i--) await key(page, 'ArrowUp');
for (let i = 0; i < 12 && !/Amateur Freestyle/.test((await ui(page)).labels[0]); i++) await key(page, 'ArrowRight');
f = await ui(page);
assert.match(f.labels[f.choices.indexOf('fameevent')], /Event Type: Amateur Freestyle/);
await waitDetail(page, /^1\. ACE · .*\|61,230$/);
const scores = await details(page);
assert.match(scores[0], /\|World · Amateur Freestyle$/);
assert.ok(scores.includes('Highest Objective Scores|'), scores.join('\n'));
assert.ok(scores.some(l => /^(▶ )?2\. PLAYER1 · .*\|62,000$/.test(l)), scores.join('\n'));
assert.deepEqual(requests.filter(r => r.startsWith('score:')), ['score:unspstunt:env05'], requests.join());
await choose(page, 'fameworld');

// The Daily Event, from Single Player.
await key(page, 'Escape'); await onPage(page, 'main');
await choose(page, 'single'); await onPage(page, 'single');
await choose(page, 'daily'); await onPage(page, 'daily');
await waitDetail(page, /^Yesterday's Winner:\|WIN · 05:00\.12$/);
const daily = await details(page);
assert.match(daily[0], /^Daily Event · \d{4}-\d{2}-\d{2}\|UTC$/);
assert.ok(daily.some(l => /^Track:\|.+/.test(l)) && daily.some(l => /^Vehicle:\|Any (ATV|Motorcycle)$/.test(l)), daily.join('\n'));
assert.ok(requests.some(r => r.startsWith('daily:')), requests.join());
f = await ui(page);
assert.deepEqual(f.choices, ['dailystart', 'back']);

// A Freestyle objective: `objectiveTest=score` completes Score Stunt Points
// at once with the course's first Highest Objective Score, so the name entry
// opens; entering the name sends the run to the world boards.
const ride = () => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride || {});
const results = count => page.waitForFunction(count => {
  const ride = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride;
  return ride?.objective?.recorded && ride?.results?.length === count;
}, count, { timeout: 120000 });
await page.goto(url('/play.html?uiDiagnostics=1&weather=off&objectiveTest=score'));
await onPage(page, 'title');
await page.mouse.click(8, 8);
await key(page, 'Enter'); await onPage(page, 'main');
await key(page, 'Enter'); await onPage(page, 'single');
await choose(page, 'championship'); await onPage(page, 'championship');
await choose(page, 'free:unspstunt'); await onPage(page, 'objectives');
await choose(page, 'obj:3');
await results(1);
let state = await ride();
assert.deepEqual(state.fame.broken.slice(0, 1), ['Highest Objective Score'], JSON.stringify(state.fame));
assert.equal(submissions.length, 0, 'nothing is sent before the name is entered');
const score = Math.trunc(state.objective.score);
await key(page, 'Enter');
await results(2);
await page.waitForFunction(() => /^World #2 Objective Score!$/.test(JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride?.worldLine ?? ''), null, { timeout: 30000 })
  .catch(async () => { throw new Error(`no World rank on the results: ${JSON.stringify((await ride()).worldLine)}; sent ${JSON.stringify(submissions)}`); });
assert.equal(submissions.length, 1, JSON.stringify(submissions));
const sent = submissions[0];
assert.deepEqual([sent.name, sent.series, sent.course, sent.score], ['PLAYER1', 'unspstunt', 'env01', score], JSON.stringify(sent));
assert.ok(Number.isInteger(sent.vehicle) && sent.vehicle >= 1 && sent.vehicle <= 46, `vehicle ${sent.vehicle}`);
assert.ok(Number.isInteger(sent.seconds) && sent.seconds >= 1, `seconds ${sent.seconds}`);
// The server's rule: one point a second from the start, give or take two.
const { points, marks, resets } = sent.trace;
assert.ok(points.length >= 1 && Math.abs(points.length - (sent.seconds + 1)) <= 2, `${points.length} points for ${sent.seconds} s`);
assert.ok(points.every(p => p.length === 2 && p.every(Number.isInteger)), JSON.stringify(points));
assert.deepEqual([marks, resets], [[], []]);
assert.equal(sent.lapMs ?? sent.raceMs ?? sent.daily, undefined);
// Continue leaves the results; nothing is sent twice.
await key(page, 'ArrowDown');
await key(page, 'Enter');
await onPage(page, 'objectives', 120000);
assert.equal(submissions.length, 1);

assertNoErrors(errors);
await browser.close();
console.log('world boards: World view loads race and Freestyle boards, marks offline and caches; Daily Event shows its pick and board; a Freestyle objective score is sent and its World rank shown');
