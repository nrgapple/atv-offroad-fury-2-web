// Profile stunt records in the browser (research/PROFILE.md, "Stunt
// records"): the race path driver (`autopilot=1`) rides a one-lap Supercross
// race. The race measures each jump from takeoff to landing once the landing
// settles (0024df70), keeps jumps over 40 ft (00324cc0) and landed chains'
// combos and points (001f22a0); at the finish the loaded profile's sx01
// course record and the profile keep them as maxima (00322080), and View
// Profile's Overview and Records show them ("%d Feet").
// Screenshot: ATV_PROFILE_STUNTS_SHOTS=<dir> saves the Overview and Records.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const shots = process.env.ATV_PROFILE_STUNTS_SHOTS;
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
async function shot(page, name) {
  if (!shots) return;
  await nextFrames(page, 20);
  await page.screenshot({ path: `${shots}/${name}.png` });
}
const details = f => Object.fromEntries(f.profileBook.details);
const grouped = n => n.toLocaleString('en-US');

// An earlier record the race must keep where it is larger.
const old = { points: 20, races: 2, wins: 0, stunts: { longest_jump: 41.5, most_combos: 0, most_points: 0 } };
const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await page.addInitScript(old => {
    if (sessionStorage.getItem('profile-stunts-test')) return;
    sessionStorage.setItem('profile-stunts-test', '1');
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.setItem('fury-profile', JSON.stringify(old));
    localStorage.setItem('furyScript', JSON.stringify([{ frames: 9000, input: {} }]));
  }, old);
  await page.goto(url('/play.html?venue=sx01&race=1&laps=1&cpu=3&seed=11&weather=off&autopilot=1&script=storage'));
  await page.waitForFunction(() => document.querySelector('#trace') || /error|fail/i.test(document.querySelector('#status')?.textContent), null, { timeout: 420000 });
  const trace = JSON.parse(await page.locator('#trace').textContent());
  const race = (Array.isArray(trace) ? trace.at(-1) : trace).race;
  assert.equal(race?.phase, 'finished', 'player finished the race');
  const stunts = race.player_stunts;
  assert.ok(stunts?.records, `race stunt records in the trace: ${JSON.stringify(stunts)}`);
  const { longest_jump: jump, most_combos: combos, most_points: points } = stunts.records;
  // Supercross jumps clear 40 ft; shorter ones are not records (00324cc0).
  assert.ok(jump > 40 && jump < 400, `race longest jump ${jump} ft`);
  assert.ok(combos === 0 || combos >= 2, `most combos ${combos}`);
  assert.ok(points >= 0 && stunts.combo_points >= 0);

  await page.waitForFunction(() => JSON.parse(localStorage.getItem('fury-profile') || '{}').races === 3, null, { timeout: 30000 });
  const profile = JSON.parse(await page.evaluate(() => localStorage.getItem('fury-profile')));
  const course = profile.courses?.sx01;
  assert.ok(course, JSON.stringify(profile));
  assert.equal(course.stunts?.longest_jump, jump, JSON.stringify(course));
  const best = Math.max(jump, old.stunts.longest_jump);
  assert.equal(profile.stunts.longest_jump, best);
  assert.equal(profile.stunts.most_combos ?? 0, combos);
  assert.equal(profile.stunts.most_points ?? 0, points);

  // View Profile shows them.
  await page.goto(url('/play.html?uiDiagnostics=1'));
  await onPage(page, 'title');
  await page.mouse.click(8, 8);
  await key(page, 'Enter'); await onPage(page, 'main');
  await choose(page, 'profile'); await onPage(page, 'profile');
  await choose(page, 'profile-view'); await onPage(page, 'profile-view');
  await choose(page, 'view-overview'); await onPage(page, 'profile-overview');
  let d = details(await state(page));
  assert.equal(d['Longest Jump:'], `${Math.trunc(best)} Feet`, JSON.stringify(d));
  assert.equal(d['Most Combos:'], grouped(combos));
  assert.equal(d['Most Points In One Jump:'], grouped(points));
  assert.equal(d['Most Stunt Points In An Objective:'], '-');
  await shot(page, 'overview');
  await key(page, 'Escape'); await onPage(page, 'profile-view');
  await choose(page, 'view-records'); await onPage(page, 'profile-records');
  for (let i = 0; i < 8 && !(await state(page)).labels[0].includes('Amateur Supercross'); i++) await key(page, 'ArrowRight');
  await key(page, 'ArrowDown');
  for (let i = 0; i < 12 && (await state(page)).profileBook.details[1]?.[1] !== '1'; i++) await key(page, 'ArrowRight');
  d = details(await state(page));
  assert.equal(d['Total Races:'], '1', JSON.stringify((await state(page)).profileBook.details));
  assert.equal(d['Longest Jump:'], `${Math.trunc(jump)} Feet`);
  await shot(page, 'records-sx01');
  assertNoErrors(errors);
  console.log(`PASS: Rust engine profile stunt records: jump ${jump.toFixed(1)} ft, combos ${combos}, points ${points}, combo points ${stunts.combo_points}`);
} finally {
  await browser.close();
}
