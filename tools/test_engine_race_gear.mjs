// Rust frontend: Single Player → Race (course tabs and list, laps, CPU riders,
// difficulty, the preview arrows, Copy ride link, Start Race) and Garage →
// Rider Gear (catalog steppers, quad paint, previews dressed in the saved
// look), plus the `?menu=race` shortcut and a `play=0` race link opening the
// setup. Host browser evidence (frontend.js race/gear pages).
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, unlockStore, url } from './browser/harness.mjs';

const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const front = async page => (await read(page)).front;
const wait = (page, predicate, arg, timeout = 60000) => page.waitForFunction(predicate, arg, { timeout });
const onPage = (page, name) => wait(page, name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name, name);
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }
// Down (wrapping) to a row: Back returns to the row a page was left by.
async function pick(page, action) {
  for (let i = 0; i < 12 && (await front(page)).choices[(await front(page)).selection] !== action; i++) await key(page, 'ArrowDown');
  const f = await front(page);
  assert.equal(f.choices[f.selection], action);
}
async function click(page, id) {
  const hit = (await front(page)).hits.find(h => h.id === id);
  assert.ok(hit, `hit ${id}`);
  await page.mouse.click(hit.x + hit.width / 2, hit.y + hit.height / 2); await nextFrames(page, 4);
}
const label = (f, action) => f.labels[f.choices.indexOf(action)];
const ROW = 2000, NEXT = 2403, START = 2411, SHARE = 2412, STEP_NEXT = 2440;
const shots = process.env.ATV_SHOTS;

