// Single Player shows the saved profile (research/PROFILE.md): the Profile
// Points and race record that races credit to `fury-profile`, and a new
// profile when the record is missing or unreadable.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, optionalScreenshot, url } from './browser/harness.mjs';

const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const onPage = (page, name) => page.waitForFunction(name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name, name, { timeout: 60000 });
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }

const browser = await launch();
try {
  for (const [saved, expected] of [
    [JSON.stringify({ points: 137, races: 6, wins: 1, best_stunt: 4200 }), 'Profile Points 137 · 6 races · 1 win'],
    ['not json', 'Profile Points 0 · 0 races · 0 wins'],
  ]) {
    const errors = [];
    const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
    await page.addInitScript(saved => {
      localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
      localStorage.setItem('fury-profile', saved);
    }, saved);
    await page.goto(url('/play.html?uiDiagnostics=1'));
    await onPage(page, 'title');
    await page.mouse.click(8, 8);
    await key(page, 'Enter'); await onPage(page, 'main');
    await key(page, 'Enter'); await onPage(page, 'single');
    const front = (await read(page)).front;
    assert.equal(front.profile, expected);
    if (saved.startsWith('{')) await optionalScreenshot(page, 'ATV_PROFILE_MENU_SCREENSHOT', 'single');
    assertNoErrors(errors);
    await page.close();
    console.log(`profile ${saved.slice(0, 20)} -> ${front.profile}`);
  }
  console.log('PASS: Rust engine Single Player profile readout');
} finally {
  await browser.close();
}
