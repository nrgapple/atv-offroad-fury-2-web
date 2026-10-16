// Display → Graphics presets. Original must render exactly like the pre-preset
// renderer; Enhanced/High add interpolation, anisotropy, contact shadows and dirt
// without touching the simulation. Set ATV_GRAPHICS_BASELINE to a server running
// the previous main build for the pixel-identical gate (skipped otherwise).
import assert from 'node:assert/strict';
import { assertNoErrors, base, launch, newPage, button, waitForGame, waitForScreen, waitForSteps } from './browser/harness.mjs';
const baseline = process.env.ATV_GRAPHICS_BASELINE?.replace(/\/+$/, '') || null;
const maps = (process.env.ATV_GRAPHICS_MAPS || 'env01,sx01,env04').split(',');
const hooks = `
let graphicsAuditCamera;
window.__graphicsAudit = {
  set(p) { if (typeof graphicsSettings === 'undefined') return null; graphicsSettings.preset = p; graphicsSettings.onChange(); return graphicsSettings.resolved; },
  async draw(yaw = null) {
    focused = true; menuOpen = false; keys.clear(); pointers.clear();
    if (yaw === null) { graphicsAuditCamera ??= cameraFrame; cameraFrame = graphicsAuditCamera; }
    else { cameraFrame = null; camera.x = simStart.origin[0]; camera.y = simStart.origin[1] + 30; camera.z = simStart.origin[2] + 45; camera.yaw = yaw; camera.pitch = -.3; }
    render(10000); render(10000);
    const pixels = new Uint8Array(canvas.width * canvas.height * 4); gl.readPixels(0, 0, canvas.width, canvas.height, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
    const hash = [...new Uint8Array(await crypto.subtle.digest('SHA-256', pixels))].map(v => v.toString(16).padStart(2, '0')).join('');
    return { hash, resolution: [canvas.width, canvas.height], graphics: diagnostics.graphics ?? null, shadows: diagnostics.contactShadows ?? null, shadowMap: diagnostics.shadowMap ?? null, max: gl.getExtension('EXT_texture_filter_anisotropic') ? gl.getParameter(gl.getExtension('EXT_texture_filter_anisotropic').MAX_TEXTURE_MAX_ANISOTROPY_EXT) : 0 };
  }
};`;
const browser = await launch();
const errors = [];
const results = { baseline: Boolean(baseline), maps: [] };
async function frozen(site, env) {
  const page = await newPage(browser, { viewport: { width: 390, height: 844 }, deviceScaleFactor: 3, hasTouch: true }, errors);
  await page.addInitScript(() => { window.__auditFreeze = true; });
  await page.route('**/game.js*', async route => {
    const r = await route.fetch();
    const source = (await r.text())
      .replace('frame = requestAnimationFrame(render);', 'if (!window.__auditFreeze) frame = requestAnimationFrame(render);')
      .replace('if (simReady && !simulationPaused()) {', 'if (!window.__auditFreeze && simReady && !simulationPaused()) {');
    await route.fulfill({ response: r, body: source + hooks });
  });
  await page.goto(`${site}/play.html?drive=1&environment=${env}`);
  await waitForGame(page, { until: () => window.atvTerrainStatus.simulation && window.__graphicsAudit });
  return page;
}
const draw = (page, preset, yaw) => page.evaluate(async ({ preset, yaw }) => { window.__graphicsAudit.set(preset); return window.__graphicsAudit.draw(yaw); }, { preset, yaw });
try {
  for (const env of maps) {
    const candidate = await frozen(base, env), reference = baseline ? await frozen(baseline, env) : null;
    const row = { env, views: [] };
    for (const yaw of [null, 0, Math.PI / 2, Math.PI, Math.PI * 1.5]) {
      const original = await draw(candidate, 'original', yaw);
      assert.equal(original.graphics.preset, 'original');
      assert.equal(original.graphics.anisotropyApplied, 0);
      assert.equal(original.shadows, null, 'Original submits no contact shadows');
      assert.equal(original.shadowMap, null, 'Original renders no shadow map');
      if (reference) assert.equal(original.hash, (await draw(reference, 'original', yaw)).hash, `${env}/${yaw}: Original differs from the baseline renderer`);
      const view = { yaw, original: original.hash };
      for (const preset of ['enhanced', 'high']) {
        const enhanced = await draw(candidate, preset, yaw);
        assert.equal(enhanced.graphics.anisotropyApplied, Math.min(preset === 'high' ? 16 : 4, enhanced.max || 0));
        assert.equal(enhanced.shadows?.draws, 1, `${env}: one contact-shadow draw`);
        assert.equal(Boolean(enhanced.shadowMap?.draws), preset === 'high', `${env}: only High renders the shadow map`);
        assert(enhanced.shadows.casters >= 1);
        view[preset] = enhanced.hash;
      }
      if (yaw !== null) assert.notEqual(view.high, view.original, `${env}/${yaw}: High changes the frame`);
      // Switching back resets the samplers exactly.
      assert.equal((await draw(candidate, 'original', yaw)).hash, original.hash, `${env}/${yaw}: Original after High differs`);
      row.views.push(view);
    }
    if (env === maps[0]) {
      // Every new GPU resource survives context loss with High selected.
      await candidate.evaluate(() => { window.__graphicsAudit.set('high'); window.__lose = document.querySelector('#game').getContext('webgl2').getExtension('WEBGL_lose_context'); window.__lose.loseContext(); });
      await candidate.waitForFunction(() => !window.atvTerrainStatus.ready);
      await candidate.evaluate(() => window.__lose.restoreContext());
      await candidate.waitForFunction(() => window.atvTerrainStatus.ready);
      const restored = await draw(candidate, 'high', null);
      assert.equal(restored.shadows?.draws, 1);
      assert.equal(restored.graphics.anisotropyApplied, Math.min(16, restored.max || 0));
      row.contextRestore = true;
    }
    results.maps.push(row);
    await candidate.close(); await reference?.close();
  }
  // Live ride: Auto default, menu persistence, interpolation isolation, dirt and reset.
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await page.addInitScript(() => {
    window.__batches = [];
    const post = Worker.prototype.postMessage;
    Worker.prototype.postMessage = function (data, ...rest) {
      if (data?.op === 50) window.__batches.push(Object.keys(JSON.parse(new TextDecoder().decode(data.bytes)).input).sort().join(','));
      return post.call(this, data, ...rest);
    };
    window.__interpolated = 0; window.__lifts = 0;
    const sample = () => { const g = window.atvTerrainStatus?.graphics; if (g?.interpolated) window.__interpolated++; if (g?.wheelLift > 0) window.__lifts++; requestAnimationFrame(sample); };
    requestAnimationFrame(sample);
  });
  await page.goto(`${base}/play.html?drive=1&environment=env01`);
  await waitForGame(page, { until: () => window.atvTerrainStatus.simulation && window.atvTerrainStatus.graphics });
  const status = () => page.evaluate(() => window.atvTerrainStatus);
  const auto = (await status()).graphics;
  assert.equal(auto.setting, 'auto');
  assert(['enhanced', 'high'].includes(auto.preset));
  const resolved = auto.preset === 'high' ? 'High' : 'Enhanced';
  await button(page, 'Pause').click(); await waitForScreen(page, 'pause');
  await button(page, 'Settings').click(); await waitForScreen(page, 'settings');
  await button(page, 'Display').click(); await waitForScreen(page, 'hud-settings');
  await button(page, `Graphics: Auto (${resolved})`).click();
  await button(page, 'Graphics: Original').waitFor();
  assert.equal(await page.evaluate(() => localStorage.getItem('atv-graphics')), 'original');
  await button(page, 'Back').click(); await waitForScreen(page, 'settings');
  await button(page, 'Controls').click(); await waitForScreen(page, 'controls');
  await button(page, 'Vibration: On').click();
  await button(page, 'Vibration: Off').waitFor();
  assert.equal(await page.evaluate(() => localStorage.getItem('atv-vibration')), 'off');
  await page.reload();
  await waitForGame(page, { until: () => window.atvTerrainStatus.simulation && window.atvTerrainStatus.graphics });
  assert.equal((await status()).graphics.setting, 'original', 'preset persists across reload');
  assert.equal((await status()).vibration.enabled, false, 'vibration persists across reload');
  await page.locator('#game').focus();
  await page.keyboard.down('KeyW'); await waitForSteps(page, 180); await page.keyboard.up('KeyW');
  const originalRide = await page.evaluate(() => ({ interpolated: window.__interpolated, lifts: window.__lifts, batches: [...new Set(window.__batches)] }));
  assert.equal(originalRide.interpolated, 0, 'Original never interpolates');
  assert.equal(originalRide.lifts, 0, 'Original keeps the recovered wheel placement');
  await page.evaluate(() => { localStorage.setItem('atv-graphics', 'high'); localStorage.setItem('atv-vibration', 'on'); });
  await page.reload();
  await waitForGame(page, { until: () => window.atvTerrainStatus.simulation && window.atvTerrainStatus.graphics });
  await page.locator('#game').focus();
  await page.keyboard.down('KeyW'); await waitForSteps(page, 600); await page.keyboard.up('KeyW');
  const highRide = await page.evaluate(() => { const s = window.atvTerrainStatus; return { interpolated: window.__interpolated, lifts: window.__lifts, batches: [...new Set(window.__batches)], graphics: s.graphics, shadows: s.contactShadows, tracks: s.tireTracks, dust: s.lingeringDust, shadowMap: s.shadowMap }; });
  assert(highRide.graphics.wheelLinkage, 'the render core holds the quad linkage for wheel re-projection');
  assert(highRide.lifts > 0, 'sunk wheels are re-projected onto the visible ground');
  assert(highRide.tracks?.segments > 0 && highRide.tracks.draws === 1, 'tire tracks build behind the wheels in one draw');
  assert(highRide.dust?.live > 0 && highRide.dust.live <= 96, 'lingering dust stays within the High cap');
  assert(highRide.shadowMap?.draws > 0 && highRide.shadowMap.size === 2048, 'High renders the vehicle shadow map');
  assert(highRide.interpolated > 0, 'High presents interpolated ATV frames');
  assert.deepEqual(highRide.batches, originalRide.batches, 'the simulation receives the same input fields in every preset');
  assert(highRide.graphics.dirtLevel > 0, 'riding on dirt builds up the original dirt passes');
  assert(highRide.shadows?.draws === 1);
  await page.keyboard.press('KeyR');
  await page.waitForFunction(() => window.atvTerrainStatus.graphics?.dirtLevel === 0, null, { timeout: 15000 });
  results.live = { auto: auto.preset, originalRide, highRide: { interpolated: highRide.interpolated, lifts: highRide.lifts, dirtLevel: highRide.graphics.dirtLevel, tracks: highRide.tracks, dust: highRide.dust, shadowMap: highRide.shadowMap }, resetClearsDirt: true };
  // A real Supercross start-straight wreck rumbles a connected controller.
  const pad = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await pad.addInitScript(() => {
    window.__rumbles = [];
    const button = { pressed: false, touched: false, value: 0 };
    const gamepad = { connected: true, id: 'test pad', index: 0, mapping: 'standard', timestamp: 0, axes: [0, 0, 0, 0], buttons: Array.from({ length: 17 }, () => button),
      vibrationActuator: { type: 'dual-rumble', playEffect: (type, params) => { window.__rumbles.push({ type, ...params }); return Promise.resolve('complete'); } } };
    navigator.getGamepads = () => [gamepad, null, null, null];
  });
  await pad.goto(`${base}/play.html?drive=1&environment=sx01`);
  await waitForGame(pad, { until: () => window.atvTerrainStatus.simulation });
  await pad.locator('#game').focus();
  await pad.keyboard.down('KeyW');
  await pad.waitForFunction(() => window.atvTerrainStatus.frame?.wreck?.phase !== 'Riding' || window.__rumbles.length, null, { timeout: 30000 });
  await waitForSteps(pad, 10); await pad.keyboard.up('KeyW');
  const rumbles = await pad.evaluate(() => ({ effects: window.__rumbles, status: window.atvTerrainStatus.vibration }));
  assert(rumbles.effects.length >= 1 && rumbles.effects.every(e => e.type === 'dual-rumble' && e.strongMagnitude > 0 && e.strongMagnitude <= 1), 'impact rumbles the controller');
  results.vibration = rumbles;
  await pad.close();
  assertNoErrors(errors);
  console.log(JSON.stringify(results));
  console.log(`PASS: graphics presets${baseline ? ' (Original pixel-identical to baseline)' : ' (baseline comparison skipped: set ATV_GRAPHICS_BASELINE)'}`);
} finally {
  await browser.close();
}
