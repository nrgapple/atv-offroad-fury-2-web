// Shared Rust HUD: toolbar (Pause/Camera/Keys), the GPS minimap, the keyboard
// controls card and the original speed meter, at desktop and portrait sizes.
// Host browser evidence; parity images come from work scripts, not this gate.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const state = page => page.locator('#ui-state').textContent().then(JSON.parse);
const wait = (page, predicate) => page.waitForFunction(predicate, null, { timeout: 60000 });
const hud = () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').toolbar?.hud;
const centre = hit => ({ x: hit.x + hit.width / 2, y: hit.y + hit.height / 2 });

const browser = await launch();
try {
  for (const viewport of [{ width: 1280, height: 720 }, { width: 390, height: 844 }]) {
    const errors = [];
    const page = await newPage(browser, { viewport }, errors);
    await page.goto(url('/play.html?venue=env01&uiDiagnostics=1'));
    await page.waitForSelector('#ui-state', { state: 'attached', timeout: 120000 });
    await wait(page, () => { const h = JSON.parse(document.querySelector('#ui-state').textContent).toolbar?.hud; return h?.minimap.visible; });
    let s = await state(page);
    const hits = Object.fromEntries(s.toolbar.hits.map(h => [h.id, h]));
    for (const id of [1, 2, 3, 10]) assert.ok(hits[id], `hit target ${id}`);
    for (const id of [1, 2, 3]) assert.ok(hits[id].width >= 44 && hits[id].height >= 44, '44px toolbar targets');
    const portrait = viewport.width < 600;
    // The 160px top-right map meets the labelled toolbar on phones: icon-only.
    assert.equal(s.toolbar.hud.compact, portrait, 'compact toolbar where the map meets it');
    if (portrait) assert.equal(hits[1].width, 44);
    assert.equal(s.toolbar.hud.helpOpen, true, 'keyboard card opens on the first keyboard ride');
    await page.keyboard.press('KeyH');
    await wait(page, () => !JSON.parse(document.querySelector('#ui-state').textContent).toolbar.hud.helpOpen);
    const keys = centre(hits[3]);
    await page.mouse.click(keys.x, keys.y);
    await wait(page, () => JSON.parse(document.querySelector('#ui-state').textContent).toolbar.hud.helpOpen);
    const map = centre(hits[10]);
    await page.mouse.click(map.x, map.y);
    await wait(page, () => JSON.parse(document.querySelector('#ui-state').textContent).toolbar.hud.minimap.headingUp);
    assert.equal(await page.evaluate(() => localStorage.getItem('atv-minimap-orientation')), 'heading-up');
    assert.equal(await page.evaluate(() => localStorage.getItem('atv2-keyboard-help-seen-v1')), '1');
    s = await state(page);
    console.log(`  ${viewport.width}x${viewport.height}: map build ${s.toolbar.hud.minimap.buildMs.toFixed(1)} ms, UI ${s.toolbar.prepareMs.toFixed(2)} ms`);
    assertNoErrors(errors);
    await page.close();

    // Minimap off: the original speed/preload meter replaces it.
    const meter = await newPage(browser, { viewport }, errors);
    await meter.addInitScript(() => { localStorage.setItem('atv-minimap-enabled', 'false'); localStorage.setItem('atv2-keyboard-help-seen-v1', '1'); });
    await meter.goto(url('/play.html?venue=env01&uiDiagnostics=1'));
    await meter.waitForSelector('#ui-state', { state: 'attached', timeout: 120000 });
    await nextFrames(meter, 30);
    s = await state(meter);
    assert.equal(s.toolbar.hud.meter, true, 'original speed meter shown');
    assert.equal(s.toolbar.hud.minimap.visible, false);
    assert.equal(s.toolbar.hud.helpOpen, false, 'seen card stays closed');
    assert.ok(!s.toolbar.hits.some(h => h.id === 10));
    assertNoErrors(errors);
    await meter.close();
  }
  console.log('PASS: Rust HUD toolbar, minimap, keyboard card and speed meter');
} finally {
  await browser.close();
}
