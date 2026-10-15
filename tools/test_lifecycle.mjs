// Opt-in original-content regression. Persisted lifecycle events are synthetic;
// this proves recovery semantics, not that a browser actually used BFCache.
import assert from 'node:assert/strict';
import { writeFile } from 'node:fs/promises';
import { assertNoErrors, base, button, launch, newPage, waitForScreen } from './browser/harness.mjs';
const engine = process.env.ATV_TEST_ENGINE || 'webkit';
const browser = await launch({ engine });
try {
  const errors = [];
  const page = await newPage(
    browser,
    { viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true },
    errors
  );
  await page.addInitScript(() => {
    window.workerLifecycle = { created: 0, terminated: 0 };
    const NativeWorker = Worker;
    window.Worker = class extends NativeWorker {
      constructor(...args) {
        super(...args);
        window.workerLifecycle.created++;
      }
      terminate() {
        window.workerLifecycle.terminated++;
        return super.terminate();
      }
    };
  });
  await page.goto(new URL('/play.html?drive=1', base).href);
  await page.waitForFunction(() => window.atvTerrainStatus?.simulation, null, { timeout: 90000 });
  await page.locator('#game').tap({ position: { x: 60, y: 560 } });
  await page.keyboard.down('KeyW');
  await page.waitForFunction(
    () => window.atvTerrainStatus.frame.speed > 20 && !window.atvTerrainStatus.frame.grounded.some(Boolean),
    null,
    { timeout: 20000 }
  );
  const before = await page.evaluate(() => window.atvTerrainStatus);
  await page.evaluate(() => window.dispatchEvent(new PageTransitionEvent('pagehide', { persisted: true })));
  await page.keyboard.up('KeyW');
  await page.waitForTimeout(300); // Time-based on purpose: an already-submitted worker batch may finish.
  const suspended = await page.evaluate(() => window.atvTerrainStatus.simulationSteps);
  await page.waitForTimeout(300); // Time-based on purpose: proves suspended steps do not advance over wall-clock time.
  assert.equal(await page.evaluate(() => window.atvTerrainStatus.simulationSteps), suspended);
  await page.evaluate(() => {
    window.dispatchEvent(new PageTransitionEvent('pageshow', { persisted: true }));
    window.dispatchEvent(new PageTransitionEvent('pageshow', { persisted: true }));
  });
  await page.waitForFunction(step => window.atvTerrainStatus.simulationSteps > step + 15, suspended, {
    timeout: 10000
  });
  assert.equal(
    await page.evaluate(() => window.atvTerrainStatus.input.throttle),
    0,
    'Pre-hide held throttle must be cleared'
  );
  await page.locator('#game').tap({ position: { x: 60, y: 560 } });
  await page.keyboard.down('KeyW');
  await page.waitForFunction(
    step => window.atvTerrainStatus.simulationSteps > step + 60 && window.atvTerrainStatus.input.throttle === 1,
    suspended,
    { timeout: 15000 }
  );
  await page.keyboard.up('KeyW');
  const after = await page.evaluate(() => window.atvTerrainStatus);
  assert.notDeepEqual(after.frame.origin, before.frame.origin, 'Retained Rust body resumes movement');
  assert.deepEqual(await page.evaluate(() => window.workerLifecycle), { created: 1, terminated: 0 });
  assert.equal(after.simulation, true);
  assert.equal(after.glError, 0);
  assert(!after.audioError);
  await page.keyboard.press('Escape');
  await waitForScreen(page, 'pause');
  await button(page, 'Resume').tap();
  await page.waitForFunction(() => document.querySelector('#frontend').hidden);
  assertNoErrors(errors);
  if (process.env.ATV_LIFECYCLE_TRACE)
    await writeFile(
      process.env.ATV_LIFECYCLE_TRACE,
      JSON.stringify(
        {
          method: 'Synthetic persisted PageTransitionEvents; not actual BFCache navigation',
          engine,
          before,
          suspended,
          after,
          errors
        },
        null,
        2
      )
    );
  if (process.env.ATV_LIFECYCLE_SCREENSHOT) await page.screenshot({ path: process.env.ATV_LIFECYCLE_SCREENSHOT });
  console.log(
    `PASS ${engine}: retained Rust worker resumes from airborne persisted-page suspension, stale input clears, drive and pause/resume remain usable. Synthetic lifecycle regression only.`
  );
} finally {
  await browser.close();
}
