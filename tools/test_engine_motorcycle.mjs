// A motorcycle in the Rust engine's browser build: the Suzuki RM125 (Garage
// selection 29) rides 600 scripted steps on ENV01 through its own two-wheel
// model, then holds. tools/compare_engine.mjs ride --quad 29 compares every
// step with the JavaScript game; a race on bike 29 reaches GO with three
// CPU quads.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, url } from './browser/harness.mjs';

const script = [
  { frames: 60, input: {} },
  { frames: 240, input: { throttle: 1 } },
  { frames: 150, input: { throttle: 1, steer: -0.6 } },
  { frames: 150, input: { brake: 1, steer: 0.4 } },
];
const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 640, height: 360 } }, errors);
  await page.addInitScript(text => localStorage.setItem('furyScript', text), JSON.stringify(script));
  await page.goto(url('/engine.html?venue=env01&quad=29&weather=off&traceAll=1&script=storage'));
  await page.waitForFunction(() => document.querySelector('#trace') || /error|fail/i.test(document.querySelector('#status')?.textContent), null, { timeout: 180000 });
  const trace = JSON.parse(await page.locator('#trace').textContent());
  assert.equal(trace.length, 600);
  const [first, last] = [trace[0], trace.at(-1)];
  assert.equal(first.wheel_compression.length, 2, 'two wheels recorded');
  const travelled = Math.hypot(last.origin[0] - first.origin[0], last.origin[2] - first.origin[2]);
  assert.ok(travelled > 100, `the bike rode ${travelled.toFixed(1)} ft`);
  assert.ok(Math.max(...trace.map(r => r.speed)) > 25, 'the bike accelerated');
  assert.equal(last.camera.mode, 'Follow');
  // A motorcycle race: the CPUs keep their quads and the ATV rider
  // (compare_engine.mjs race ... 29 compares every step).
  const race = await newPage(browser, { viewport: { width: 640, height: 360 } }, errors);
  await race.addInitScript(() => localStorage.setItem('furyScript', JSON.stringify([{ frames: 800, input: { throttle: 1 } }])));
  await race.goto(url('/engine.html?venue=env01&quad=29&race=1&laps=1&cpu=3&seed=12345&weather=off&traceAll=1&script=storage'));
  await race.waitForFunction(() => document.querySelector('#trace') || /error|fail/i.test(document.querySelector('#status')?.textContent), null, { timeout: 180000 });
  const racing = JSON.parse(await race.locator('#trace').textContent());
  assert.equal(racing[435].race.phase, 'countdown');
  assert.equal(racing[675].race.phase, 'racing');
  assert.equal(racing.at(-1).race.opponents.length, 3);
  assert.equal(racing.at(-1).wheel_compression.length, 2, 'the player rides the motorcycle');
  assertNoErrors(errors);
  console.log(`PASS: Rust engine motorcycle rode ${travelled.toFixed(0)} ft over ${trace.length} steps and raced three CPU quads to GO`);
} finally {
  await browser.close();
}
