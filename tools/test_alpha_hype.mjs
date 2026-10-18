// Settings → Alpha hype effects on a real loaded course. The quad is placed in
// the air with an upward velocity through the worker's start-position op, then
// the recovered simulation flies and lands it; nothing else is injected.
import assert from 'node:assert/strict';
import { assertNoErrors, base, button, launch, newPage, optionalScreenshot, waitForGame, waitForScreen } from './browser/harness.mjs';

const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await page.addInitScript(() => {
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.setItem('atv-alpha', JSON.stringify(window.alphaOverride ?? ['air-slowmo', 'big-air-camera', 'music-muffle', 'apex-glare', 'landing-burst', 'heavy-mud']));
    const NativeWorker = Worker;
    window.Worker = class extends NativeWorker { constructor(...args) { super(...args); window.alphaWorker = this; } };
    window.hypeSamples = [];
    const sample = () => {
      const s = window.atvTerrainStatus;
      if (s?.hype && s.frame) window.hypeSamples.push({ ...s.hype, steps: s.simulationSteps, now: performance.now(), dirt: s.graphics?.dirtLevel ?? 0, grounded: s.frame.grounded.some(Boolean) });
      requestAnimationFrame(sample);
    };
    requestAnimationFrame(sample);
  });
  await page.goto(`${base}/play.html?mode=single&map=${process.env.ATV_ALPHA_MAP || 'env01'}`);
  await waitForGame(page, { until: () => window.atvTerrainStatus?.simulation && document.querySelector('#frontend').hidden && window.atvTerrainStatus.hype });
  // Let the start settle on the ground, then launch about 40 ft/s upwards.
  await page.waitForFunction(() => window.atvTerrainStatus.frame.grounded.every(Boolean), null, { timeout: 30000 });
  const launched = await page.evaluate(() => new Promise((resolve, reject) => {
    const worker = window.alphaWorker, id = 1000000001, frame = window.atvTerrainStatus.frame, [w, x, y, z] = frame.orientation;
    const yaw = Math.atan2(2 * (w * y + x * z), 1 - 2 * (y * y + z * z)), forward = [Math.sin(yaw), Math.cos(yaw)];
    const listener = ({ data }) => { if (data.id !== id) return; worker.removeEventListener('message', listener); data.error ? reject(new Error(data.error)) : resolve({ origin: frame.origin, yaw }); };
    worker.addEventListener('message', listener);
    window.hypeSamples.length = 0;
    worker.postMessage({ id, op: 23, bytes: new TextEncoder().encode(JSON.stringify({ origin: [frame.origin[0], frame.origin[1] + 3, frame.origin[2]], yaw, velocity: [forward[0] * 25, 42, forward[1] * 25] })) });
  }));
  // Screenshot near the apex for manual inspection, then wait for touchdown.
  await page.waitForFunction(() => window.atvTerrainStatus.hype.slowmos > 0 && window.atvTerrainStatus.hype.timeScale < .5, null, { timeout: 20000 });
  await page.waitForTimeout(250);
  await optionalScreenshot(page, 'ATV_ALPHA_SCREENSHOTS', 'apex');
  await page.waitForFunction(() => window.atvTerrainStatus.hype.landings > 0, null, { timeout: 20000 });
  await page.waitForTimeout(60);
  await optionalScreenshot(page, 'ATV_ALPHA_SCREENSHOTS', 'landing');
  await page.waitForTimeout(350);
  await optionalScreenshot(page, 'ATV_ALPHA_SCREENSHOTS', 'landing-later');
  await page.waitForTimeout(1200);
  const samples = await page.evaluate(() => window.hypeSamples);
  const max = key => Math.max(...samples.map(s => s[key]));
  const minScale = Math.min(...samples.map(s => s.timeScale));
  const landing = samples.at(-1).landing;
  // Slow motion: real time per 60 Hz step stretches to at least ~2x during the apex.
  const slow = samples.filter(s => s.timeScale < .5);
  const stretch = slow.length > 2 ? (slow.at(-1).now - slow[0].now) / Math.max(1, slow.at(-1).steps - slow[0].steps) : 0;
  const result = { glareSun: samples.find(s => s.glareSun)?.glareSun ?? null, launched, minScale, stretchMsPerStep: stretch, maxAir: max('air'), maxGlare: max('glare'), glareDraws: max('glareDraws'), maxMuffle: max('musicMuffle'),
    hypeMuffle: max('muffle'), minDip: Math.min(...samples.map(s => s.dip)), burstLive: max('burstLive'), landing, dirtStart: samples[0].dirt, dirtEnd: samples.at(-1).dirt, slowmos: samples.at(-1).slowmos };
  console.log(JSON.stringify(result));
  assert.equal(result.slowmos, 1, 'one apex slow motion');
  assert(minScale <= .32, `slow motion scale ${minScale}`);
  assert(stretch > 30, `real ms per step during slow motion ${stretch}`);
  assert(samples.at(-1).timeScale === 1, 'full speed after landing');
  assert(result.maxAir > .5, 'big-air framing engaged');
  assert(result.maxGlare > .3 && result.glareDraws === 1, 'apex glare drawn');
  assert(result.hypeMuffle > .5, 'muffle engaged in the air');
  assert(landing?.severity > 20, 'hard landing detected');
  assert(result.minDip < -.2, 'landing camera compression');
  assert(result.burstLive > 10, 'landing burst particles');
  assert(result.dirtEnd > result.dirtStart, 'heavy mud splat on landing');
  // Alpha page lists every experiment with its saved state.
  await page.keyboard.press('Escape');
  await waitForScreen(page, 'pause');
  await button(page, 'Settings').click();
  await waitForScreen(page, 'settings');
  await button(page, 'Alpha (experimental)').click();
  await waitForScreen(page, 'alpha');
  const rows = await page.locator('.pause-panel button').evaluateAll(b => b.map(x => x.getAttribute('aria-label')));
  assert.deepEqual(rows, ['Air slow-mo: On', 'Big-air camera: On', 'Speed & air muffle: On', 'Apex sun glare: On', 'Landing burst: On', 'Heavy mud: On', 'Arcade flips: Off', 'Back']);
  await button(page, 'Air slow-mo: On').click();
  await page.waitForFunction(() => JSON.parse(localStorage.getItem('atv-alpha')).includes('air-slowmo') === false);
  await optionalScreenshot(page, 'ATV_ALPHA_SCREENSHOTS', 'menu');
  assertNoErrors(errors);
} finally {
  await browser.close();
}
