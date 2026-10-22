// The original frontend tune in the Rust engine (audio.js loadMenuMusic /
// playMenuMusic; the routine at 00255b50's UIMusic list): after the first gesture on
// the title, the first UIMUSIC.AUD tune (uitune1, left/right VAGs from
// AUDIO.RES) loops on the music bus while the menus are shown and focused;
// Music off stops it; a ride takes over. Records the mixer's voices through
// the engine's audio diagnostics. Host browser evidence.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const wait = (page, predicate, arg, timeout = 60000) => page.waitForFunction(predicate, arg, { timeout });
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }

const browser = await launch({ args: ['--autoplay-policy=no-user-gesture-required'] });
try {
  for (const enabled of [true, false]) {
    const errors = [];
    const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
    await page.addInitScript(enabled => {
      localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
      localStorage.setItem('atv-soundtrack-v1', JSON.stringify({ v: 3, enabled, volume: 0.45, history: [], historyIndex: -1 }));
    }, enabled);
    await page.goto(url('/engine.html?uiDiagnostics=1'));
    await wait(page, () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === 'title', null, 120000);
    assert.equal((await read(page)).menuMusic?.playing, false, 'silent before a gesture');
    await page.mouse.click(8, 8);
    await key(page, 'Enter');
    if (enabled) {
      await wait(page, () => JSON.parse(document.querySelector('#ui-state').textContent).menuMusic?.playing === true);
      assert.equal((await read(page)).menuMusic.loaded, true);
      // Audible: the output carries the tune.
      await wait(page, () => JSON.parse(document.querySelector('#ui-state').textContent).menuMusic?.peak > 0.01);
    } else {
      await page.waitForTimeout(3000);
      const silent = await read(page);
      assert.equal(silent.menuMusic?.playing, false);
      assert.ok(!(silent.menuMusic?.peak > 0), JSON.stringify(silent.menuMusic));
    }
    assertNoErrors(errors);
    console.log(`  Music ${enabled ? 'on' : 'off'}: ${JSON.stringify((await read(page)).menuMusic)}`);
    await page.close();
  }
  console.log('PASS: Rust frontend plays the original menu tune');
} finally {
  await browser.close();
}
