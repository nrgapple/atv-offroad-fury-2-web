// Supercross drop gate in the browser (research/RACING.md "Supercross drop
// gate"): SX01 declares its [DropGate] in the common sx01.toy, not the race
// TOY. Past GO the gate stands at its TOY position and has swung down through
// RotationAngleDegrees (40) about its local -X axis, as on Nationals.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, optionalScreenshot, url } from './browser/harness.mjs';

const POSITION = [138.375397, 89.999962, 104.317039];
const UP = [-0.473147, 0.707107, -0.525483];
const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 640, height: 360 } }, errors);
  // Intro and countdown (675 steps), then about 1.4 s past GO.
  await page.addInitScript(() => localStorage.setItem('furyScript', JSON.stringify([{ frames: 760, input: {} }])));
  await page.goto(url('/play.html?venue=sx01&race=1&laps=1&cpu=3&seed=7&weather=off&script=storage'));
  await page.waitForFunction(() => document.querySelector('#trace') || /error|fail/i.test(document.querySelector('#status')?.textContent), null, { timeout: 300000 });
  const trace = JSON.parse(await page.locator('#trace').textContent());
  const race = (Array.isArray(trace) ? trace.at(-1) : trace).race;
  assert.ok(race, 'race frame in the trace');
  assert.ok(race.elapsed > 0.2, `the race is past GO (${race.elapsed}s)`);
  const gate = race.drop_gate;
  assert.ok(Array.isArray(gate) && gate.length === 16, `SX01 has a posed drop gate: ${JSON.stringify(gate)}`);
  for (let i = 0; i < 3; i += 1) assert.ok(Math.abs(gate[12 + i] - POSITION[i]) < 1e-3, `gate position ${gate.slice(12, 15)}`);
  const y = gate.slice(4, 7);
  const length = Math.hypot(...y) * Math.hypot(...UP);
  const degrees = Math.acos(y.reduce((sum, v, i) => sum + v * UP[i], 0) / length) * 180 / Math.PI;
  assert.ok(Math.abs(degrees - 40) < 0.1, `gate swung ${degrees.toFixed(2)} degrees`);
  await optionalScreenshot(page, 'ATV_RACE_DROP_GATE_SCREENSHOT', 'sx01');
  assertNoErrors(errors);
  console.log(`PASS: Rust engine Supercross drop gate: SX01 gate down ${degrees.toFixed(2)} degrees at ${race.elapsed.toFixed(2)}s`);
} finally {
  await browser.close();
}
