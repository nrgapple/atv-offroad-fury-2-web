// Opt-in original-content severe-crash visual gate. Supplies authored throttle
// and held-stunt inputs through the existing WASM input interface; never sets
// velocity, pose, wreck state, damage or effects state. Results remain private.
import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';
import { captureSeveredMesh } from './capture_severed_mesh.mjs';
import { captureSimulation } from './capture_simulation.mjs';
import {
  captureCrashIsolation,
  verifyCrashIsolation,
  captureCrashDraws,
  verifyCrashImpacts
} from './crash_isolation.mjs';
import { assertNoErrors, base, launch, newPage, waitForAnimations } from './browser/harness.mjs';
const browser = await launch();
await mkdir('work/verification', { recursive: true });
try {
  const errors = [],
    page = await newPage(
      browser,
      { viewport: { width: Number(process.env.ATV_CRASH_WIDTH || 1280), height: 844 } },
      errors
    );
  await captureSeveredMesh(page);
  await captureCrashIsolation(page);
  await captureCrashDraws(page);
  await captureSimulation(page, 'crashFrames');
  await page.addInitScript(() => {
    if (!localStorage.getItem('atv-crash-mode')) localStorage.setItem('atv-crash-mode', 'gorn');
    const post = Worker.prototype.postMessage;
    let held = false;
    Worker.prototype.postMessage = function (message, ...args) {
      if (message.op === 50 && window.crashDriving) {
        const source = JSON.parse(new TextDecoder().decode(message.bytes)),
          f = window.atvTerrainStatus?.frame;
        if (f?.speed_magnitude > 55 && !f.grounded.some(Boolean) && f.wreck.phase === 'Riding') held = true;
        if (f?.wreck.phase !== 'Riding') held = false;
        source.count = 4;
        source.input = {
          ...source.input,
          throttle: 1,
          brake: 0,
          steer: 0,
          lean: 0,
          reverse: false,
          touch_drive: null,
          preload_axis: null,
          stunt_modifiers: held ? 1 : 0,
          stunt_x: 0,
          stunt_y: held ? 1 : 0
        };
        message = { ...message, bytes: new TextEncoder().encode(JSON.stringify(source)) };
      }
      return post.call(this, message, ...args);
    };
  });
  await page.goto(`${base}/play.html?drive=1&environment=env01`);
  await page.waitForFunction(() => window.atvTerrainStatus?.simulation, null, { timeout: 120000 });
  assert.equal(await page.evaluate(() => window.atvTerrainStatus.funnyCrashes), true);
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'Reset ATV', exact: true }).waitFor();
  await page.evaluate(() => (window.crashDriving = true));
  await page.getByRole('button', { name: 'Reset ATV', exact: true }).click();
  await page.waitForFunction(
    () =>
      window.atvTerrainStatus.frame.funny_crash?.detached_parts >= 1 &&
      window.atvTerrainStatus.frame.funny_crash.rider_node_matrices.length > 0 &&
      window.atvTerrainStatus.frame.funny_crash.severed_nodes.length >= 1 &&
      window.atvTerrainStatus.blood.draws === 1,
    null,
    { timeout: 60000 }
  );
  await page.evaluate(() => (window.crashDriving = false));
  const crash = await page.evaluate(() => window.atvTerrainStatus.frame);
  // Breakaway and dismemberment are local to the actual impacts, never everything at once.
  assert(crash.funny_crash.detached_parts >= 1 && crash.funny_crash.detached_parts <= 7);
  assert(
    crash.funny_crash.severed_nodes.length >= 1 && crash.funny_crash.severed_nodes.length <= 4,
    `severed ${crash.funny_crash.severed_nodes}`
  );
  assert(crash.funny_crash.rider_impact >= 37, `rider struck at ${crash.funny_crash.rider_impact} ft/s`);
  const blood = crash.funny_crash.blood;
  assert.equal(
    blood.stumps.length,
    14 * crash.funny_crash.severed_nodes.length,
    'body and limb stump caps per torn joint'
  );
  assert(blood.drops.length / 7 <= 120 && blood.splats.length / 10 <= 64);
  // Camera shake was removed (#72); only the crash boom remains.
  const feedback = await page.evaluate(() => ({
    shake: window.atvTerrainStatus.cameraShake,
    audio: window.atvTerrainStatus.audio
  }));
  assert.equal(feedback.shake, undefined);
  assert(feedback.audio.crashBooms > 0);
  const presentation = await page.evaluate(() => ({
    camera: window.atvTerrainStatus.cameraPresentation,
    removed: window.atvTerrainStatus.severedBridgeTriangles
  }));
  assert(presentation.removed > 0, 'Severed mesh bridge triangles must be removed from the rendered rider');
  assert(presentation.camera.crash, 'Crash camera must frame posed ATV and rider bounds');
  // The tight crash camera (#72) favors the rider; distant debris may leave frame.
  let mesh = await page.evaluate(() => window.severedMesh);
  assert(mesh.draws > 0);
  assert(mesh.actorDraws > mesh.draws);
  assert(mesh.maxEdge < 8, `Rendered severed triangles stretch ${mesh.maxEdge} feet`);
  const framing = null;
  assert([...blood.drops, ...blood.splats, ...blood.stumps].every(Number.isFinite));
  await page.screenshot({
    path: `${process.env.ATV_GORN_CAPTURES || 'private/reports/devlog-images'}/gorn-severe-host.png`
  });
  await page.waitForFunction(() => crashDraws.quad > 0 && crashDraws.rider > 0);
  if (process.env.ATV_CRASH_SCREENSHOT) await page.screenshot({ path: process.env.ATV_CRASH_SCREENSHOT });
  await page.waitForFunction(n => window.atvTerrainStatus.frame.reset_count > n, crash.reset_count, { timeout: 20000 });
  assert.equal(await page.evaluate(() => window.atvTerrainStatus.frame.funny_crash ?? null), null);
  mesh = await page.evaluate(() => window.severedMesh);
  assert(mesh.maxEdge < 8, 'No stretched severed triangles later in recovery');
  const frames = await page.evaluate(() => window.crashFrames);
  const originalIsolation = await verifyCrashIsolation(page);
  const extras = async () => {
    await page.keyboard.press('Escape');
    await page.getByRole('button', { name: 'Settings', exact: true }).click();
    await page.getByRole('button', { name: 'Extras', exact: true }).click();
  };
  await extras();
  await page.getByRole('button', { name: 'Crashes: Gorn (blood & limbs)', exact: true }).waitFor();
  await page.screenshot({
    path: `${process.env.ATV_GORN_CAPTURES || 'private/reports/devlog-images'}/gorn-settings-host.png`
  });
  await page.reload();
  await page.waitForFunction(() => window.atvTerrainStatus?.simulation, null, { timeout: 120000 });
  assert.equal(await page.evaluate(() => window.atvTerrainStatus.crashMode), 'gorn');
  await extras();
  await page.getByRole('button', { name: 'Crashes: Gorn (blood & limbs)', exact: true }).click();
  assert.equal(await page.evaluate(() => window.atvTerrainStatus.crashMode), 'original');
  await page.getByRole('button', { name: 'Crashes: Original', exact: true }).click();
  assert.equal(await page.evaluate(() => window.atvTerrainStatus.crashMode), 'crazy');
  for (const viewport of [
    { width: 390, height: 844 },
    { width: 844, height: 390 }
  ]) {
    await page.setViewportSize(viewport);
    for (const name of ['Crashes: Crazy']) {
      const box = await page.getByRole('button', { name, exact: true }).boundingBox();
      assert(box.height >= 44 && box.width >= 44);
    }
  }
  await page.setViewportSize({ width: Number(process.env.ATV_CRASH_WIDTH || 1280), height: 844 });
  // Animated nested menus: step back until the pause screen offers Resume.
  for (let k = 0; k < 6 && !(await page.getByRole('button', { name: 'Resume', exact: true }).isVisible()); k++) {
    await waitForAnimations(page);
    const back = page.getByRole('button', { name: 'Back', exact: true });
    if (await back.isVisible()) await back.click({ force: true });
  }
  await waitForAnimations(page);
  await page.getByRole('button', { name: 'Resume', exact: true }).click({ force: true });
  await page.waitForFunction(() => window.crashIsolation.compared > 100);
  const isolation = await verifyCrashIsolation(page);
  const impact = verifyCrashImpacts(frames);
  // The current Caribou route breaks parts before the rider separates (also on
  // production); the aerial intact-ATV case is covered by the Rust unit gate.
  console.log(`Aerial intact-ATV ragdoll frames on this route: ${impact.aerial}`);
  assert(impact.impacts > 0);
  assert(
    frames.every(
      s =>
        !s.frame.funny_crash ||
        [...s.frame.funny_crash.quad_node_matrices.flat(), ...s.frame.funny_crash.rider_node_matrices.flat()].every(
          Number.isFinite
        )
    )
  );
  assertNoErrors(errors);
  assert.equal(await page.evaluate(() => window.atvTerrainStatus.glError), 0);
  await writeFile(
    process.env.ATV_CRASH_TRACE || 'work/verification/camera-gorn.json',
    JSON.stringify({
      crash,
      isolation,
      originalIsolation,
      feedback,
      impact,
      presentation,
      mesh,
      framing,
      frames,
      errors
    })
  );
  console.log(
    'PASS: Gorn severe crash tears only the struck limb(s), renders stumps/droplets/splats in one draw, impact camera/audio, intact recovery, settings persistence and exact paired gameplay.'
  );
} finally {
  await browser.close();
}
