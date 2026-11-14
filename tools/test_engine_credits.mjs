// Options → Credits (research/MENU_FLOW.md "Credits"): from the main menu's
// Options the Credits row opens the original page 18. Its scroll script is
// PAGE18.RES `credits.txt` from the content package (never the repository),
// followed by the port's "Fury Remastered" section. The page scrolls on its
// own and Back returns to Options on the Credits row.
// ATV_CREDITS_SHOTS=<dir> saves captures (an ignored directory).
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const shots = process.env.ATV_CREDITS_SHOTS || '';
const state = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }
async function until(page, predicate, what, timeout = 60000) {
  const deadline = Date.now() + timeout;
  for (;;) {
    const s = await state(page);
    if (predicate(s)) return s;
    if (Date.now() > deadline) throw new Error(`timed out: ${what} ${JSON.stringify(s.front?.menu ?? s.front?.page)}`);
    await page.waitForTimeout(50);
  }
}
async function choose(page, choices, selection, action) {
  const at = choices.indexOf(action);
  assert.ok(at >= 0, `${action} in ${choices}`);
  for (let i = selection; i < at; i++) await key(page, 'ArrowDown');
  for (let i = selection; i > at; i--) await key(page, 'ArrowUp');
  await key(page, 'Enter');
}

if (shots) await mkdir(shots, { recursive: true });
const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await page.addInitScript(() => localStorage.setItem('atv2-keyboard-help-seen-v1', '1'));
  await page.goto(url('/play.html?uiDiagnostics=1'));
  await until(page, s => s.front?.page === 'title', 'title');
  await page.mouse.click(8, 8);
  await key(page, 'Enter');
  let s = await until(page, s => s.front?.page === 'main' && !s.front?.menu, 'main');
  await choose(page, s.front.choices, s.front.selection, 'settings');
  s = await until(page, s => s.front?.menu?.page === 'settings', 'Options');
  const options = s.front.menu.choices;
  assert.deepEqual(options.slice(0, 4), ['jukebox', 'game-settings', 'controls', 'credits']);
  await choose(page, options, s.front.menu.selection ?? 0, 'credits');
  s = await until(page, s => s.front?.menu?.page === 'credits' && s.front.menu.credits, 'Credits script');
  const credits = s.front.menu.credits;
  assert.deepEqual(s.front.menu.choices, ['back']);
  assert.equal(credits.original, true, 'the original credits.txt was read');
  assert.equal(credits.first, 'Game Designer');
  assert.deepEqual(credits.bounds, [359, 0, 640, 448], 'disc text rectangle');
  assert.equal(credits.nameIndent, 10, 'disc name indentation');
  assert.ok(credits.entries > 500, `entries ${credits.entries}`);
  // The scroll moves on its own: the frames differ a second apart.
  await page.waitForTimeout(1500);
  const a = await page.screenshot();
  if (shots) await page.screenshot({ path: `${shots}/credits-1.png` });
  await page.waitForTimeout(1000);
  const b = await page.screenshot();
  assert.ok(!a.equals(b), 'the credits scroll');
  if (shots) {
    await page.waitForTimeout(8000);
    await page.screenshot({ path: `${shots}/credits-2.png` });
  }
  await page.setViewportSize({ width: 390, height: 844 });
  await nextFrames(page, 5);
  assert.equal((await state(page)).front.menu.page, 'credits', 'portrait keeps Credits open');
  if (shots) await page.screenshot({ path: `${shots}/credits-portrait.png` });
  await key(page, 'Escape');
  s = await until(page, s => s.front?.menu?.page === 'settings', 'back to Options');
  assert.equal(s.front.menu.choices[s.front.menu.selection], 'credits', 'Back returns to the Credits row');
  assertNoErrors(errors);
  console.log(`credits: ${credits.entries} entries; scrolls; Back returns to Options`);
} finally {
  await browser.close();
}
