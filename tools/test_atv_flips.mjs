// Optional Extras flips: keyboard preload jump, then L + lean back in the air.
// With Flips on the ATV backflips past ATV2's airborne ejection angle while
// still riding; with the default Off the same keys leave the original ride.
// Drives only through real Chromium key events; no physics/state injection.
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { captureSimulation } from './capture_simulation.mjs';
import { assertNoErrors, base, launch, newPage, waitForGame, waitForSteps } from './browser/harness.mjs';

await mkdir('work/verification', { recursive: true });
const browser = await launch();
// ATV01 AirPitchUpLimit is 85 degrees.
const AIR_LIMIT = 85 * Math.PI / 180;
try {
  for (const flips of [true, false]) {
    const errors = [];
    const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
    await page.addInitScript(on => {
      if (on) localStorage.setItem('atv-flips', 'unleashed');
      localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    }, flips);
    await captureSimulation(page, 'flipFrames');
    const url = new URL('/play.html', base);
    url.searchParams.set('drive', '1');
    url.searchParams.set('environment', 'env01');
    await page.goto(url.href);
    await waitForGame(page, { until: () => window.atvTerrainStatus.simulation });
    await page.bringToFront();
    await page.locator('#game').focus();
    await page.keyboard.down('KeyW');
    await page.waitForFunction(() => window.atvTerrainStatus.frame.speed > 30, null, { timeout: 30000 });
    // Space holds preload; release jumps.
    await page.keyboard.down('Space');
    await page.waitForFunction(() => window.atvTerrainStatus.frame.preload_armed, null, { timeout: 10000 });
    await waitForSteps(page, 20);
    await page.keyboard.up('Space');
    await page.waitForFunction(() => !window.atvTerrainStatus.frame.grounded.some(Boolean), null,
      { timeout: 5000, polling: 'raf' });
    await page.keyboard.up('KeyW');
    await page.keyboard.down('KeyL');
    await waitForSteps(page, 2);
    await page.keyboard.down('KeyE');
    if (flips && process.env.ATV_FLIP_SCREENSHOT) {
      // Mid-flip, upside down: the frame original ATV2 rules would eject.
      await page.waitForFunction(at => (window.atvTerrainStatus.frame.flip?.rotation ?? 0) > at
        || window.atvTerrainStatus.frame.grounded.some(Boolean), Number(process.env.ATV_FLIP_SCREENSHOT_AT ?? 1.8),
      { timeout: 15000, polling: 'raf' });
      await page.screenshot({ path: process.env.ATV_FLIP_SCREENSHOT });
    }
    await page.waitForFunction(() => window.atvTerrainStatus.frame.grounded.some(Boolean)
      || window.atvTerrainStatus.frame.wreck.phase !== 'Riding', null, { timeout: 15000, polling: 'raf' });
    const stunt = await page.evaluate(() => document.querySelector('canvas[data-renderer="rust"]')?.dataset.stunt);
    await page.keyboard.up('KeyE');
    await page.keyboard.up('KeyL');
    await waitForSteps(page, 30);
    const frames = await page.evaluate(() => window.flipFrames.map(({ frame }) => ({
      flip: frame.flip, events: frame.flip_events, phase: frame.wreck.phase,
      grounded: frame.grounded.some(Boolean), angular: frame.local_angular_velocity
    })));
    const air = frames.filter(f => !f.grounded);
    assert(air.length > 20, 'A real airborne interval must occur');
    if (flips) {
      assert(frames.some(f => f.events?.some(e => e.kind === 'started' && e.trick === 'Backflip')),
        'L + lean back in the air must start a backflip');
      const riding = frames.filter(f => f.phase === 'Riding' && f.flip?.active === 'Backflip');
      const peak = Math.max(...riding.map(f => f.flip.rotation));
      assert(peak > AIR_LIMIT, `backflip must rotate past ATV2's ${AIR_LIMIT.toFixed(2)} rad air limit while riding (peak ${peak.toFixed(2)})`);
      const outcome = frames.flatMap(f => f.events ?? []).find(e => e.kind === 'landed' || e.kind === 'bailed');
      console.log(`flips on: peak rotation ${(peak * 180 / Math.PI).toFixed(0)} deg, outcome ${outcome?.kind ?? 'none'}${outcome?.name ? ` (${outcome.name})` : ''}, HUD "${stunt}"`);
    } else {
      assert(frames.every(f => f.flip === null && (f.events ?? []).length === 0), 'Off must not construct flip state');
      // ATV2 lean alone settles at AeroMoment*ScaleX/AeroDamping = 1 rad/s
      // (the last samples before touchdown carry contact impacts).
      const start = frames.findIndex(f => !f.grounded), end = frames.findIndex((f, i) => i > start && f.grounded);
      const rates = frames.slice(start, end - 3).map(f => Math.abs(f.angular[0]));
      assert(Math.max(...rates) < 1.05, `Off keeps the recovered ATV2 air pitch rate (max ${Math.max(...rates).toFixed(2)})`);
      console.log('flips off: no flip state, original air rates');
    }
    assertNoErrors(errors);
    await page.close();
  }
  // Touch (default one-handed mode): a second finger in the air opens the
  // trick wheel; dragging into its outer-band Backflip sector and lifting picks it.
  {
    const errors = [];
    const page = await newPage(browser, { viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true }, errors);
    await page.addInitScript(() => localStorage.setItem('atv-flips', 'unleashed'));
    await captureSimulation(page, 'flipFrames');
    const url = new URL('/play.html', base);
    url.searchParams.set('drive', '1');
    url.searchParams.set('environment', 'env01');
    await page.goto(url.href);
    await waitForGame(page, { until: () => window.atvTerrainStatus.simulation });
    await page.bringToFront();
    await page.locator('#game').focus();
    const cdp = await page.context().newCDPSession(page);
    const touch = (type, points = []) => cdp.send('Input.dispatchTouchEvent', {
      type, touchPoints: points.map(point => ({ radiusX: 6, radiusY: 6, force: 1, ...point }))
    });
    const thumb = { id: 1, x: 200, y: 640 };
    await touch('touchStart', [thumb]);
    await page.waitForFunction(() => window.atvTerrainStatus.frame.preload_armed, null, { timeout: 45000, polling: 'raf' });
    await touch('touchEnd', [thumb]);
    await page.waitForFunction(() => !window.atvTerrainStatus.frame.grounded.some(Boolean), null, { timeout: 5000, polling: 'raf' });
    await touch('touchStart', [thumb]);
    const finger = { id: 2, x: 195, y: 330 };
    await touch('touchStart', [thumb, finger]);
    for (let i = 1; i <= 6; i++) await touch('touchMove', [thumb, { ...finger, y: finger.y + i * 28 }]);
    const selected = await page.locator('.touch-trick-choice.selected').textContent();
    if (process.env.ATV_FLIP_TOUCH_SCREENSHOT) await page.screenshot({ path: process.env.ATV_FLIP_TOUCH_SCREENSHOT });
    await touch('touchEnd', [{ ...finger, y: finger.y + 168 }]);
    await page.waitForFunction(() => window.flipFrames.some(s => s.frame.flip?.active === 'Backflip'), null, { timeout: 5000, polling: 'raf' });
    await touch('touchCancel');
    const frames = await page.evaluate(() => window.flipFrames.map(({ frame }) => frame.flip));
    assert.equal(selected, 'Backflip', 'dragging into the outer band previews Backflip');
    const peak = Math.max(...frames.map(f => f?.rotation ?? 0));
    console.log(`touch: wheel selected ${selected}, backflip started, rotation ${(peak * 180 / Math.PI).toFixed(0)} deg so far`);
    assertNoErrors(errors);
    await page.close();
  }
  console.log('PASS optional ATV flips: Unleashed backflip past original air limit, Off unchanged');
} finally {
  await browser.close();
}
