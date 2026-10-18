// Alpha "Arcade flips": the riding stick steers rotation in the air. On env01's
// Caribou ramp a held lean-back backflips and a held lean-forward front flips;
// letting go near a full turn lands them, and the HUD names the trick. Touch:
// lift the one-handed thumb for the preload jump, touch again in the air and
// pull down; a sideways drag leans, past full lock it spins. Drives only through real Chromium key and touch events.
import assert from 'node:assert/strict';
import { captureSimulation } from './capture_simulation.mjs';
import { assertNoErrors, base, launch, newPage, optionalScreenshot, waitForGame, waitForSteps } from './browser/harness.mjs';

// ATV01 AirPitchUpLimit is 85 degrees; Arcade must pass it while riding.
const AIR_LIMIT = (85 * Math.PI) / 180;
// Let go this far into the rotation; the bleed-out carries it near a full turn.
const LET_GO = 4.8;

async function open(browser, options) {
  const errors = [];
  const page = await newPage(browser, options, errors);
  await page.addInitScript(() => {
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    // Settings → Alpha → Arcade flips.
    localStorage.setItem('atv-alpha', JSON.stringify(['arcade-flips']));
  });
  await captureSimulation(page, 'arcadeFrames');
  const url = new URL('/play.html', base);
  url.searchParams.set('drive', '1');
  url.searchParams.set('environment', 'env01');
  await page.goto(url.href);
  await waitForGame(page, { until: () => window.atvTerrainStatus.simulation });
  await page.bringToFront();
  await page.locator('#game').focus();
  return { page, errors };
}

const rotated = (page, at) =>
  page.waitForFunction(
    at => {
      const frame = window.atvTerrainStatus.frame;
      return (
        (frame.flip?.arcade?.total ?? 0) > at || frame.grounded.some(Boolean) || frame.wreck.phase !== 'Riding'
      );
    },
    at,
    { timeout: 15000, polling: 'raf' }
  );
const landed = page =>
  page.waitForFunction(
    () => window.atvTerrainStatus.frame.grounded.some(Boolean) || window.atvTerrainStatus.frame.wreck.phase !== 'Riding',
    null,
    { timeout: 15000, polling: 'raf' }
  );

async function outcome(page) {
  // The name is credited after the landing holds for LAND_CONFIRM (0.25 s).
  await waitForSteps(page, 40);
  const stunt = await page.evaluate(() => document.querySelector('canvas[data-renderer="rust"]')?.dataset.stunt);
  const frames = await page.evaluate(() =>
    window.arcadeFrames.map(({ frame }) => ({
      flip: frame.flip,
      events: frame.flip_events,
      phase: frame.wreck.phase,
      grounded: frame.grounded.some(Boolean)
    }))
  );
  const riding = frames.filter(f => f.phase === 'Riding' && f.flip?.arcade?.engaged);
  const peak = Math.max(0, ...riding.map(f => Math.abs(f.flip.arcade.pitch)));
  const pop = frames.find(f => f.flip?.arcade?.engaged)?.flip.arcade;
  const result = frames.flatMap(f => f.events ?? []).find(e => e.kind === 'landed' || e.kind === 'bailed');
  return { stunt, peak, pop, result, frames };
}

