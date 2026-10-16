// Authored input/presentation fixture. Chromium uses real two-touch CDP events;
// WebKit uses synthetic Pointer Events with a capture shim, not physical iOS.
import assert from 'node:assert/strict';
import { mkdirSync, writeFileSync } from 'node:fs';
import { assertNoErrors, base, launch, newPage } from './browser/harness.mjs';
const output = process.env.ATV_TRICK_WHEEL_OUTPUT || 'work/verification/trick-wheel';
mkdirSync(output, { recursive: true });
const results = [];
for (const [name, engine] of [
  ['Chromium', 'chromium'],
  ['WebKit', 'webkit']
]) {
  const browser = await launch({ engine });
  try {
    for (const viewport of [
      { width: 390, height: 844 },
      { width: 844, height: 390 },
      { width: 320, height: 568 },
      { width: 667, height: 375 }
    ]) {
      const errors = [];
      const page = await newPage(browser, { viewport, hasTouch: true, isMobile: true }, errors);
      await page.route('**/wheel-fixture', route =>
        route.fulfill({
          contentType: 'text/html',
          body: '<!doctype html><meta name="viewport" content="width=device-width,initial-scale=1"><link rel="stylesheet" href="/design-system.css"><link rel="stylesheet" href="/touch.css"><style>html,body{margin:0;overflow:hidden}#game{position:fixed;inset:0;width:100%;height:100%;touch-action:none}</style><canvas id="game"></canvas><div id="toolbar"></div>'
        })
      );
      await page.goto(new URL('/wheel-fixture', base).href);
      await page.evaluate(async synthetic => {
        const { mountTouchControls, TRICK_WHEEL } = await import('/touch.js');
        localStorage.setItem('atv-mobile-one-handed', 'true');
        window.wheelTricks = TRICK_WHEEL;
        const surface = document.querySelector('#game');
        if (synthetic) surface.setPointerCapture = () => {};
        window.controls = mountTouchControls({ active: () => true, surface, flips: () => window.flipsOn === true });
        controls.feedback({ grounded: [false, false, false, false] });
      }, name === 'WebKit');
      const cdp = name === 'Chromium' ? await page.context().newCDPSession(page) : null;
      let contacts = [];
      const touch = async (type, points = []) => {
        if (cdp)
          await cdp.send('Input.dispatchTouchEvent', {
            type,
            touchPoints: points.map(p => ({ radiusX: 6, radiusY: 6, force: 1, ...p }))
          });
        else {
          const eventType = {
            touchStart: 'pointerdown',
            touchMove: 'pointermove',
            touchEnd: 'pointerup',
            touchCancel: 'pointercancel'
          }[type];
          const changed =
            type === 'touchStart'
              ? points.filter(p => !contacts.some(q => q.id === p.id))
              : type === 'touchEnd'
                ? points
                : type === 'touchCancel'
                  ? contacts
                  : points;
          await page.evaluate(
            ({ eventType, changed }) => {
              for (const p of changed)
                document
                  .querySelector('#game')
                  .dispatchEvent(
                    new PointerEvent(eventType, {
                      pointerType: 'touch',
                      pointerId: p.id,
                      clientX: p.x,
                      clientY: p.y,
                      bubbles: true,
                      cancelable: true
                    })
                  );
            },
            { eventType, changed }
          );
        }
        contacts =
          type === 'touchEnd'
            ? contacts.filter(p => !points.some(q => q.id === p.id))
            : type === 'touchCancel'
              ? []
              : points;
      };
      const sample = () => page.evaluate(() => controls.sample());
      const wheel = page.locator('.touch-trick-wheel');
      const primary = { id: 1, x: viewport.width * 0.72, y: viewport.height * 0.75 };
      const secondary = { id: 2, x: viewport.width * 0.28, y: viewport.height * 0.55 };
      await touch('touchStart', [primary]);
      await sample();
      const moved = { ...primary, x: primary.x - 20, y: primary.y - 15 };
      await touch('touchMove', [moved]);
      const baseline = await sample();
      const scale = await page.evaluate(() => visualViewport.scale);
      const tricks = await page.evaluate(() => wheelTricks);
      for (const [index, trick] of tricks.entries()) {
        await touch('touchStart', [moved, secondary]);
        assert(await wheel.isVisible());
        const box = await wheel.boundingBox();
        assert(
          box.x >= 0 && box.y >= 0 && box.x + box.width <= viewport.width && box.y + box.height + 14 <= viewport.height,
          'wheel and cue fit'
        );
        const target = await page.locator('.touch-trick-choice').nth(index).boundingBox();
        const selected = { ...secondary, x: target.x + target.width / 2, y: target.y + target.height / 2 };
        await touch('touchMove', [moved, selected]);
        assert.equal(await page.locator('.touch-trick-choice.selected').textContent(), trick.label);
        assert.equal(await page.locator('.touch-trick-name').textContent(), trick.name, 'full name shown above the wheel');
        assert(
          await page
            .locator('.touch-trick-choice, .touch-trick-cue, .touch-trick-center, .touch-rider-marker span')
            .evaluateAll(elements =>
              elements.every(element => {
                const style = getComputedStyle(element);
                return (
                  style.backgroundColor === 'rgba(0, 0, 0, 0)' &&
                  style.borderTopWidth === '0px' &&
                  style.borderLeftWidth === '0px' &&
                  style.boxShadow === 'none' &&
                  style.color === 'rgb(255, 255, 255)' &&
                  style.webkitTextStrokeColor === 'rgb(0, 0, 0)'
                );
              })
            ),
          'labels are white with black outlines and no boxes or backdrops'
        );
        assert.equal(
          await wheel.evaluate(element => getComputedStyle(element).pointerEvents),
          'none',
          'wheel never steals touch from game'
        );
        assert.equal((await sample()).stunt_modifiers, 0, 'preview does not invoke trick');
        if (index === 0 && viewport.width === 390) await page.screenshot({ path: `${output}/fixture-${name}.png` });
        await touch('touchEnd', [selected]);
        const command = await sample();
        assert(!(await wheel.isVisible()));
        assert.equal(command.stunt_modifiers, trick.modifiers);
        assert.equal(command.stunt_x, trick.axes[0]);
        assert.equal(command.stunt_y, trick.axes[1]);
        assert.equal(command.steer, baseline.steer);
        assert.equal(command.lean, baseline.lean);
        assert.equal(command.preload_axis, 1);
        assert.deepEqual(await sample(), baseline, 'single release pulse then original primary commands');
      }
      // Edge spawning keeps all named targets aligned with actual selection.
      for (const edge of [
        { x: 4, y: 4 },
        { x: viewport.width - 4, y: viewport.height - 4 }
      ]) {
        const second = { ...secondary, ...edge };
        await touch('touchStart', [moved, second]);
        assert.equal((await sample()).stunt_modifiers, 0);
        await touch('touchEnd', [second]);
        assert.deepEqual(await sample(), baseline, 'edge tap is neutral');
        for (const index of [0, 2, 5, 8]) {
          await touch('touchStart', [moved, second]);
          const target = await page.locator('.touch-trick-choice').nth(index).boundingBox();
          const chosen = { ...second, x: target.x + target.width / 2, y: target.y + target.height / 2 };
          await touch('touchMove', [moved, chosen]);
          assert.equal(await page.locator('.touch-trick-choice.selected').textContent(), tricks[index].label);
          const center = await wheel.boundingBox();
          const neutral = { ...second, x: center.x + center.width / 2, y: center.y + center.height / 2 };
          await touch('touchMove', [moved, neutral]);
          assert.equal(await page.locator('.touch-trick-choice.selected').count(), 0);
          await touch('touchEnd', [neutral]);
          assert.deepEqual(await sample(), baseline, 'center release cancels');
        }
      }
      // Optional Extras flips: gold labels outside the ring, each reachable
      // from center and edge presses, submitted once on release.
      await page.evaluate(() => { window.flipsOn = true; controls.refresh(); });
      const flips = await page.evaluate(async () => (await import('/touch.js')).FLIP_WHEEL);
      for (const spawn of [secondary, { ...secondary, x: 4, y: 4 }, { ...secondary, x: viewport.width - 4, y: viewport.height - 4 }]) {
        for (const [index, flip] of flips.entries()) {
          await touch('touchStart', [moved, spawn]);
          for (const element of await page.locator('.touch-flip-choice, .touch-trick-cue, .touch-trick-choice').all()) {
            const b = await element.boundingBox();
            assert(b && b.x >= 0 && b.y >= 0 && b.x + b.width <= viewport.width && b.y + b.height <= viewport.height,
              `flip wheel labels fit ${viewport.width}x${viewport.height}: ${await element.textContent()} ${JSON.stringify(b)}`);
          }
          const target = await page.locator('.touch-flip-choice').nth(index).boundingBox();
          const chosen = { ...spawn, x: target.x + target.width / 2, y: target.y + target.height / 2 };
          await touch('touchMove', [moved, chosen]);
          assert.equal(await page.locator('.touch-trick-choice.selected').textContent(), flip.label);
          assert.equal((await sample()).flip_request, null, 'flip preview does not start a flip');
          if (index === 0 && spawn === secondary && [390, 844].includes(viewport.width))
            await page.screenshot({ path: `${output}/flips-${name}-${viewport.width}x${viewport.height}.png` });
          await touch('touchEnd', [chosen]);
          const command = await sample();
          assert.equal(command.flip_request, flip.flip);
          assert.equal(command.stunt_modifiers, 0);
          assert.equal(command.lean, baseline.lean);
          assert.deepEqual(await sample(), baseline, 'flip is a single release pulse');
        }
      }
      await page.evaluate(() => { window.flipsOn = false; controls.refresh(); });
      assert.equal(await page.locator('.touch-flip-choice').first().isVisible(), false, 'Off hides flip labels');
      assert.equal(await page.evaluate(() => visualViewport.scale), scale, 'two-touch drags never zoom');
      assert.deepEqual(await page.evaluate(() => [scrollX, scrollY]), [0, 0]);
      const suppressed = await page.evaluate(() =>
        ['touchstart', 'touchmove', 'gesturestart', 'gesturechange', 'gestureend', 'contextmenu', 'selectstart'].map(
          type => {
            const event = new Event(type, { bubbles: true, cancelable: true });
            document.querySelector('#game').dispatchEvent(event);
            return event.defaultPrevented;
          }
        )
      );
      assert(suppressed.every(Boolean), 'game gestures/selection/callout suppressed');
      // Lifecycle cancellation must hide the menu and discard pending input.
      for (const type of ['blur', 'resize', 'visibilitychange']) {
        await touch('touchStart', [moved, secondary]);
        const target = await page.locator('.touch-trick-choice').first().boundingBox();
        await touch('touchMove', [moved, { ...secondary, x: target.x + 32, y: target.y + 14 }]);
        await page.evaluate(
          type => (type === 'visibilitychange' ? document : window).dispatchEvent(new Event(type)),
          type
        );
        assert(!(await wheel.isVisible()));
        const empty = await sample();
        assert(!empty.drive.held);
        assert.equal(empty.stunt_modifiers, 0);
        assert.equal(empty.preload_axis, null);
        await touch('touchCancel');
        await touch('touchStart', [moved]);
      }
      await touch('touchCancel');
      assert(!(await wheel.isVisible()));
      assertNoErrors(errors);
      results.push({
        browser: name,
        viewport,
        tenReleaseSelections: true,
        edgeTargetsAndCenterCancel: true,
        plainOutlinedText: true,
        noZoomScrollSelection: true,
        lifecycleCancellation: true,
        optionalFlips: true,
        errors
      });
      await page.close();
    }
  } finally {
    await browser.close();
  }
}
writeFileSync(`${output}/report.json`, JSON.stringify(results, null, 2));
console.log(JSON.stringify({ gate: 'trick-wheel', results }));
