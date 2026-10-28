// Main menu → Options → Jukebox in the Rust engine (the original Options page
// leads with it; frontend.js jukeboxReturn,
// audio.js setMusicPreview): the soundtrack list over the frontend, a chosen
// song playing in place of the menu tune with the spectrum and PAGE14 art,
// Back returning to the menu tune with the song paused in place, and the next
// ride starting a new song (game.js loadGame builds a new MusicStream). Host
// browser evidence.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const until = (page, predicate, arg, timeout = 60000) => page.waitForFunction(predicate, arg, { timeout });
const state = () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}');
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }
async function click(page, action) {
  const s = await read(page), index = s.front.menu.choices.indexOf(action);
  const hit = s.front.menuHits.find(h => h.id === 1000 + index);
  assert.ok(index >= 0 && hit, `${action} on ${s.front.menu.page}`);
  await page.mouse.click(hit.x + hit.width / 2, hit.y + hit.height / 2); await nextFrames(page, 4);
}

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
  await page.goto(url('/play.html?uiDiagnostics=1&audioDiagnostics=1'));
  await until(page, () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === 'title', null, 120000);
  await page.mouse.click(8, 8);
  await key(page, 'Enter');
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).front.page === 'main');
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).menuMusic?.playing === true);
  let s = await read(page);
  // Open the Jukebox from Options, as the original's Options page (PAGE13) leads with it.
  assert.ok(!s.front.choices.includes('jukebox'), s.front.choices.join());
  for (let i = 0; i < 10 && (await read(page)).front.choices[(await read(page)).front.selection] !== 'settings'; i++) await key(page, 'ArrowDown');
  await key(page, 'Enter');
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).front.menu?.page === 'settings');
  s = await read(page);
  assert.equal(s.front.menu.choices[0], 'jukebox', s.front.menu.choices.join());
  await key(page, 'Enter');
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).front.menu?.page === 'jukebox');
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).front.menu.music?.songs === 21);
  s = await read(page);
  assert.equal(s.front.menu.choices.filter(c => c.startsWith('song:')).length, 21);
  // A chosen song replaces the menu tune; the bars and the disc art show.
  await click(page, 'song:2');
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).soundtrack?.playing === 2);
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).menuMusic.playing === false);
  await until(page, () => (JSON.parse(document.querySelector('#ui-state').textContent).soundtrack?.musicTime ?? 0) > 0.5);
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).front.menu?.spectrum > 0);
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).front.menu?.art === true);
  await nextFrames(page, 10);
  if (process.env.ATV_SHOT) await page.screenshot({ path: process.env.ATV_SHOT });
  const title = (await read(page)).soundtrack.musicTitle;
  const saved = await page.evaluate(() => JSON.parse(localStorage.getItem('atv-soundtrack-v1')));
  assert.ok(saved.history.length >= 1, 'the play history is saved');
  // Back to Options on its Jukebox row: the menu tune returns and the song pauses in place.
  await key(page, 'Escape');
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).front.menu?.page === 'settings');
  s = await read(page);
  assert.equal(s.front.menu.choices[s.front.menu.selection], 'jukebox');
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).menuMusic.playing === true);
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).soundtrack.musicPaused === true);
  console.log(`  Jukebox: ${title}; back on the menu tune with the song paused`);
  // Back again closes Options on its main-menu row.
  await key(page, 'Escape');
  await until(page, () => !JSON.parse(document.querySelector('#ui-state').textContent).front.menu);
  s = await read(page);
  assert.equal(s.front.choices[s.front.selection], 'settings');
  // A ride starts a new song (game.js loadGame builds a new MusicStream).
  for (let i = 0; i < 10 && (await read(page)).front.choices[(await read(page)).front.selection] !== 'single'; i++) await key(page, 'ArrowUp');
  await key(page, 'Enter');
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).front.page === 'single');
  for (let i = 0; i < 10 && (await read(page)).front.choices[(await read(page)).front.selection] !== 'tracks'; i++) await key(page, 'ArrowDown');
  await key(page, 'Enter');
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).front.page === 'tracks');
  await key(page, 'Enter');
  await until(page, () => (document.querySelector('#status')?.textContent ?? '').startsWith('ENV'), null, 180000);
  await page.mouse.click(640, 400);
  await until(page, t => {
    const audio = JSON.parse(document.documentElement.getAttribute('data-engine-audio') ?? 'null');
    return audio?.musicTitle && audio.musicTitle !== t && audio.musicPaused === false && audio.musicTime > 0;
  }, title, 120000);
  console.log(`  Ride: a new song after the Jukebox's ${title}`);
  assertNoErrors(errors);
  console.log('PASS: Rust main-menu Jukebox plays the soundtrack; the next ride picks a new song');
} finally {
  await browser.close();
}
