// Pause → Jukebox in the Rust engine (jukebox.js + music.js): the original
// soundtrack list, song choice, Pause/Play, Next/Previous through the play
// history, the volume range, the saved v3 soundtrack record, and the
// Jukebox pause ending when it closes. Host browser evidence.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state').textContent));
const menu = async page => (await read(page)).toolbar.menu;
const audio = page => page.evaluate(() => JSON.parse(document.documentElement.getAttribute('data-engine-audio') ?? 'null'));
const until = (page, predicate, arg) => page.waitForFunction(predicate, arg, { timeout: 30000 });
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }
async function click(page, action) {
  let m = await menu(page), hit = m.hits.find(h => h.action === action);
  assert.ok(hit, `${action} on ${m.page}`);
  if (hit.y < 0 || hit.y + hit.height > 720) { await page.mouse.move(900, 400); await page.mouse.wheel(0, hit.y < 0 ? -4000 : hit.y - 300); await nextFrames(page, 6); m = await menu(page); hit = m.hits.find(h => h.action === action); }
  await page.mouse.click(hit.x + hit.width / 2, hit.y + hit.height / 2); await nextFrames(page, 4);
}
const music = page => menu(page).then(m => m?.music);

const browser = await launch({ args: ['--autoplay-policy=no-user-gesture-required'] });
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await page.addInitScript(() => {
    if (sessionStorage.getItem('seeded')) return;
    sessionStorage.setItem('seeded', '1');
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.setItem('atv-soundtrack-v1', JSON.stringify({ v: 3, enabled: true, volume: 0.45, history: [], historyIndex: -1 }));
  });
  await page.goto(url('/engine.html?venue=env01&uiDiagnostics=1&audioDiagnostics=1'));
  await page.waitForSelector('#ui-state', { state: 'attached', timeout: 120000 });
  await nextFrames(page, 20);
  await page.mouse.click(640, 430);
  await until(page, () => JSON.parse(document.documentElement.getAttribute('data-engine-audio') ?? 'null')?.musicTitle);
  await key(page, 'Escape');
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).toolbar?.menu?.page === 'pause');
  await click(page, 'jukebox');
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).toolbar?.menu?.page === 'jukebox');
  let m = await menu(page);
  assert.equal(m.choices.filter(c => c.startsWith('song:')).length, 21);
  assert.deepEqual(m.choices.slice(0, 5), ['jukeplay', 'jukeprev', 'jukenext', 'jukeseek', 'jukevolume']);
  assert.ok(m.choices.includes('back'));
  // A chosen song plays and is recorded in the saved history.
  await click(page, 'song:2');
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).toolbar.menu.music?.playing === 2);
  await until(page, () => JSON.parse(document.documentElement.getAttribute('data-engine-audio')).musicTime > 0.5);
  // drawBars: the analyser lights the spectrum while the song plays.
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).toolbar.menu.music?.spectrum > 0);
  assert.equal((await music(page)).bars, true);
  if (process.env.ATV_SHOT) await page.screenshot({ path: process.env.ATV_SHOT });
  let saved = await page.evaluate(() => JSON.parse(localStorage.getItem('atv-soundtrack-v1')));
  assert.equal(saved.v, 3); assert.equal(saved.history.at(-1).toLowerCase().includes('.vag'), true);
  // Pause and Play (a transient pause, not Music off).
  await click(page, 'jukeplay');
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).toolbar.menu.music?.paused === true);
  assert.equal((await audio(page)).musicPaused, true);
  await click(page, 'jukeplay');
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).toolbar.menu.music?.paused === false);
  // Next picks a new song; Previous returns through the history.
  await click(page, 'jukenext');
  await until(page, () => { const p = JSON.parse(document.querySelector('#ui-state').textContent).toolbar.menu.music?.playing; return p !== null && p !== 2; });
  await click(page, 'jukeprev');
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).toolbar.menu.music?.playing === 2);
  // Right on the volume range: 45% → 50%, saved in the soundtrack record.
  m = await menu(page);
  const volumeAt = m.choices.indexOf('jukevolume');
  while ((await menu(page)).selection !== volumeAt) await key(page, 'ArrowDown');
  await key(page, 'ArrowRight');
  saved = await page.evaluate(() => JSON.parse(localStorage.getItem('atv-soundtrack-v1')));
  assert.ok(Math.abs(saved.volume - 0.5) < 1e-6, JSON.stringify(saved));
  await until(page, () => Math.abs(JSON.parse(document.querySelector('#ui-state').textContent).toolbar.menu.music.volume - 0.5) < 1e-6);
  // Leaving the Jukebox ends its pause.
  await click(page, 'jukeplay');
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).toolbar.menu.music?.paused === true);
  await key(page, 'Escape');
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).toolbar.menu?.page === 'pause');
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).toolbar.menu.music?.paused === false);
  assertNoErrors(errors);
  console.log('PASS: Rust Jukebox song choice, pause/play, next/previous history, volume and saved record');
} finally {
  await browser.close();
}
