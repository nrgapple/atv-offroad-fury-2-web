// Original race start slots in the browser (research/RACING.md "Start slots"):
// on the ENV01 National with four CPUs, each load draws a 001f6350 slot
// permutation and every rider, the player included, stands in its own slot of
// the 001f8410 row: evenly spaced across the start, one row behind the gate.
// Different seeds put the player in different slots.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, optionalScreenshot, url } from './browser/harness.mjs';

const SEEDS = [11, 12, 13, 14];
const browser = await launch();
try {
  const playerSlots = [];
  for (const seed of SEEDS) {
    const errors = [];
    const page = await newPage(browser, { viewport: { width: 640, height: 360 } }, errors);
    // Through the pre-race camera into the countdown, which frames the grid.
    await page.addInitScript(() => localStorage.setItem('furyScript', JSON.stringify([{ frames: 470, input: {} }])));
    await page.goto(url(`/play.html?venue=env01&race=1&laps=1&cpu=4&seed=${seed}&weather=off&traceAll=1&script=storage`));
    await page.waitForFunction(() => document.querySelector('#trace') || /error|fail/i.test(document.querySelector('#status')?.textContent), null, { timeout: 180000 });
    const trace = JSON.parse(await page.locator('#trace').textContent());
    const first = trace[0];
    const slots = first.race.grid_slots;
    assert.deepEqual([...slots].sort(), [1, 2, 3, 4, 5], `slot permutation: ${slots}`);
    // Horizontal positions by slot: the player first, then each CPU.
    const bySlot = [];
    bySlot[slots[0]] = first.origin;
    first.race.opponents.forEach((o, i) => { bySlot[slots[i + 1]] = o.origin; });
    const step = [bySlot[2][0] - bySlot[1][0], bySlot[2][2] - bySlot[1][2]];
    const spacing = Math.hypot(...step);
    assert.ok(spacing > 7 && spacing < 16, `RiderSpacing ${spacing}`);
    for (let slot = 1; slot <= 5; slot++) {
      const expected = [bySlot[1][0] + step[0] * (slot - 1), bySlot[1][2] + step[1] * (slot - 1)];
      const miss = Math.hypot(bySlot[slot][0] - expected[0], bySlot[slot][2] - expected[1]);
      assert.ok(miss < 0.5, `seed ${seed} slot ${slot} off the row by ${miss.toFixed(2)} ft`);
    }
    playerSlots.push(slots[0]);
    await optionalScreenshot(page, 'ATV_RACE_GRID_SCREENSHOT', `seed${seed}`);
    assertNoErrors(errors);
    await page.close();
  }
  assert.ok(new Set(playerSlots).size > 1, `player slot varies with the seed: ${playerSlots}`);
  console.log(`PASS: Rust engine race grid: five riders in 001f8410 rows; player slots ${playerSlots} for seeds ${SEEDS}`);
} finally {
  await browser.close();
}
