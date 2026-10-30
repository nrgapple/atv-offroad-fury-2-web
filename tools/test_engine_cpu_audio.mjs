// Race CPUs' sound in the Rust engine (research/AUDIO.md "CPU riders"): an
// ENV01 race with three CPUs plays their engines through the race sound
// table's sets with the set's distance attenuation, alongside the player's
// own engine. The diagnostics report only counts, levels and labels; the
// CPU vocal/impact/chassis cues are covered by atv-game unit tests.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, url } from './browser/harness.mjs';

const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 640, height: 360 } }, errors);
  await page.goto(url('/play.html?venue=env01&race=1&laps=1&cpu=3&seed=12345&weather=off&audioDiagnostics=1'));
  const state = () => page.evaluate(() => JSON.parse(document.documentElement.dataset.engineAudio ?? 'null'));
  await page.locator('canvas').waitFor({ state: 'visible', timeout: 180000 });
  // Activation: a click and a held throttle start the output and the ride.
  await page.waitForFunction(() => document.documentElement.dataset.engineAudio, null, { timeout: 180000 });
  await page.locator('canvas').click();
  await page.keyboard.down('KeyW');
  await page.waitForFunction(() => {
    const s = JSON.parse(document.documentElement.dataset.engineAudio);
    return s.started && s.cpu && s.cpu.engines >= 1 && s.cpu.levels.some(l => l[2][0] + l[2][1] > 0);
  }, null, { timeout: 120000 }).catch(async error => {
    console.error(await state(), errors);
    throw error;
  });
  const s = await state();
  await page.keyboard.up('KeyW');
  assert.ok(s.cpu.engines >= 1 && s.cpu.engines <= 3, `CPU engine voices: ${s.cpu.engines}`);
  assert.ok(s.cpu.engineStarts >= s.cpu.engines, `CPU engine starts: ${s.cpu.engineStarts}`);
  assert.ok(s.cpu.voices >= s.cpu.engines, `CPU-labelled mixer voices: ${s.cpu.voices}`);
  // Each voiced CPU reports [peer, distance to the nearest listener, gains].
  for (const [peer, distance, gains] of s.cpu.levels) {
    assert.ok(peer >= 200 && peer < 232, `CPU peer id ${peer}`);
    assert.ok(distance >= 0 && gains.every(g => g >= 0 && g <= 1), `CPU level ${distance} ${gains}`);
  }
  assertNoErrors(errors);
  console.log(`PASS: Rust engine CPU audio: ${s.cpu.engines} CPU engine voices (${s.cpu.engineStarts} starts, ${s.cpu.engineCulls} range stops), levels ${JSON.stringify(s.cpu.levels)}; cues vocals ${s.cpu.vocals}, impacts ${s.cpu.impacts}, chassis ${s.cpu.chassis}`);
} finally {
  await browser.close();
}
