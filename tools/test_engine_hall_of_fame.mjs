// The Hall of Fame in the browser (research/PROFILE.md, "Hall of Fame"): the
// race path driver (`autopilot=1`) rides a one-lap Supercross race. Its jumps
// set the course's Hall of Fame Longest Jump (00332810), kept under the
// player's placeholder until the race-over name entry ([NameEntry Menu],
// 00217e40) names it; typing a name and Enter stores it (00331f50). Main
// Menu → Hall of Fame (PAGE10 HOF) then shows the holder, and the next race
// on the course shows it on its loading screen (002585c0).
// Screenshots: ATV_HALL_OF_FAME_SHOTS=<dir> saves the name entry, the page
// and the loading screen.
import assert from 'node:assert/strict';
import { UNLOCKED_PROFILE, assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const shots = process.env.ATV_HALL_OF_FAME_SHOTS;
const state = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front || {});
const onPage = (page, name, timeout = 90000) =>
  page.waitForFunction(name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name, name, { timeout });
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }
async function choose(page, action) {
  const f = await state(page);
  const at = f.choices.indexOf(action);
  assert.ok(at >= 0, `${action} in ${f.page}: ${f.choices}`);
  for (let i = f.selection; i < at; i++) await key(page, 'ArrowDown');
  for (let i = f.selection; i > at; i--) await key(page, 'ArrowUp');
  await key(page, 'Enter');
}
async function shot(page, name, frames = 20) {
  if (!shots) return;
  await nextFrames(page, frames);
  await page.screenshot({ path: `${shots}/${name}.png` });
}
const hall = page => page.evaluate(() => JSON.parse(localStorage.getItem('fury-hall-of-fame') || '{"courses":{}}'));

const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await page.addInitScript(store => {
    if (sessionStorage.getItem('hall-of-fame-test')) return;
    sessionStorage.setItem('hall-of-fame-test', '1');
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    // The Profile Store's unlocks open Single Race's courses for the last step.
    localStorage.setItem('fury-profile', JSON.stringify({ points: 0, races: 0, wins: 0, store }));
    localStorage.removeItem('fury-hall-of-fame');
    localStorage.setItem('furyScript', JSON.stringify([{ frames: 9000, input: {} }]));
  }, UNLOCKED_PROFILE.store);
  await page.goto(url('/play.html?venue=sx01&race=1&laps=1&cpu=3&seed=11&weather=off&autopilot=1&script=storage'));
  await page.waitForFunction(() => document.querySelector('#trace') || /error|fail/i.test(document.querySelector('#status')?.textContent), null, { timeout: 420000 });
  const trace = JSON.parse(await page.locator('#trace').textContent());
  const race = (Array.isArray(trace) ? trace.at(-1) : trace).race;
  assert.equal(race?.phase, 'finished', 'player finished the race');
  // (The trace is a separate replay; the live ride keeps the records.)
  // At the results the records are kept under the loaded profile's name...
  await page.waitForFunction(() => JSON.parse(localStorage.getItem('fury-hall-of-fame') || '{"courses":{}}').courses.sx01?.longest_jump?.name === 'PLAYER1', null, { timeout: 30000 }).catch(async e => { console.log('profile', await page.evaluate(() => localStorage.getItem('fury-profile')), 'hall', await page.evaluate(() => localStorage.getItem('fury-hall-of-fame'))); throw e; });
  await shot(page, 'name-entry');
  const saved = (await hall(page)).courses.sx01;
  const jump = saved.longest_jump;
  assert.ok(jump.value > 0 && jump.value < 400, JSON.stringify(saved));
  // One lap with Track flag 8: no lap is judged and the race is not full length.
  assert.equal(saved.laps, undefined);
  assert.equal(saved.races, undefined);
  assert.equal((await hall(page)).courses['sx01:expert'], undefined);
  // ...and the name entry renames them.
  for (let i = 0; i < 8; i++) await key(page, 'Backspace');
  for (const c of ['KeyA', 'KeyC', 'KeyE']) await key(page, c);
  await key(page, 'Enter');
  await page.waitForFunction(() => JSON.parse(localStorage.getItem('fury-hall-of-fame')).courses.sx01.longest_jump.name === 'ACE', null, { timeout: 10000 });
  const fame = (await hall(page)).courses.sx01;
  assert.ok(Math.abs(fame.longest_jump.value - jump.value) < 1e-3 * jump.value, JSON.stringify(fame));
  for (const e of [fame.most_combos, fame.highest_stunt].filter(Boolean)) assert.equal(e.name, 'ACE');
  await shot(page, 'results');

  // Main Menu → Hall of Fame.
  await page.goto(url('/play.html?uiDiagnostics=1&weather=off'));
  await onPage(page, 'title');
  await page.mouse.click(8, 8);
  await key(page, 'Enter'); await onPage(page, 'main');
  const main = (await state(page)).choices;
  assert.equal(main[main.indexOf('profile') + 1], 'halloffame', main.join());
  await choose(page, 'halloffame'); await onPage(page, 'hall-of-fame');
  for (let i = 0; i < 8 && !(await state(page)).labels[0].includes('Amateur Supercross'); i++) await key(page, 'ArrowRight');
  await key(page, 'ArrowDown');
  for (let i = 0; i < 16 && !(await state(page)).profileBook.details[0]?.[1]?.startsWith('Amateur Supercross') ; i++) await key(page, 'ArrowRight');
  let details = (await state(page)).profileBook.details;
  for (let i = 0; i < 16 && !details.some(([l]) => l === 'Longest Jump: ACE'); i++) {
    await key(page, 'ArrowRight');
    details = (await state(page)).profileBook.details;
  }
  const row = details.find(([l]) => l === 'Longest Jump: ACE');
  assert.ok(row, JSON.stringify(details));
  assert.equal(row[1], `${Math.trunc(jump.value)} Feet`);
  assert.deepEqual(details[0], ['Sausalito', 'Normal Difficulty']);
  assert.deepEqual(details[2], ['1. <Nobody>', '--:--.--']);
  await shot(page, 'page');
  // Expert keeps its own records.
  await key(page, 'ArrowDown'); await key(page, 'ArrowRight');
  details = (await state(page)).profileBook.details;
  assert.ok(details.some(([l]) => l === 'Longest Jump: <Nobody>'), JSON.stringify(details));
  await key(page, 'Escape'); await onPage(page, 'main');

  // The next sx01 race's loading screen.
  await choose(page, 'single'); await onPage(page, 'single');
  await choose(page, 'race'); await onPage(page, 'race');
  let f = await state(page);
  for (let i = 0; i < 40 && !f.labels.some(l => /Sausalito/.test(l)); i++) {
    if (f.choices[f.selection] !== 'racetrack') { await key(page, 'ArrowDown'); f = await state(page); continue; }
    await key(page, 'ArrowRight'); f = await state(page);
  }
  assert.ok(f.labels.some(l => /Sausalito/.test(l)), f.labels.join());
  await choose(page, 'racestart');
  await onPage(page, 'loading');
  await shot(page, 'loading', 0);
  const loading = (await state(page)).loading;
  assert.equal(loading.fame[0], 'Fastest Lap: <Nobody> --:--.--', JSON.stringify(loading.fame));
  assert.equal(loading.fame[3], `Longest Jump: ACE ${Math.trunc(jump.value)} Feet`);
  assertNoErrors(errors);
  console.log(`PASS: Rust engine Hall of Fame: Longest Jump ${jump.value.toFixed(1)} ft named ACE, page and loading screen show it`);
} finally {
  await browser.close();
}
