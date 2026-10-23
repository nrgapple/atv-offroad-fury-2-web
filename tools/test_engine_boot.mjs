// engine.html's startup, after play.html (tools/test_ui_polish.mjs): the
// "Now Loading..." boot panel shows while the WebAssembly and content load and
// fits phone portrait/landscape; it closes once the Rust frontend draws. A
// failing content request closes it too and shows the error on #menu-notice
// with #frontend[data-error=true] (frontend.js `fail`).
// Host browser evidence. Content group.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, optionalScreenshot, url } from './browser/harness.mjs';

const sizes = [
  { width: 390, height: 844 },
  { width: 844, height: 390 },
  { width: 667, height: 375 }
];
const browser = await launch();
try {
  {
    const errors = [];
    const page = await newPage(browser, { viewport: sizes[0] }, errors);
    // Hold only the engine's WebAssembly so the boot screen is observable.
    let release;
    const held = new Promise(resolve => (release = resolve));
    await page.route('**/atv_app_bg.wasm*', async route => {
      await held;
      await route.continue();
    });
    await page.goto(url('/engine.html'));
    for (const size of sizes) {
      await page.setViewportSize(size);
      // install.js resizes the page's --app-width/--app-height on resize.
      await page.waitForFunction(
        ({ width, height }) => {
          const box = document.querySelector('.loading-panel').getBoundingClientRect();
          return Math.abs(box.width - width) < 2 && Math.abs(box.height - height) < 2;
        },
        size,
        { timeout: 5000 }
      );
      const box = await page.locator('.loading-panel').boundingBox();
      assert(
        box && box.x >= -1 && box.y >= -1 && box.x + box.width <= size.width + 1 && box.y + box.height <= size.height + 1,
        `.loading-panel fits ${JSON.stringify(size)}: ${JSON.stringify(box)}`
      );
      for (const selector of ['.loading-brand', '#loading-title', '.loading-meter']) {
        const part = await page.locator(selector).boundingBox();
        assert(part && part.width > 0 && part.y + part.height <= size.height + 1, `${selector} at ${JSON.stringify(size)}`);
      }
      assert(await page.locator('#loading-screen').isVisible());
      assert.equal(await page.locator('#loading-title').textContent(), 'Now Loading...');
      assert(!(await page.locator('#menu-notice').isVisible()));
      await optionalScreenshot(page, 'ATV_BOOT_SHOTS', `boot-${size.width}x${size.height}`);
    }
    release();
    await page.waitForFunction(() => document.querySelector('#frontend').dataset.page === 'title', null, {
      timeout: 120000
    });
    await page.waitForFunction(() => document.querySelector('#loading-screen').hidden, null, { timeout: 10000 });
    assert.equal(await page.locator('#loading-screen').getAttribute('aria-busy'), 'false');
    assert(!(await page.locator('#menu-notice').isVisible()));
    assert.equal(await page.locator('#frontend').getAttribute('data-error'), null);
    assertNoErrors(errors);
    await page.close();
  }
  // A content package that cannot be read ends startup with its error.
  for (const [name, handle] of [
    ['abort', route => route.abort()],
    ['404', route => route.fulfill({ status: 404, body: 'missing' })]
  ]) {
    const page = await browser.newPage({ viewport: sizes[0] });
    await page.route('**/content/manifest.json', handle);
    await page.goto(url('/engine.html'));
    await page.waitForFunction(() => document.querySelector('#frontend').dataset.error === 'true', null, {
      timeout: 120000
    });
    assert.equal(await page.locator('#loading-screen').isVisible(), false, name);
    assert(await page.locator('#menu-notice').isVisible(), name);
    const notice = (await page.locator('#menu-notice').textContent()).trim();
    assert(notice.length > 0, name);
    const box = await page.locator('#menu-notice').boundingBox();
    assert(box.x >= 0 && box.x + box.width <= sizes[0].width && box.y + box.height <= sizes[0].height, name);
    console.log(`${name}: ${notice}`);
    await optionalScreenshot(page, 'ATV_BOOT_SHOTS', `boot-failure-${name}`);
    await page.close();
  }
  console.log('PASS: engine boot loader at three sizes, closes on the title, and startup failures show the notice');
} finally {
  await browser.close();
}
