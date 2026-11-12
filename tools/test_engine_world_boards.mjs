// World Hall of Fame and the Daily Event (research/WORLD_BOARDS.md): Main
// Menu → Hall of Fame → View: World fetches the selection's lap and race
// boards once, shows the lap board's rows (the loaded profile's name marked)
// and the race board's offline state, and View: This Device returns to the
// local table without fetching again. Single Player → Daily Event shows the
// day's pick with its board and yesterday's winner.
//
// The board API is mocked through Playwright routes: lap boards answer, race
// boards fail to connect (offline), daily boards answer with a winner.
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

const requests = [];
async function api(route) {
  const { pathname } = new URL(route.request().url());
  const board = decodeURIComponent(pathname.slice('/api/boards/'.length));
  requests.push(board);
  const json = body => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(body) });
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
await page.addInitScript(() => localStorage.setItem('atv2-keyboard-help-seen-v1', '1'));
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

assertNoErrors(errors);
await browser.close();
console.log('world boards: World view loads, marks offline and caches; Daily Event shows its pick and board');
