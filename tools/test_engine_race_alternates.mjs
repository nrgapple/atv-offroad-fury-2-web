// Race distance on overlapping alternate lines (research/RACING.md "Course
// polygon"): sx14's 12->57 alternate crosses the circuit about 1,000ft further
// on. Riders passing that crossing must keep their race distance instead of
// being mapped back near the fork. The race path driver rides the player
// (`autopilot=1`) with three CPUs; every traced frame's standings may only
// move forward apart from small on-track wobbles, and riders must carry on
// over the bridge past checkpoint 57 (~1,545ft).
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, optionalScreenshot, url } from './browser/harness.mjs';

const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 480, height: 270 } }, errors);
  // Intro and countdown (675 steps), then about 32s of racing past the crossing.
  await page.addInitScript(() => localStorage.setItem('furyScript', JSON.stringify([{ frames: 2600, input: {} }])));
  await page.goto(url('/play.html?venue=sx14&race=1&laps=3&cpu=3&seed=7&weather=off&autopilot=1&script=storage&traceAll=1'));
  await page.waitForFunction(() => document.querySelector('#trace') || /error|fail/i.test(document.querySelector('#status')?.textContent), null, { timeout: 600000 });
  const trace = JSON.parse(await page.locator('#trace').textContent());
  assert.ok(Array.isArray(trace) && trace.length > 2000, 'per-frame trace');
  let previous = null;
  let furthest = 0;
  const drops = [];
  for (const record of trace) {
    if (!record.race) continue;
    const now = Object.fromEntries(record.race.standings.map(s => [s.name, s.progress]));
    if (previous) {
      for (const [name, progress] of Object.entries(now)) {
        if (progress < previous[name] - 100) drops.push(`${name} ${Math.round(previous[name])}->${Math.round(progress)} at step ${record.step}`);
      }
    }
    furthest = Math.max(furthest, ...Object.values(now));
    previous = now;
  }
  assert.deepEqual(drops, [], 'no rider loses race distance');
  assert.ok(furthest > 1700, `a rider passed the 12->57 crossing and the bridge (furthest ${Math.round(furthest)}ft)`);
  await optionalScreenshot(page, 'ATV_RACE_ALTERNATES_SCREENSHOT', 'sx14');
  assertNoErrors(errors);
  const last = trace.at(-1).race.standings.map(s => `${s.name} ${Math.round(s.progress)}ft`).join(', ');
  console.log(`PASS: Rust engine race distance on sx14 alternates: ${last}`);
} finally {
  await browser.close();
}
