// Actual recovered-content GPS, settings, lifecycle and bounded A/B timing gate.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { mkdir, writeFile } from 'node:fs/promises';
const { chromium } = createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || 'playwright');
const base = process.env.ATV_TEST_URL || 'http://127.0.0.1:4173';
const out = process.env.ATV_MINIMAP_OUTPUT || 'work/verification/minimap';
await mkdir(out, { recursive: true });
const browser = await chromium.launch({ headless: true, args: ['--use-angle=metal'] });
const results = [], errors = [];
try {
  for (const viewport of [{ width: 1280, height: 720 }, { width: 390, height: 844 }].filter(v => !process.env.ATV_MINIMAP_WIDTHS || process.env.ATV_MINIMAP_WIDTHS.split(',').includes(String(v.width)))) {
    const page = await browser.newPage({ viewport, hasTouch: true, deviceScaleFactor: viewport.width === 390 ? 3 : 1 });
    page.on('pageerror', e => errors.push(e.message));
    await page.addInitScript(orientation => { localStorage.setItem('atv.diagnostics.v1', 'performance'); localStorage.setItem('atv-minimap-orientation', orientation); }, process.env.ATV_MINIMAP_ORIENTATION || 'north-up');
    await page.goto(`${base}/play.html?drive=1`);
    const state = () => page.evaluate(() => window.atvTerrainStatus);
    const click = name => page.getByRole('button', { name, exact: true }).click();
    const ready = async () => { try { await page.waitForFunction(() => window.atvTerrainStatus?.ready && window.atvTerrainStatus.minimap?.visible, null, { timeout: 90000 }); } catch (e) { console.log(JSON.stringify(await state())); throw e; } };
    await ready();
    // Synthetic focus is explicit: this is one host, not physical-phone timing.
    await page.evaluate(() => window.dispatchEvent(new Event('focus')));
    const map = page.locator('#freestyle-minimap'), canvas = map.locator('canvas');
    assert.equal(await map.evaluate(e => getComputedStyle(e).borderWidth), '0px');
    assert.equal(await map.evaluate(e => getComputedStyle(e).pointerEvents), 'auto');
    const pixelCheck = await canvas.evaluate(e => { const p = e.getContext('2d').getImageData(0, 0, e.width, e.height).data; return new Set(Array.from(p).filter((_, i) => i % 4 !== 3)).size; });
    assert(pixelCheck > 32, 'Actual aerial pixels have varied source terrain colors');
    await page.keyboard.press('F3'); await page.keyboard.press('F3');
    await page.screenshot({ path: `${out}/${viewport.width}-ride.png` });
    await page.keyboard.press('F3');
    await click('Pause'); await click('HUD settings'); await click('Minimap configuration');
    const slider = page.getByRole('slider', { name: 'Minimap size', exact: true });
    await slider.waitFor(); await slider.focus(); await page.keyboard.press('Home');
    await page.waitForFunction(() => window.atvTerrainStatus.minimap.size === 96);
    await click('Back'); await click('Back'); await click('Resume'); await ready();
    const small = await map.boundingBox();
    await click('Pause'); await click('HUD settings'); await click('Minimap configuration'); await slider.focus(); await page.keyboard.press('End');
    await page.waitForFunction(() => window.atvTerrainStatus.minimap.size === 224);
    await page.waitForTimeout(350); await page.screenshot({ path: `${out}/${viewport.width}-settings.png` });
    await click('Back'); await click('Back'); await click('Resume'); await ready();
    const large = await map.boundingBox(); assert(large.width > small.width);
    assert(large.x >= 0 && large.x + large.width <= viewport.width && large.y + large.height < viewport.height);
    const speed = await page.locator('#native-hud').getAttribute('data-speed'); assert(speed !== null);
    await click('Pause'); const frozen = (await state()).minimap.updates;
    await page.waitForTimeout(250);
    assert.equal((await state()).minimap.updates, frozen, 'Pause suppresses minimap work');
    await click('HUD settings'); await click('Minimap configuration'); await click('Freestyle minimap: On');
    assert.equal((await state()).minimap.backingBytes, 0, 'Off releases backing pixels');
    await click('Back'); await click('Back'); await click('Resume'); await page.waitForTimeout(250);
    assert.equal((await state()).minimap.updates, frozen, 'Off performs no map draws');
    await page.reload(); await page.waitForFunction(() => window.atvTerrainStatus?.ready, null, { timeout: 90000 });
    assert.equal((await state()).minimap.enabled, false); assert.equal((await state()).minimap.size, 224);
    assert.equal((await state()).minimap.buildSlices, 0, 'Saved Off skips course raster generation');
    await click('Pause'); await click('HUD settings'); await click('Minimap configuration'); await click('Freestyle minimap: Off');
    await slider.focus(); await page.keyboard.press('Home'); await click('Back'); await click('Back'); await click('Resume'); await ready();
    await page.setViewportSize({ width: 844, height: 390 }); await page.waitForTimeout(200);
    const rotated = await map.boundingBox(); assert(rotated.x + rotated.width <= 844 && rotated.y + rotated.height <= 390);
    await page.setViewportSize(viewport);
    // Stationary repeated alternating windows avoid incomparable drive routes.
    const timings = [];
    for (const size of [96, 160, 224]) {
      await click('Pause'); await click('HUD settings'); await click('Minimap configuration');
      await slider.fill(String(size)); await slider.dispatchEvent('input');
      await click('Back'); await click('Back'); await click('Resume');
      for (const enabled of (process.env.ATV_MINIMAP_QUICK === '1' ? [true, false] : [true, false, true, false])) {
        if ((await state()).minimap.enabled !== enabled) {
          await click('Pause'); await click('HUD settings'); await click('Minimap configuration'); await click(`Freestyle minimap: ${enabled ? 'Off' : 'On'}`); await click('Back'); await click('Back'); await click('Resume');
        }
        if (enabled) await ready();
        await page.waitForTimeout(2200);
        const before = await state(); await page.waitForTimeout(2500); const after = await state();
        const updates = after.minimap.updates - before.minimap.updates;
        timings.push({ size, enabled, fps: after.performance.fps, frameP95: after.performance.p95Ms, renderCpuMs: after.performance.renderCpuMs, mapCpuMs: updates ? (after.minimap.totalCpuMs - before.minimap.totalCpuMs) / updates : 0, updates, draws: after.drawCalls, backingBytes: after.minimap.backingBytes, resolution: after.minimap.resolution });
        assert(updates <= 52, 'Map update rate stays bounded at 20Hz');
        if (!enabled) assert.equal(updates, 0);
      }
    }
    await click('Pause'); await click('HUD settings'); await click('Minimap configuration'); await click('Freestyle minimap: Off'); await slider.fill('160'); await slider.dispatchEvent('input'); await click('Back'); await click('Back'); await click('Resume'); await ready();
    await page.locator('#game').focus(); const before = (await state()).frame.origin;
    await page.keyboard.down('KeyW'); await page.waitForTimeout(1500); await page.keyboard.up('KeyW');
    const after = await state(); assert(Math.hypot(...after.frame.origin.map((x, i) => x - before[i])) > 3, 'Real Rust rider drives with GPS enabled');
    assert.equal(after.glError, 0); assert.equal(after.minimap.extraSceneDraws, 0);
    assert(Math.hypot(after.minimap.center[0]-after.frame.origin[0], after.minimap.center[1]-after.frame.origin[2]) < 12, 'GPS follows the actual Rust origin');
    await page.evaluate(() => window.dispatchEvent(new Event('blur'))); const suspended=(await state()).minimap.updates; await page.waitForTimeout(200); assert.equal((await state()).minimap.updates,suspended,'Blur suppresses map updates');
    await page.evaluate(() => window.dispatchEvent(new Event('focus'))); await ready();
    const lost=await page.evaluate(()=>{const ext=document.getElementById('game').getContext('webgl2').getExtension('WEBGL_lose_context'); if(!ext)return false;window.minimapContext=ext;ext.loseContext();return true;});
    if(lost){await page.waitForFunction(()=>!window.atvTerrainStatus.ready && !window.atvTerrainStatus.minimap.visible);assert.equal((await state()).minimap.visible,false);await page.evaluate(()=>window.minimapContext.restoreContext());await ready();assert.equal((await state()).glError,0);}
    results.push({ viewport, build: after.minimap, small, large, timings }); await page.close();
  }
  assert.deepEqual(errors, []);
  await writeFile(`${out}/metrics.json`, JSON.stringify({ environment: 'One host Mac, Chromium Metal, synthetic focus; mobile viewport is not mobile hardware', results, errors }, null, 2));
  console.log(JSON.stringify(results.map(r => ({ viewport: r.viewport, buildMs: r.build.buildMs, maxBuildSliceMs: r.build.maxBuildSliceMs, timings: r.timings })), null, 2));
} finally { await browser.close(); }
