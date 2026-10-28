// The original Profile Editor in the Rust engine (research/PROFILE.md): an old
// single `fury-profile` save migrates to a loaded PLAYER1; Main Menu → Profile
// Editor creates a named profile through the name entry, copies, deletes (with
// the original "Delete this save file?" alert) and loads profiles; View Profile
// shows the Overview, Trophies and Records pages from the saved record; the
// book survives a reload; and a race (the race path driver, `autopilot=1`)
// credits the loaded profile's course record.
// Screenshots: ATV_PROFILE_EDITOR_SHOTS=<dir> saves the pages (landscape and a
// 390x844 phone portrait).
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const shots = process.env.ATV_PROFILE_EDITOR_SHOTS;
const state = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front || {});
const onPage = (page, name, timeout = 90000) =>
  page.waitForFunction(name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name, name, { timeout });
const book = page => page.evaluate(() => JSON.parse(localStorage.getItem('fury-profiles') || 'null'));
const mirror = page => page.evaluate(() => JSON.parse(localStorage.getItem('fury-profile') || 'null'));
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }
async function choose(page, action) {
  const f = await state(page);
  const at = f.choices.indexOf(action);
  assert.ok(at >= 0, `${action} in ${f.page}: ${f.choices}`);
  for (let i = f.selection; i < at; i++) await key(page, 'ArrowDown');
  for (let i = f.selection; i > at; i--) await key(page, 'ArrowUp');
  assert.equal((await state(page)).choices[(await state(page)).selection], action);
  await key(page, 'Enter');
}
async function shot(page, name) {
  if (!shots) return;
  await nextFrames(page, 20);
  await page.screenshot({ path: `${shots}/${name}.png` });
}
async function typeName(page, text) {
  const input = page.getByLabel('Enter Player Profile Save File Name:', { exact: true });
  await input.click();
  await page.waitForFunction(() => document.activeElement?.tagName === 'INPUT', null, { timeout: 5000 });
  for (let i = (await input.inputValue()).length; i > 0; i--) await page.keyboard.press('Backspace');
  await page.keyboard.type(text);
  await page.keyboard.press('Enter');
  await nextFrames(page, 4);
}
const details = f => Object.fromEntries(f.profileBook.details);

