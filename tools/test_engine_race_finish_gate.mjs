// Original finish-gate lap rule in the browser (research/RACING.md "Finish gate
// and track window"): on SX06, whose TDF sets Track flag 4, a CPU that drives
// through the finish gate for the first time (about 1,860ft from the grid)
// is only armed by it (rider flag 0x10000) and still has no lap. Laps count
// at later crossings of the gate (one SX06 circuit, 3,745ft, later), not at a
// route checkpoint.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, optionalScreenshot, url } from './browser/harness.mjs';

const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 640, height: 360 } }, errors);
  // Intro and countdown (675 steps), then about 105s of racing.
  await page.addInitScript(() => localStorage.setItem('furyScript', JSON.stringify([{ frames: 7000, input: {} }])));
  await page.goto(url('/play.html?venue=sx06&race=1&laps=3&cpu=3&seed=7&weather=off&script=storage'));
  await page.waitForFunction(() => document.querySelector('#trace') || /error|fail/i.test(document.querySelector('#status')?.textContent), null, { timeout: 300000 });
  const trace = JSON.parse(await page.locator('#trace').textContent());
  const record = Array.isArray(trace) ? trace.at(-1) : trace;
  const race = record.race;
  assert.ok(race, 'race frame in the trace');
  assert.equal(race.track_flags & 4, 4, `SX06 Track flags ${race.track_flags}`);
  const gate = race.finish_at;
  assert.ok(gate > 1700 && gate < 2000, `SX06 finish gate ${gate} ft from the grid`);
  const CIRCUIT = 3745;
  const riders = race.standings.map(s => `${s.name} ${Math.round(s.progress)}ft ${s.laps} laps`).join(', ');
  const armed = race.standings.filter(s => s.progress > gate + 50 && s.progress < gate + CIRCUIT - 50);
  const counted = race.standings.filter(s => s.progress > gate + CIRCUIT + 50 && s.progress < gate + 2 * CIRCUIT - 50);
  assert.ok(armed.length + counted.length > 0, `a rider passed the gate: ${riders}`);
  for (const s of armed) assert.equal(s.laps, 0, `first gate pass is not a lap: ${riders}`);
  for (const s of counted) assert.equal(s.laps, 1, `second gate pass is lap 1: ${riders}`);
  await optionalScreenshot(page, 'ATV_RACE_FINISH_GATE_SCREENSHOT', 'sx06');
  assertNoErrors(errors);
  console.log(`PASS: Rust engine finish gate: SX06 gate at ${Math.round(gate)}ft (Track flag 4); ${riders}`);
} finally {
  await browser.close();
}
