// The Rust engine's single-player race in the browser: the original ENV01
// National course with three CPU riders replays 1,000 steps and reports the
// pre-race camera, countdown and GO on the steps the JavaScript game uses
// (tools/compare_engine.mjs race compares every step), with the CPUs racing.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, url } from './browser/harness.mjs';

const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 640, height: 360 } }, errors);
  await page.addInitScript(() => localStorage.setItem('furyScript', JSON.stringify([{ frames: 1000, input: { keyboard_drive: { held: true, axis: 1 } } }])));
  await page.goto(url('/play.html?venue=env01&race=1&laps=1&cpu=3&seed=12345&weather=off&traceAll=1&script=storage'));
  await page.waitForFunction(() => document.querySelector('#trace') || /error|fail/i.test(document.querySelector('#status')?.textContent), null, { timeout: 180000 });
  const trace = JSON.parse(await page.locator('#trace').textContent());
  const phase = step => trace[step - 1].race.phase;
  assert.equal(phase(1), 'intro');
  assert.equal(trace[0].camera.mode, 'Intro');
  assert.equal(phase(435), 'intro');
  assert.equal(phase(436), 'countdown');
  assert.equal(phase(675), 'countdown');
  assert.equal(phase(676), 'racing');
  const last = trace.at(-1).race;
  assert.equal(last.opponents.length, 3);
  assert.ok(last.opponents.every(o => o.distance > 100), `CPUs race: ${last.opponents.map(o => o.distance)}`);
  assert.equal(last.standings.length, 4);
  assertNoErrors(errors);
  console.log(`PASS: Rust engine race: intro 1-435, countdown 436-675, GO at 676; CPUs at ${last.opponents.map(o => Math.round(o.distance))} ft`);
} finally {
  await browser.close();
}
