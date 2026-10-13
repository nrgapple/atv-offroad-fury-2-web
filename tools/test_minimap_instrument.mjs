// Actual HUD integration/corner/shape gate; original content remains opt-in.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { mkdir, writeFile } from 'node:fs/promises';
const { chromium, webkit } = createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || 'playwright');
const base = process.env.ATV_TEST_URL || 'http://127.0.0.1:4191';
const out = process.env.ATV_MINIMAP_OUTPUT || (process.env.ATV_UI_BROWSER === 'webkit' ? 'work/verification/minimap-instrument-webkit' : 'work/verification/minimap-instrument');
const engine = process.env.ATV_UI_BROWSER === 'webkit' ? webkit : chromium;
await mkdir(out, { recursive: true });
const browser = await engine.launch({ headless: true });
const errors = [], results = [];
try {
  for (const viewport of [{ width: 390, height: 844 }, { width: 320, height: 568 }, { width: 844, height: 390 }, { width: 1280, height: 720 }].filter(v => !process.env.ATV_MINIMAP_WIDTHS || process.env.ATV_MINIMAP_WIDTHS.split(',').includes(String(v.width)))) {
    const page = await browser.newPage({ viewport, hasTouch: true, deviceScaleFactor: 3 });
    const graph = [];
    page.on('request', request => {
      const url = new URL(request.url());
      if (url.origin === new URL(base).origin && /\.(js|css|wasm)$/.test(url.pathname)) graph.push({path:url.pathname,version:url.searchParams.get('v')});
    });
    page.on('pageerror', e => errors.push(e.message));
    const state = () => page.evaluate(() => window.atvTerrainStatus);
    const click = name => page.getByRole('button', { name, exact: true }).click();
    const ready = () => page.waitForFunction(() => window.atvTerrainStatus?.minimap.visible, null, { timeout: 90000 });
    const open = async () => { await click('Pause'); await click('HUD settings'); assert.equal(await page.getByRole('slider').count(), 0); await click('Minimap configuration'); await page.getByRole('slider', { name: 'Minimap zoom', exact: true }).waitFor(); };
    const resume = async () => { await click('Back'); await click('Back'); await click('Resume'); };
    await page.goto(`${base}/play.html?drive=1&environment=env02`);
    await ready();
    const map = page.locator('#freestyle-minimap'), native = page.locator('#native-hud');
    assert.equal((await state()).minimap.shape, 'circle'); assert.equal((await state()).minimap.corner, 'top-right');
    assert.equal(await native.getAttribute('data-speed-visible'), 'false');
    assert.equal(await native.evaluate(e => { const p = e.getContext('2d').getImageData(0, 0, e.width, e.height).data; return p.some((v, i) => i % 4 === 3 && v > 0); }), false, 'Standalone speed/preload pixels are absent');
    await page.waitForFunction(() => document.querySelector('#freestyle-minimap canvas').dataset.speed === window.atvTerrainStatus.frame.hud.speed_text);
    await page.screenshot({ path: `${out}/${viewport.width}-circle-top-right.png` });
    const initialBuilds = (await state()).minimap.builds;
    assert.equal((await state()).minimap.zoom, 50); assert.equal((await state()).minimap.rangeFeet, 1536);
    const originalBox = await map.boundingBox(), originalBytes = (await state()).minimap.backingBytes;
    for (const zoom of [25, 200, 50]) {
      await open();
      const slider = page.getByRole('slider', { name: 'Minimap zoom', exact: true });
      await slider.fill(String(zoom)); await slider.dispatchEvent('input');
      await resume(); await ready();
      await page.waitForFunction(zoom => window.atvTerrainStatus.minimap.zoom === zoom && window.atvTerrainStatus.minimap.scaleFeet === ({25:800,50:400,200:100})[zoom], zoom);
      assert.equal((await state()).minimap.rangeFeet, 76800 / zoom);
      assert.equal((await state()).minimap.builds, initialBuilds); assert.equal((await state()).minimap.backingBytes, originalBytes);
      assert.deepEqual(await map.boundingBox(), originalBox, 'Zoom changes world coverage, not screen size');
    }
    for (const corner of ['top-right', 'bottom-left']) for (const shape of ['circle', 'square']) {
      await open();
      if ((await state()).minimap.corner !== corner) await click(`Position: ${corner === 'top-right' ? 'Bottom left' : 'Top right'}`);
      if ((await state()).minimap.shape !== shape) await click(`Shape: ${shape === 'circle' ? 'Square' : 'Circular'}`);
      for (const input of await page.getByRole('slider').all()) assert((await input.boundingBox()).height >= 44);
      assert(await page.locator('#menu-buttons button').evaluateAll(buttons => buttons.every(b => b.getBoundingClientRect().height >= 43.99)));
      const slider = page.getByRole('slider', { name: 'Minimap size', exact: true });
      await slider.fill('224'); await slider.dispatchEvent('input');
      await resume(); await ready();
      assert.equal((await state()).minimap.builds, initialBuilds, 'Shape/corner/size reuse the cached aerial map');
      const box = await map.boundingBox();
      assert(box.x >= 0 && box.y >= 0 && box.x + box.width <= viewport.width && box.y + box.height <= viewport.height);
      if (corner === 'top-right') {
        assert(box.y <= 9 && Math.abs(viewport.width - box.x - box.width - 8) < 1);
        const toolbar = await page.locator('#toolbar').boundingBox();
        assert(box.x >= toolbar.x + toolbar.width || box.y >= toolbar.y + toolbar.height, 'GPS leaves the toolbar usable');
      } else assert(box.x <= 9 && Math.abs(viewport.height - box.y - box.height - 8) < 1);
      assert.equal(await map.getAttribute('data-shape'), shape);
      const radius = await map.evaluate(e => getComputedStyle(e).borderRadius);
      assert.equal(radius, shape === 'circle' ? '50%' : '4px');
      assert.equal(await map.evaluate(e => getComputedStyle(e).borderWidth), '0px');
      await page.screenshot({ path: `${out}/${viewport.width}-${shape}-${corner}.png` });
    }
    await page.reload(); await ready();
    assert.equal((await state()).minimap.shape, 'square'); assert.equal((await state()).minimap.corner, 'bottom-left'); assert.equal((await state()).minimap.size, 224);
    await open(); await click('Freestyle minimap: On'); await resume();
    await page.waitForFunction(() => document.querySelector('#native-hud').dataset.speedVisible === 'true');
    assert.equal((await state()).minimap.backingBytes, 0);
    assert(await native.evaluate(e => { const p = e.getContext('2d').getImageData(0, 0, e.width, e.height).data; return p.some((v, i) => i % 4 === 3 && v > 0); }), 'Off restores original speed HUD');
    await open(); await click('Freestyle minimap: Off'); await click('Shape: Square'); await click('Position: Bottom left');
    const smallest = page.getByRole('slider', { name: 'Minimap size', exact: true }); await smallest.fill('96'); await smallest.dispatchEvent('input');
    await resume(); await ready(); await page.screenshot({path:`${out}/${viewport.width}-small-footer.png`});
    await page.locator('#game').focus(); await page.keyboard.down('KeyW'); await page.keyboard.down('Space');
    await page.waitForFunction(() => window.atvTerrainStatus.frame.hud.preload_visible && document.querySelector('#freestyle-minimap canvas').dataset.preload === 'true', null, {timeout:15000});
    await page.keyboard.up('Space'); await page.keyboard.up('KeyW');
    assert.equal(await native.getAttribute('data-speed-visible'), 'false');
    assert.equal((await state()).glError, 0);
    if (process.env.ATV_RELEASE_COMMIT) {
      assert(graph.length >= 12); assert(graph.every(r => r.version === process.env.ATV_RELEASE_COMMIT), 'Public CSS/module/Worker/WASM graph uses one release');
    }
    results.push({ viewport, state: (await state()).minimap, graph }); await page.close();
  }
  assert.deepEqual(errors, []);
  await writeFile(`${out}/results.json`, JSON.stringify({ results, errors }, null, 2));
  console.log(`PASS: circular/square GPS, both corners, nested settings, saved preferences, no duplicate speed pixels, Off fallback, Rust preload and cache reuse at ${results.length} viewport(s).`);
} finally { await browser.close(); }
