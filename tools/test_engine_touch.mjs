// Shared Rust overlay: real Chromium multitouch through winit, plus WebKit
// pointer dispatch. Host browser input evidence, not physical-phone validation.
import assert from 'node:assert/strict';
import path from 'node:path';
import { mkdir } from 'node:fs/promises';
import { assertNoErrors, launch, newPage, nextFrames, url, writeJson } from './browser/harness.mjs';

const report = [];
const state = page => page.locator('#ui-state').textContent().then(JSON.parse);
const frames = async (page, n = 3) => nextFrames(page, n);
const wait = (page, predicate) => page.waitForFunction(predicate, null, { timeout: 20000 });
const center = hit => ({ x: hit.x + hit.width / 2, y: hit.y + hit.height / 2 });

for (const engine of ['chromium', 'webkit']) {
  const browser = await launch({ engine });
  try {
    for (const viewport of [{ width: 390, height: 844 }, { width: 844, height: 390 }]) {
      console.log(`Checking ${engine} ${viewport.width}x${viewport.height}`);
      const errors = [];
      const page = await newPage(browser, { viewport, deviceScaleFactor: 3, hasTouch: true, isMobile: true }, errors);
      await page.goto(url('/engine.html?venue=env01&uiDiagnostics=1'));
      await page.waitForSelector('#ui-state', { state: 'attached', timeout: 120000 });
      // The toolbar turns icon-only once the GPS map meets it; read targets after.
      await wait(page, () => JSON.parse(document.querySelector('#ui-state').textContent).toolbar?.hud?.minimap.visible);
      let s = await state(page);
      assert.equal(s.paused, false);
      const hits = Object.fromEntries(s.toolbar.hits.map(h => [h.id, h]));
      for (const id of [1, 2]) assert.ok(hits[id].width >= 44 && hits[id].height >= 44, '44px utility targets');
      const pause = center(hits[1]);
      const camera = center(hits[2]);
      const primary = { x: viewport.width * .6, y: viewport.height * .65 };
      const points = new Map();
      const cdp = engine === 'chromium' ? await page.context().newCDPSession(page) : null;
      async function pointer(phase, id, at) {
        if (cdp) {
          if (phase === 'down' || phase === 'move') points.set(id, { id, ...at }); else points.delete(id);
          await cdp.send('Input.dispatchTouchEvent', {
            type: phase === 'down' ? 'touchStart' : phase === 'move' ? 'touchMove' : phase === 'cancel' ? 'touchCancel' : 'touchEnd',
            touchPoints: phase === 'cancel' ? [] : phase === 'up' ? [{ id, ...at }] : [...points.values()],
          });
          if (phase === 'cancel') points.clear();
        } else {
          await page.evaluate(({ phase, id, at }) => {
            const canvas = document.querySelector('canvas');
            const type = { down: 'pointerdown', move: 'pointermove', up: 'pointerup', cancel: 'pointercancel' }[phase];
            const event = new PointerEvent(type, {
              pointerId: id, pointerType: 'touch', isPrimary: id === 1,
              clientX: at.x, clientY: at.y, button: phase === 'move' ? -1 : 0, buttons: phase === 'up' || phase === 'cancel' ? 0 : 1,
              bubbles: true, cancelable: true,
            });
            // Synthetic moves have no browser-generated coalesced list.
            if (phase === 'move') event.getCoalescedEvents = () => [event];
            canvas.dispatchEvent(event);
          }, { phase, id, at });
        }
        await frames(page);
      }
      await pointer('down', 1, primary);
      await wait(page, () => JSON.parse(document.querySelector('#ui-state').textContent).controls.touchHeld);
      await pointer('move', 1, { x: primary.x + 35, y: primary.y + 35 });
      s = await state(page);
      assert.ok(Math.abs(s.controls.steer - .425) < .001, `default 85% mobile steering: ${JSON.stringify(s.controls)}`);
      assert.ok(Math.abs(s.controls.lean - .5) < .001, 'independent physical lean');
      const originalCamera = s.camera.preset;
      await pointer('down', 2, camera);
      await pointer('up', 2, camera);
      s = await state(page);
      assert.notEqual(s.camera.preset, originalCamera, 'held-finger camera utility cycles once');
      assert.equal(s.toolbar.touchOwned, true, 'camera preserves riding finger');
      const changedCamera = s.camera.preset;
      await pointer('down', 2, camera);
      await pointer('move', 2, { x: camera.x, y: camera.y + 30 });
      await pointer('up', 2, camera);
      assert.equal((await state(page)).camera.preset, changedCamera, 'drag-away utility cancels');
      await pointer('down', 2, pause);
      await pointer('up', 2, pause);
      s = await state(page);
      assert.equal(s.paused, true);
      assert.equal(s.toolbar.touchOwned, false, 'pause clears held touch');
      await frames(page, 10);
      assert.equal((await state(page)).step, s.step, 'pause freezes simulation');
      await pointer('up', 1, primary);
      const resume = center((await state(page)).toolbar.hits.find(h => h.id === 1));
      await pointer('down', 2, resume);
      await pointer('up', 2, resume);
      await wait(page, () => !JSON.parse(document.querySelector('#ui-state').textContent).paused);
      assert.equal((await state(page)).controls.touchHeld, false, 'resume requires fresh press');
      await pointer('down', 1, primary);
      await page.keyboard.down('KeyR');
      await frames(page, 4);
      await page.keyboard.up('KeyR');
      await frames(page);
      assert.equal((await state(page)).toolbar.touchOwned, false, 'keyboard reset clears touch');
      await pointer('up', 1, primary);
      await pointer('down', 1, primary);
      await page.keyboard.down('Escape');
      await frames(page, 4);
      await page.keyboard.up('Escape');
      await frames(page);
      assert.equal((await state(page)).toolbar.touchOwned, false, 'keyboard pause clears touch');
      await pointer('up', 1, primary);
      await page.keyboard.down('Escape');
      await frames(page, 4);
      await page.keyboard.up('Escape');
      await frames(page);
      await pointer('down', 1, primary);
      await pointer('cancel', 1, primary);
      assert.equal((await state(page)).toolbar.touchOwned, false, 'pointer cancellation drops ownership');
      await pointer('down', 1, primary);
      await page.setViewportSize({ width: viewport.width + 1, height: viewport.height });
      await frames(page);
      assert.equal((await state(page)).toolbar.touchOwned, false, 'resize drops ownership');
      await pointer('up', 1, primary);
      const capture = path.resolve('private/reports/devlog-images', `rust-touch-${engine}-${viewport.width}.png`);
      await mkdir(path.dirname(capture), { recursive: true });
      await pointer('down', 1, primary);
      await pointer('move', 1, { x: primary.x + 35, y: primary.y + 35 });
      await page.screenshot({ path: capture });
      report.push({ engine, viewport, state: await state(page), capture });
      if (viewport.width === 390) {
        // A natural Caribou hop exercises the wheel at each browser's real
        // backing ratio, without bypassing the recovered stunt entry gates.
        await pointer('up', 1, primary);
        await page.keyboard.down('KeyR');
        await frames(page, 4);
        await page.keyboard.up('KeyR');
        const start = (await state(page)).step;
        await pointer('down', 1, primary);
        await page.waitForFunction(start => {
          const s = JSON.parse(document.querySelector('#ui-state').textContent);
          return s.step > start + 130 && s.grounded.every(g => !g);
        }, start, { timeout: 20000 });
        const wheel = { x: 195, y: 300 };
        await pointer('down', 2, wheel);
        await pointer('move', 2, { x: wheel.x, y: wheel.y - 65 });
        const preview = await state(page);
        assert.ok(preview.toolbar.touchOwned && preview.toolbar.secondaryOwned, 'airborne wheel preserves both fingers');
        const wheelCapture = path.resolve('private/reports/devlog-images', `rust-touch-wheel-${engine}.png`);
        await page.screenshot({ path: wheelCapture });
        report.push({ engine, viewport, state: preview, capture: wheelCapture });
        await pointer('up', 2, { x: wheel.x, y: wheel.y - 65 });
        assert.equal((await state(page)).toolbar.secondaryOwned, false, 'wheel release drops only secondary');
      }
      assertNoErrors(errors);
      await page.close();
    }
  } finally { await browser.close(); }
}
await writeJson('private/reports/engine-ui/touch.json', report);
console.log('PASS: Rust touch axes, held-finger utilities, drag guards, pause/resume, reset, cancel and resize; Chromium real touch + WebKit pointer fixture.');