const browser = await launch();
try {
  // The last case holds lean-back through touchdown: over-rotating still
  // wrecks, because the original landing gates decide (no landing assist).
  for (const [key, trick, sign, letGo] of [
    ['KeyE', 'Backflip', 1, LET_GO],
    ['KeyQ', 'Front Flip', -1, LET_GO],
    ['KeyE', null, 1, Infinity]
  ]) {
    const { page, errors } = await open(browser, { viewport: { width: 1280, height: 720 } });
    await page.keyboard.down('KeyW');
    await page.waitForFunction(() => window.atvTerrainStatus.frame.speed > 30, null, { timeout: 30000 });
    // Space holds preload; releasing near the lip jumps (and scores the pop).
    await page.keyboard.down('Space');
    await page.waitForFunction(() => window.atvTerrainStatus.frame.preload_armed, null, { timeout: 10000 });
    await waitForSteps(page, 20);
    await page.keyboard.up('Space');
    await page.waitForFunction(() => !window.atvTerrainStatus.frame.grounded.some(Boolean), null, {
      timeout: 5000,
      polling: 'raf'
    });
    await page.keyboard.up('KeyW');
    await page.keyboard.down(key);
    if (trick && process.env.ATV_ARCADE_SCREENSHOT) {
      // Mid-rotation, upside down, still riding.
      await rotated(page, Math.PI);
      await optionalScreenshot(page, 'ATV_ARCADE_SCREENSHOT', trick.replace(' ', '-').toLowerCase());
    }
    await rotated(page, letGo);
    await page.keyboard.up(key);
    await landed(page);
    const { stunt, peak, pop, result, frames } = await outcome(page);
    if (!trick) {
      assert.equal(result?.kind, 'bailed', `holding through touchdown over-rotates and wrecks (${JSON.stringify(result)})`);
      assert.notEqual(frames.at(-1).phase, 'Riding');
      console.log(`keyboard ${key} held through landing: wrecked after ${((peak * 180) / Math.PI).toFixed(0)} deg`);
      assertNoErrors(errors);
      await page.close();
      continue;
    }
    const pitches = frames.filter(f => f.flip?.arcade?.engaged).map(f => f.flip.arcade.pitch);
    assert(
      pitches.length > 20 && pitches.every(p => Math.sign(p) !== -sign || Math.abs(p) < 0.2),
      `${key} rotates the ${trick} way`
    );
    assert(peak > AIR_LIMIT, `${trick} passes ATV2's air limit while riding (peak ${peak.toFixed(2)})`);
    assert.equal(result?.kind, 'landed', `${trick} let go near a full turn lands (${JSON.stringify(result)})`);
    assert.equal(result.name, trick);
    assert.equal(stunt, trick, 'HUD names the landed trick');
    console.log(
      `keyboard ${key}: ${trick} landed, peak ${((peak * 180) / Math.PI).toFixed(0)} deg, lip pop ${pop?.pop?.toFixed(2)} (preload age ${pop?.pop_age?.toFixed(3) ?? 'none'} s)`
    );
    assertNoErrors(errors);
    await page.close();
  }

  // Touch (default one-handed): hold the thumb into the ramp, lift for the
  // preload jump, then touch again in the air and pull down to backflip.
  {
    const { page, errors } = await open(browser, {
      viewport: { width: 390, height: 844 },
      hasTouch: true,
      isMobile: true
    });
    const cdp = await page.context().newCDPSession(page);
    const touch = (type, points = []) =>
      cdp.send('Input.dispatchTouchEvent', {
        type,
        touchPoints: points.map(point => ({ radiusX: 6, radiusY: 6, force: 1, ...point }))
      });
    const thumb = { id: 1, x: 200, y: 600 };
    await touch('touchStart', [thumb]);
    await page.waitForFunction(() => window.atvTerrainStatus.frame.preload_armed, null, {
      timeout: 45000,
      polling: 'raf'
    });
    await waitForSteps(page, 20);
    await touch('touchEnd', [thumb]);
    await page.waitForFunction(() => !window.atvTerrainStatus.frame.grounded.some(Boolean), null, {
      timeout: 5000,
      polling: 'raf'
    });
    const air = { id: 2, x: 200, y: 520 };
    await touch('touchStart', [air]);
    for (let i = 1; i <= 8; i++) await touch('touchMove', [{ ...air, y: air.y + i * 10 }]);
    // The Unleashed thumb-flip cue stays off: Arcade replaces those gestures.
    const cue = await page.evaluate(() => document.querySelector('.touch-rider-marker span')?.textContent ?? '');
    assert(!cue.includes('LIFT →'), `no Unleashed flip cue in Arcade (${cue})`);
    await rotated(page, LET_GO);
    await touch('touchEnd', [{ ...air, y: air.y + 80 }]);
    await landed(page);
    const { peak, result } = await outcome(page);
    assert(peak > AIR_LIMIT, `touch pull-down backflips past the air limit while riding (peak ${peak.toFixed(2)})`);
    console.log(
      `touch: pull-down in the air rotated ${((peak * 180) / Math.PI).toFixed(0)} deg, outcome ${result?.kind ?? 'none'}${result?.name ? ` (${result.name})` : ''}`
    );
    assertNoErrors(errors);
    await page.close();
  }
  // Touch: a full-lock sideways drag in the air only leans (the original air
  // control); dragging past full lock starts the 360.
  {
    const { page, errors } = await open(browser, {
      viewport: { width: 390, height: 844 },
      hasTouch: true,
      isMobile: true
    });
    const cdp = await page.context().newCDPSession(page);
    const touch = (type, points = []) =>
      cdp.send('Input.dispatchTouchEvent', {
        type,
        touchPoints: points.map(point => ({ radiusX: 6, radiusY: 6, force: 1, ...point }))
      });
    const thumb = { id: 1, x: 200, y: 600 };
    await touch('touchStart', [thumb]);
    await page.waitForFunction(() => window.atvTerrainStatus.frame.preload_armed, null, {
      timeout: 45000,
      polling: 'raf'
    });
    await waitForSteps(page, 20);
    await touch('touchEnd', [thumb]);
    await page.waitForFunction(() => !window.atvTerrainStatus.frame.grounded.some(Boolean), null, {
      timeout: 5000,
      polling: 'raf'
    });
    // Default sensitivity 85: full lock at 82 px, the 360 from about 107 px.
    const air = { id: 2, x: 120, y: 520 };
    await touch('touchStart', [air]);
    for (let i = 1; i <= 9; i++) await touch('touchMove', [{ ...air, x: air.x + i * 10 }]);
    await waitForSteps(page, 15);
    const lean = await page.evaluate(() => {
      const frame = window.atvTerrainStatus.frame;
      return {
        steer: window.atvTerrainStatus.input?.steer,
        spin: window.atvTerrainStatus.input?.arcade_spin,
        engaged: frame.flip?.arcade?.engaged,
        grounded: frame.grounded.some(Boolean),
        cue: document.querySelector('.touch-rider-marker span')?.textContent ?? ''
      };
    });
    assert(!lean.grounded, 'still airborne after the lean drag');
    assert.equal(lean.engaged, false, `full-lock drag leans without spinning (${JSON.stringify(lean)})`);
    for (let i = 1; i <= 8; i++) await touch('touchMove', [{ ...air, x: air.x + 90 + i * 10 }]);
    await page.waitForFunction(
      () => {
        const frame = window.atvTerrainStatus.frame;
        return Math.abs(frame.flip?.arcade?.yaw ?? 0) > 1 || frame.grounded.some(Boolean);
      },
      null,
      { timeout: 5000, polling: 'raf' }
    );
    const spun = await page.evaluate(() => ({ ...window.atvTerrainStatus.frame.flip?.arcade, cue: document.querySelector('.touch-rider-marker span')?.textContent ?? '' }));
    await touch('touchEnd', [{ ...air, x: air.x + 170 }]);
    assert(spun.engaged && spun.yaw > 1, `dragging past full lock spins right (${JSON.stringify(spun)})`);
    console.log(
      `touch: full-lock drag leaned (steer ${lean.steer?.toFixed(2)}, cue ${lean.cue}); past full lock spun ${((spun.yaw * 180) / Math.PI).toFixed(0)} deg (cue ${spun.cue})`
    );
    assertNoErrors(errors);
    await page.close();
  }
  console.log('PASS Arcade flips: steered backflip and front flip landed with HUD names, over-rotation wrecks; touch rotates in the air, sideways drag leans until past full lock');
} finally {
  await browser.close();
}
