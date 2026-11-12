// A User Created Enduro course link (issue #546, research/RACING.md "User
// Created Enduro"): `wp=` carries the venue, laps and gates; opening it
// races that course alone on the long Enduro archive, and an invalid code
// is refused on the main menu's notice line.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

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
  assertNoErrors(errors);
  console.log(`PASS: Rust User Created Enduro link: env05 3 gates, 2 laps, solo; tampered code refused (${wp.length}-character code)`);
} finally {
  await browser.close();
}