const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  // Single Player picks follow the profile's unlocks (PROFILE_STORE.md).
  await unlockStore(page);
  await page.addInitScript(() => {
    if (sessionStorage.getItem('seeded')) return;
    sessionStorage.setItem('seeded', '1');
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.removeItem('atv-player-selection');
  });
  await page.goto(url('/play.html?uiDiagnostics=1'));
  await onPage(page, 'title');
  await page.mouse.click(8, 8);
  await key(page, 'Enter'); await onPage(page, 'main');
  await key(page, 'Enter'); await onPage(page, 'single');
  let f = await front(page);
  assert.deepEqual(f.choices, ['race', 'tracks', 'objective-courses', 'championship', 'games', 'garage']);
  assert.ok(f.race.courses >= 30, `race courses ${f.race.courses}`);
  await pick(page, 'race'); await key(page, 'Enter'); await onPage(page, 'race');
  f = await front(page);
  assert.deepEqual(f.choices.filter(c => c !== 'back'), ['racetrack', 'raceevent', 'racelaps', 'racecpu', 'racedifficulty', 'racestart', 'copyride']);
  assert.deepEqual([f.race.course, f.race.laps, f.race.cpu, f.race.difficulty, f.race.event], ['env01', 3, 3, 1, 'race']);
  assert.equal(label(f, 'raceevent'), 'Event: Single Race');
  // Steppers: Left/Right on value rows; the preview arrows step the course.
  await key(page, 'ArrowDown');
  await key(page, 'ArrowDown'); await key(page, 'ArrowLeft');
  await key(page, 'ArrowDown'); await key(page, 'ArrowRight');
  f = await front(page);
  assert.deepEqual([f.race.laps, f.race.cpu], [2, 4]);
  assert.equal(label(f, 'racelaps'), 'Laps: 2');
  await click(page, STEP_NEXT + f.choices.indexOf('racedifficulty'));
  assert.equal((await front(page)).race.difficulty, 2);
  await click(page, NEXT);
  f = await front(page);
  // The next course follows env01 in the list: env02, or env01's Enduro
  // variants when the content package carries them.
  assert.ok(['env02', 'env01endl', 'env01ends'].includes(f.race.course), f.race.course);
  assert.equal(f.choices[f.selection], 'racetrack');
  // Event tabs: Supercross lists only its courses.
  await key(page, 'PageUp');
  f = await front(page);
  assert.equal(f.race.filter, 'sx');
  assert.ok(f.race.course.startsWith('sx'), f.race.course);
  // PageDown wraps back to All; the course stays.
  await key(page, 'PageDown');
  f = await front(page);
  assert.equal(f.race.filter, 'all');
  const course = f.race.course;
  if (shots) await page.screenshot({ path: `${shots}/race-setup.png` });
  // Copy ride link: the race setup's link.
  await click(page, SHARE);
  await wait(page, () => JSON.parse(document.querySelector('#ui-state').textContent).front.notice);
  const notice = (await front(page)).notice;
  assert.ok(notice === 'Ride link copied.' || notice.includes(`activity=race&map=${course}&`) && /laps=2&cpu=4&difficulty=expert/.test(notice), notice);

  // Garage → Rider Gear: steppers and the dressed previews.
  // Back returns to Single Player on the Race row (MENU_FLOW.md).
  await key(page, 'Escape'); await onPage(page, 'single');
  f = await front(page);
  assert.equal(f.choices[f.selection], 'race');
  await pick(page, 'garage'); await key(page, 'Enter'); await onPage(page, 'garage');
  f = await front(page);
  await click(page, ROW + f.choices.indexOf('gear')); await onPage(page, 'gear');
  await wait(page, () => JSON.parse(document.querySelector('#ui-state').textContent).front.gear.catalog === 7);
  f = await front(page);
  assert.deepEqual(f.choices.filter(c => c !== 'back'), ['gearhelmet', 'gearjersey', 'gearpant', 'gearglove', 'gearboot', 'geargoggle', 'gearpaint']);
  assert.equal(label(f, 'gearhelmet'), 'Helmet: Original');
  await key(page, 'ArrowRight');
  await key(page, 'ArrowDown'); await key(page, 'ArrowRight');
  for (let i = 0; i < 5; i++) await key(page, 'ArrowDown');
  await key(page, 'ArrowRight'); await key(page, 'ArrowRight');
  f = await front(page);
  assert.equal(f.gear.gear[0], 1);
  assert.equal(f.gear.gear[3], 1);
  assert.equal(f.gear.paint, 2);
  assert.equal(label(f, 'gearpaint'), 'Quad colour: Orange');
  assert.notEqual(label(f, 'gearhelmet'), 'Helmet: Original');
  const saved = JSON.parse(await page.evaluate(() => localStorage.getItem('atv-player-selection')));
  assert.deepEqual([saved.appearance.gear[0], saved.appearance.gear[3], saved.appearance.paint], [1, 1, 2]);
  // Previews: the painted ATV and the menu rider in the chosen gear.
  await wait(page, () => {
    const m = JSON.parse(document.querySelector('#ui-state').textContent).front.models;
    return m[0] === 'vehicle:1:[1.0, 0.075]' && m[1]?.startsWith('rider:0:[1, 0, 0, 1');
  }, null, 120000);
  await nextFrames(page, 10);
  if (shots) await page.screenshot({ path: `${shots}/rider-gear.png` });
  await key(page, 'Escape'); await onPage(page, 'garage');
  f = await front(page);
  assert.equal(f.choices[f.selection], 'gear', 'Back returns to the Rider Gear row');
  await nextFrames(page, 10);
  if (shots) await page.screenshot({ path: `${shots}/garage-dressed.png` });

  // Start Race: the race loads with the chosen laps and CPU riders.
  await key(page, 'Escape'); await onPage(page, 'single');
  await pick(page, 'race'); await key(page, 'Enter'); await onPage(page, 'race');
  f = await front(page);
  assert.deepEqual([f.race.course, f.race.laps, f.race.cpu], [course, 2, 4]);
  await click(page, START);
  await wait(page, () => /race intro|LAP \d\/2/.test(document.querySelector('#status')?.textContent ?? ''), null, 180000);
  const race = JSON.parse(await page.evaluate(() => localStorage.getItem('atv-player-selection'))).race;
  assert.deepEqual([race.course, race.laps, race.cpu, race.difficulty], [course, 2, 4, 2]);
  await page.mouse.click(640, 400);
  await wait(page, () => /LAP 1\/2 · PLACE \d\/5/.test(document.querySelector('#status')?.textContent ?? ''), null, 120000);
  console.log(`  Race started: ${await page.evaluate(() => document.querySelector('#status').textContent.slice(0, 80))}`);
  assertNoErrors(errors, 'race and gear');
  await page.close();

  // `?menu=race` opens the setup; a race link without play shows its setup.
  const shortcut = await newPage(browser, { viewport: { width: 390, height: 844 } }, []);
  // Single Player picks follow the profile's unlocks (PROFILE_STORE.md).
  await unlockStore(shortcut);
  await shortcut.goto(url('/play.html?menu=race&uiDiagnostics=1'));
  await onPage(shortcut, 'race');
  if (shots) await shortcut.screenshot({ path: `${shots}/race-portrait.png` });
  await shortcut.goto(url('/play.html?mode=single&activity=race&map=nat02&laps=5&cpu=0&play=0&uiDiagnostics=1'));
  await onPage(shortcut, 'race');
  f = await front(shortcut);
  assert.deepEqual([f.race.course, f.race.laps, f.race.cpu], ['nat02', 5, 0]);
  await shortcut.close();
  console.log('PASS: Rust Single Race setup, Rider Gear and race links');
} finally {
  await browser.close();
}
