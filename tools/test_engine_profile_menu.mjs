// The menus show the loaded profile (research/PROFILE.md) on the status bar
// (front/status_bar.rs): its name and Profile Points on the chips, and the
// race record in the profile chip's popover, as races credit them to
// `fury-profile` (an older single save migrates to the loaded PLAYER1); a new
// profile when the record is missing or unreadable.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, optionalScreenshot, url } from './browser/harness.mjs';

const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const onPage = (page, name) => page.waitForFunction(name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name, name, { timeout: 60000 });
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }

const browser = await launch();
try {
  for (const [saved, expected] of [
    [JSON.stringify({ points: 137, races: 6, wins: 1, best_stunt: 4200 }), { points: 137, races: '6', wins: '1' }],
    ['not json', { points: 0, races: '0', wins: '0' }],
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
    let front = (await read(page)).front;
    // No points line under the rows any more: the bar carries them.
    assert.equal(front.profile, null);
    assert.equal(front.status.profile, 'PLAYER1');
    assert.equal(front.status.points, expected.points);
    // The profile chip's popover has the race record.
    const chip = front.hits.find(h => h.id === 2460);
    await page.mouse.click(chip.x + chip.width / 2, chip.y + chip.height / 2);
    await nextFrames(page, 3);
    front = (await read(page)).front;
    assert.equal(front.status.open, 'PLAYER1');
    const lines = Object.fromEntries(front.status.lines);
    assert.equal(lines.Races, expected.races);
    assert.equal(lines.Wins, expected.wins);
    await key(page, 'Escape');
    assert.equal((await read(page)).front.status.open, null);
    if (saved.startsWith('{')) await optionalScreenshot(page, 'ATV_PROFILE_MENU_SCREENSHOT', 'single');
    assertNoErrors(errors);
    await page.close();
    console.log(`profile ${saved.slice(0, 20)} -> ${JSON.stringify(lines)}`);
  }
  console.log('PASS: Rust engine profile readout on the status bar');
} finally {
  await browser.close();
}
