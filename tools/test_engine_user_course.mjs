// User Created Enduro and the Waypoint Editor (issue #546, research/RACING.md
// "User Created Enduro and the Waypoint Editor"): `wp=` carries the venue,
// laps and gates and races that course alone; an invalid code is refused on
// the main menu's notice line; the editor drops gates where the quad stands,
// saves the course and races it.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, optionalScreenshot, url } from './browser/harness.mjs';

const manifest = await (await fetch(url('/content/manifest.json'))).json();
if (!manifest.files.some(f => f.path === '/TRACKS/TRACKSA/ENV05/E05_ENDL.RES')) {
  console.log('SKIP: the content package has no Enduro race archives yet');
  process.exit(0);
}

// atv_core::user_course: version 1, venue, laps, gate count, name, then
// u16 x, z (half feet) and heading (1/65536 turn) per gate, URL-safe base64.
function code(venue, laps, name, gates) {
  const text = new TextEncoder().encode(name);
  const bytes = [1, venue, laps, gates.length, text.length, ...text];
  for (const [x, z, heading] of gates) {
    for (const v of [Math.round(x * 2), Math.round(z * 2), Math.round(heading * 65536) % 65536]) bytes.push(v & 255, v >> 8);
  }
  return Buffer.from(bytes).toString('base64url');
}

const state = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const onPage = (page, name, timeout = 60000) => page.waitForFunction(name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name, name, { timeout });
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }
// Moves the menu selection to a row and accepts it.
async function choose(page, action) {
  const front = (await state(page)).front;
  const at = front.choices.indexOf(action);
  assert.ok(at >= 0, `${action} in ${front.choices}`);
  for (let i = front.selection; i < at; i++) await key(page, 'ArrowDown');
  for (let i = front.selection; i > at; i--) await key(page, 'ArrowUp');
  await key(page, 'Enter');
}
const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 960, height: 540 } }, errors);
  await page.addInitScript(() => localStorage.setItem('atv2-keyboard-help-seen-v1', '1'));
  // The venue's start stands on its terrain: three gates north of it.
  await page.goto(url('/play.html?venue=env05&uiDiagnostics=1&weather=off'));
  await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride?.start, null, { timeout: 300000 });
  const [sx, , sz] = (await state(page)).ride.start;
  const wp = code(5, 2, 'Test Loop', [[sx, sz + 60, 0], [sx + 60, sz + 120, 0.25], [sx, sz + 180, 0.5]]);
  await page.goto(url(`/play.html?mode=single&activity=race&map=env05&quad=1&rider=0&wp=${wp}&play=1&uiDiagnostics=1&weather=off`));
  await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride?.userCourseGates != null,
    null, { timeout: 300000 });
  await nextFrames(page, 10);
  const ride = (await state(page)).ride;
  assert.equal(ride.userCourseGates, 3, JSON.stringify(ride));
  assert.equal(ride.environment, 'env05');
  assert.equal(ride.raceLap?.laps, 2, `the course's laps: ${JSON.stringify(ride.raceLap)}`);
  assert.equal(ride.raceLap?.field, 1, 'a User Created Enduro races alone');
  assert.equal(ride.cpuDrawn, 0);
  // A tampered code (a gate count beyond 50) is refused.
  await page.goto(url(`/play.html?mode=single&activity=race&map=env05&wp=${code(5, 2, '', Array.from({ length: 51 }, (_, i) => [100 + 30 * i, 100, 0]))}&play=1&uiDiagnostics=1`));
  await page.waitForFunction(() => /Cannot open ride link/.test(JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.notice ?? ''),
    null, { timeout: 120000 });
  // The Waypoint Editor (Single Player's fourth row): a new slot rides the
  // venue; G drops a gate where the quad stands, U takes it back, and
  // Pause → Save Course keeps the course and opens its page.
  const editor = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await editor.addInitScript(() => localStorage.setItem('atv2-keyboard-help-seen-v1', '1'));
  await editor.goto(url('/play.html?uiDiagnostics=1&weather=off'));
  await onPage(editor, 'title');
  await editor.mouse.click(8, 8);
  await key(editor, 'Enter'); await onPage(editor, 'main');
  await key(editor, 'Enter'); await onPage(editor, 'single');
  await choose(editor, 'waypoint'); await onPage(editor, 'waypoint');
  let front = (await state(editor)).front;
  assert.deepEqual(front.choices.slice(0, 4), ['wpvenue', 'wpslot0', 'wpslot1', 'wpslot2']);
  assert.equal(front.labels[1], 'Race 1 (New)');
  await choose(editor, 'wpslot0');
  await editor.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride?.editorGates === 0, null, { timeout: 300000 });
  assert.equal((await state(editor)).ride.editorModels, true);
  await nextFrames(editor, 30);
  for (let i = 0; i < 4; i++) {
    await key(editor, 'KeyG');
    await editor.keyboard.down('KeyW'); await nextFrames(editor, 90); await editor.keyboard.up('KeyW');
    await nextFrames(editor, 30);
  }
  let gates = (await state(editor)).ride.editorGates;
  assert.ok(gates >= 3, `dropped ${gates} gates`);
  await key(editor, 'KeyU');
  assert.equal((await state(editor)).ride.editorGates, gates - 1, 'U takes the last gate back');
  gates -= 1;
  await key(editor, 'Escape');
  await editor.waitForFunction(() => JSON.parse(document.querySelector('#ui-state').textContent).toolbar?.menu?.page === 'pause');
  const pause = (await state(editor)).toolbar.menu;
  assert.equal(pause.choices[1], 'wpsave', pause.choices.join());
  await key(editor, 'ArrowDown'); await key(editor, 'Enter');
  await onPage(editor, 'waypoint-course', 120000);
  const saved = await editor.evaluate(() => JSON.parse(localStorage.getItem('fury-waypoints')));
  const venue = Object.keys(saved)[0];
  assert.equal(typeof saved[venue][0], 'string', JSON.stringify(saved));
  front = (await state(editor)).front;
  assert.equal(front.choices[0], 'wprace');
  // Race This Course: the saved gates as a User Created Enduro.
  await key(editor, 'Enter');
  await editor.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride?.userCourseGates != null, null, { timeout: 300000 });
  assert.equal((await state(editor)).ride.userCourseGates, gates);
  await optionalScreenshot(editor, 'ATV_USER_COURSE_SCREENSHOT', 'user-course-race');
  assertNoErrors(errors);
  console.log(`PASS: Rust Waypoint Editor: ${gates} gates saved and raced; User Created Enduro link: env05 3 gates, 2 laps, solo; tampered code refused (${wp.length}-character code)`);
} finally {
  await browser.close();
}