const old = {
  points: 55, races: 3, wins: 1, best_stunt: 4200,
  championships: { progress: { 'amsx-normal': { round: 6, points: [40, 10, 9, 8, 7], trophy: 'Gold' } }, opened: ['prosx-normal', 'amsx-expert'] },
};
const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await page.addInitScript(old => {
    if (sessionStorage.getItem('profile-editor-test')) return;
    sessionStorage.setItem('profile-editor-test', '1');
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.setItem('fury-profile', JSON.stringify(old));
  }, old);
  await page.goto(url('/play.html?uiDiagnostics=1'));
  await onPage(page, 'title');
  await page.mouse.click(8, 8);
  await key(page, 'Enter'); await onPage(page, 'main');
  let f = await state(page);
  assert.equal(f.choices.indexOf('profile'), f.choices.indexOf('settings') - 2, `${f.choices}`);
  assert.equal(f.choices.indexOf('halloffame'), f.choices.indexOf('settings') - 1, `${f.choices}`);
  // The old save migrated: PLAYER1, loaded, keeping its record.
  assert.equal(f.profileBook.name, 'PLAYER1');
  await shot(page, 'main-menu');
  await choose(page, 'profile'); await onPage(page, 'profile');
  f = await state(page);
  assert.deepEqual(f.choices, ['profile-create', 'profile-load', 'profile-view', 'profile-store', 'profile-unlock', 'profile-copy', 'profile-delete', 'passkey', 'back']);
  assert.equal(f.profile, 'Current Profile: PLAYER1 · Current Profile Points: 55');
  await shot(page, 'editor');

  // Create New Profile → name entry → Save Profile.
  await choose(page, 'profile-create'); await onPage(page, 'profile-name');
  await shot(page, 'name-entry');
  await typeName(page, 'ace');
  await choose(page, 'profilesave'); await onPage(page, 'profile');
  f = await state(page);
  assert.equal(f.profileBook.name, 'ACE');
  assert.match(f.notice, /ACE created and loaded/);
  let saved = await book(page);
  assert.deepEqual(saved.profiles.map(p => p.name), ['PLAYER1', 'ACE']);
  assert.equal(saved.current, 1);
  assert.equal((await mirror(page)).name, 'ACE');
  assert.equal(saved.profiles[0].points, 55, 'PLAYER1 kept its points');

  // Copy PLAYER1 as COPY1.
  await choose(page, 'profile-copy'); await onPage(page, 'profile-slots');
  assert.equal((await state(page)).profileBook.heading, 'Copy Profile');
  await choose(page, 'slot:0'); await onPage(page, 'profile-name');
  await typeName(page, 'copy1');
  await choose(page, 'profilesave'); await onPage(page, 'profile');
  assert.equal((await state(page)).notice, 'Copy Successful.');
  saved = await book(page);
  assert.deepEqual(saved.profiles.map(p => [p.name, p.points]), [['PLAYER1', 55], ['ACE', 0], ['COPY1', 55]]);

  // Delete COPY1: the alert starts on No.
  await choose(page, 'profile-delete'); await onPage(page, 'profile-slots');
  await choose(page, 'slot:2'); await onPage(page, 'profile-confirm');
  f = await state(page);
  assert.equal(f.choices[f.selection], 'confirmno');
  assert.deepEqual(f.profileBook.details[0], ['Delete this save file?', '']);
  await shot(page, 'delete-confirm');
  await choose(page, 'confirmyes'); await onPage(page, 'profile');
  assert.equal((await state(page)).notice, 'Delete Successful.');
  assert.deepEqual((await book(page)).profiles.map(p => p.name), ['PLAYER1', 'ACE']);

  // Load PLAYER1 back.
  await choose(page, 'profile-load'); await onPage(page, 'profile-slots');
  await shot(page, 'load-slots');
  await choose(page, 'slot:0'); await onPage(page, 'profile-confirm');
  await choose(page, 'confirmyes'); await onPage(page, 'profile');
  assert.equal((await state(page)).profileBook.name, 'PLAYER1');
  assert.equal((await mirror(page)).points, 55);

  // View Profile: Overview, Trophies, Records.
  await choose(page, 'profile-view'); await onPage(page, 'profile-view');
  await shot(page, 'view-menu');
  await choose(page, 'view-overview'); await onPage(page, 'profile-overview');
  let d = details(await state(page));
  assert.equal(d['Profile Points Remaining:'], '55');
  assert.equal(d['Total Races:'], '3');
  assert.equal(d['Total Victories:'], '1');
  assert.equal(d['Win Percentage:'], ' 33%');
  assert.equal(d['Highest Race Stunt Score:'], '4,200');
  await shot(page, 'overview');
  await key(page, 'Escape'); await onPage(page, 'profile-view');
  await choose(page, 'view-trophies'); await onPage(page, 'profile-trophies');
  d = details(await state(page));
  assert.equal(d['Amateur Supercross'], 'Gold / None');
  await shot(page, 'trophies');
  await key(page, 'Escape'); await onPage(page, 'profile-view');
  await choose(page, 'view-records'); await onPage(page, 'profile-records');
  await shot(page, 'records');

  // Phone portrait.
  if (shots) {
    await page.setViewportSize({ width: 390, height: 844 });
    await shot(page, 'phone-records');
    await key(page, 'Escape'); await onPage(page, 'profile-view');
    await choose(page, 'view-overview'); await onPage(page, 'profile-overview');
    await shot(page, 'phone-overview');
    await key(page, 'Escape'); await key(page, 'Escape'); await onPage(page, 'profile');
    await shot(page, 'phone-editor');
    await choose(page, 'profile-create'); await onPage(page, 'profile-name');
    await shot(page, 'phone-name-entry');
    await key(page, 'Escape'); await onPage(page, 'profile');
    await page.setViewportSize({ width: 320, height: 568 });
    await shot(page, 'small-phone-editor');
    await key(page, 'Escape'); await onPage(page, 'main');
    await shot(page, 'small-phone-main');
    await page.setViewportSize({ width: 390, height: 844 });
    await shot(page, 'phone-main');
    await choose(page, 'profile'); await onPage(page, 'profile');
    await page.setViewportSize({ width: 1280, height: 720 });
  }

  // Persistence: a reload keeps the book and the loaded profile.
  await page.reload();
  await onPage(page, 'title');
  await page.mouse.click(8, 8);
  await key(page, 'Enter'); await onPage(page, 'main');
  f = await state(page);
  assert.deepEqual(f.profileBook.names, ['PLAYER1', 'ACE']);
  assert.equal(f.profileBook.name, 'PLAYER1');

  // A race credits the loaded profile only: load ACE, race sx01.
  saved = await book(page);
  saved.current = 1;
  await page.evaluate(saved => {
    localStorage.setItem('fury-profiles', JSON.stringify(saved));
    localStorage.setItem('fury-profile', JSON.stringify(saved.profiles[1]));
    localStorage.setItem('furyScript', JSON.stringify([{ frames: 9000, input: {} }]));
  }, saved);
  await page.goto(url('/play.html?venue=sx01&race=1&laps=1&cpu=3&seed=11&weather=off&autopilot=1&script=storage'));
  await page.waitForFunction(() => JSON.parse(localStorage.getItem('fury-profile') || '{}').races === 1, null, { timeout: 420000 });
  saved = await book(page);
  const ace = saved.profiles[1];
  assert.equal(ace.name, 'ACE');
  assert.equal(ace.courses.sx01.races, 1, JSON.stringify(ace));
  // sx01 carries Track flag 8: its first lap sets no best lap (00248b20).
  assert.equal(ace.courses.sx01.best_lap_ms, undefined, JSON.stringify(ace.courses.sx01));
  assert.equal(ace.courses.sx01.best_total_ms, undefined, 'a one-lap race is not a full-length time');
  assert.equal(saved.profiles[0].points, 55, 'PLAYER1 untouched');

  // Records shows it: Amateur Supercross → Sausalito (sx01).
  await page.goto(url('/play.html?uiDiagnostics=1'));
  await onPage(page, 'title');
  await page.mouse.click(8, 8);
  await key(page, 'Enter'); await onPage(page, 'main');
  await choose(page, 'profile'); await onPage(page, 'profile');
  await choose(page, 'profile-view'); await onPage(page, 'profile-view');
  await choose(page, 'view-records'); await onPage(page, 'profile-records');
  for (let i = 0; i < 8 && !(await state(page)).labels[0].includes('Amateur Supercross'); i++) await key(page, 'ArrowRight');
  await key(page, 'ArrowDown');
  for (let i = 0; i < 12 && (await state(page)).profileBook.details[1]?.[1] !== '1'; i++) await key(page, 'ArrowRight');
  d = details(await state(page));
  assert.equal(d['Total Races:'], '1', JSON.stringify((await state(page)).profileBook.details));
  assert.equal(d['Best Lap Time:'], '--:--.--');
  // A win names the ridden ATV; otherwise there is none.
  if (ace.wins) assert.notEqual(d['Most Winning ATV:'], 'None');
  else assert.equal(d['Most Winning ATV:'], 'None');
  await shot(page, 'records-sx01');
  assertNoErrors(errors);
  console.log(`PASS: Rust engine Profile Editor (ACE sx01 ${JSON.stringify(ace.courses.sx01)})`);
} finally {
  await browser.close();
}
