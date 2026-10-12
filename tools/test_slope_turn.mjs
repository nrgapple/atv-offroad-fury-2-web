// Opt-in original-content steering route with measurable elevation change.
// Records riding and wreck frames separately; scenery contacts are not excluded.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { mkdir, writeFile } from 'node:fs/promises';
const { chromium } = createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || 'playwright');
await mkdir('work/verification', { recursive: true });
const browser = await chromium.launch({ headless: true });
try {
  const page = await browser.newPage({ viewport: { width: 1100, height: 700 } });
  const errors = []; page.on('pageerror', error => errors.push(error.message));
  await page.goto(new URL('/play.html?drive=1&environment=env01', process.env.ATV_TEST_URL || 'http://127.0.0.1:4180').href);
  await page.waitForFunction(() => window.atvTerrainStatus?.simulation, null, { timeout: 90000 });
  await page.bringToFront(); await page.locator('#game').focus();
  for (let i = 0; i < 3 && (await page.evaluate(() => window.atvTerrainStatus.performance.mode)) !== 'debug'; i++) await page.keyboard.press('F3');
  await page.evaluate(() => {
    window.turnFrames = []; let previous = -1;
    function record() {
      const state = window.atvTerrainStatus;
      if (state.simulationSteps !== previous) {
        previous = state.simulationSteps;
        window.turnFrames.push({ step: previous, frame: state.frame, input: state.input });
      }
      requestAnimationFrame(record);
    }
    requestAnimationFrame(record);
  });
  const start = await page.evaluate(() => window.atvTerrainStatus.simulationSteps);
  const until = step => page.waitForFunction(step => window.atvTerrainStatus.simulationSteps >= step, start + step, { timeout: 90000 });
  await page.keyboard.down('KeyW'); await until(90);
  await page.keyboard.down('KeyD'); await until(270);
  await page.keyboard.up('KeyD'); await page.keyboard.down('KeyA'); await until(450);
  await page.keyboard.up('KeyA'); await page.keyboard.up('KeyW');
  const frames = await page.evaluate(() => window.turnFrames);
  await writeFile(process.env.ATV_SLOPE_TRACE || 'work/verification/slope-turn-trace.json', JSON.stringify(frames));
  if (process.env.ATV_SLOPE_SCREENSHOT) await page.screenshot({ path: process.env.ATV_SLOPE_SCREENSHOT });
  const riding = frames.filter(sample => sample.frame.wreck.phase === 'Riding');
  const heights = riding.map(sample => sample.frame.origin[1]);
  assert(riding.length > 10, 'The route must include driving before any scenery wreck');
  assert(Math.max(...heights) - Math.min(...heights) > 5, 'The riding trace must cross measurable elevation');
  assert(riding.some(sample => sample.input.steer > 0) && riding.some(sample => sample.input.steer < 0), 'Both steering directions must reach the simulation');
  assert(frames.every(sample => [...sample.frame.origin, ...sample.frame.orientation].every(Number.isFinite)));
  assert.deepEqual(errors, []);
  assert.equal(await page.evaluate(() => window.atvTerrainStatus.glError), 0);
  console.log(JSON.stringify({ frames: frames.length, ridingFrames: riding.length, wreckFrames: frames.length - riding.length, ridingHeightRange: [Math.min(...heights), Math.max(...heights)], final: frames.at(-1).frame.origin }));
  console.log('PASS: both steering directions across uneven terrain remain finite; use terrain_clearance for riding tire overlap. This is not an isolated slope or original-game parity test.');
} finally { await browser.close(); }
