// Each ride starts a new random song, as game.js `loadGame` builds a new
// GameAudio whose MusicStream reloads the catalog with a fresh sessionSeed()
// and whose first pick avoids the last song heard (`beginSong`). Rides from
// the Rust frontend: ride, Quit to Main Menu, ride again (a different song);
// then a song chosen in the main-menu Jukebox does not carry into the next
// ride either. Host browser evidence.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const audio = page => page.evaluate(() => JSON.parse(document.documentElement.getAttribute('data-engine-audio') ?? 'null'));
const until = (page, predicate, arg, timeout = 60000) => page.waitForFunction(predicate, arg, { timeout });
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }
const frontPage = (page, name) => until(page, name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name, name, 120000);
async function select(page, action) {
  for (let i = 0; i < 12; i++) {
    const f = (await read(page)).front;
    if (f.choices[f.selection] === action) return;
    await key(page, 'ArrowDown');
  }
  throw new Error(`${action} not reached`);
}
// Main menu → Single Player → Select Track → Ride (the saved course), then the song.
async function ride(page, previous) {
  await select(page, 'single'); await key(page, 'Enter'); await frontPage(page, 'single');
  await select(page, 'tracks'); await key(page, 'Enter'); await frontPage(page, 'tracks');
  await key(page, 'Enter');
  await until(page, () => (document.querySelector('#status')?.textContent ?? '').startsWith('ENV'), null, 180000);
  await page.mouse.click(640, 400);
  await until(page, previous => {
    const a = JSON.parse(document.documentElement.getAttribute('data-engine-audio') ?? 'null');
    return a?.musicTitle && a.musicTitle !== previous && a.musicTime > 0.3 && !a.musicPaused;
  }, previous, 120000).catch(async error => { console.error(await audio(page)); throw error; });
  return (await audio(page)).musicTitle;
}
async function quit(page) {
  await key(page, 'Escape');
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).toolbar?.menu?.page === 'pause');
  const hit = (await read(page)).toolbar.menu.hits.find(h => h.action === 'main');
  assert.ok(hit, 'Quit to Main Menu');
  await page.mouse.click(hit.x + hit.width / 2, hit.y + hit.height / 2);
  await frontPage(page, 'main');
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
  await frontPage(page, 'title');
  await page.mouse.click(8, 8);
  await key(page, 'Enter'); await frontPage(page, 'main');
  const first = await ride(page, null);
  await quit(page);
  const second = await ride(page, first);
  assert.notEqual(second, first, 'the second ride starts a new song');
  await quit(page);
  // A Jukebox choice (Main Menu → Options → Jukebox) plays over the menus,
  // then the ride picks anew.
  await select(page, 'settings'); await key(page, 'Enter');
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).front.menu?.page === 'settings');
  await key(page, 'Enter');
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).front.menu?.music?.songs > 0);
  const s = await read(page), index = s.front.menu.choices.indexOf('song:4');
  const hit = s.front.menuHits.find(h => h.id === 1000 + index);
  await page.mouse.click(hit.x + hit.width / 2, hit.y + hit.height / 2);
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).soundtrack?.playing === 4);
  const chosen = (await read(page)).soundtrack.musicTitle;
  await key(page, 'Escape');
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).front.menu?.page === 'settings');
  await key(page, 'Escape');
  await until(page, () => !JSON.parse(document.querySelector('#ui-state').textContent).front.menu);
  const third = await ride(page, chosen);
  assert.notEqual(third, chosen, 'the Jukebox song does not carry into the ride');
  const saved = await page.evaluate(() => JSON.parse(localStorage.getItem('atv-soundtrack-v1')));
  assert.deepEqual(saved.history.slice(-4).length, 4, JSON.stringify(saved.history));
  assertNoErrors(errors);
  console.log(`  Rides: ${first} → ${second}; Jukebox ${chosen} → ride ${third}`);
  console.log('PASS: each Rust ride starts a new song');
} finally {
  await browser.close();
}
