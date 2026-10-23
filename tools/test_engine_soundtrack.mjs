// Soundtrack edge cases in the Rust engine (web/music.test.mjs and
// web/audio.test.mjs on the JavaScript player):
// - Music Off pauses the song where it is, a pointer or key gesture never
//   restarts music the player turned off, and Music On resumes the same song
//   at the same position;
// - a song that cannot be read stops music reads (Music stays on) until the
//   next selection, which reads again;
// - a race holds the soundtrack through the intro and countdown with the
//   IntOut stinger (also when sound starts after the intro was skipped),
//   Pause keeps the hold, and Quit to Main Menu ends it.
// Host browser evidence. Content group.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const menu = async page => (await read(page)).toolbar?.menu;
const audio = page => page.evaluate(() => JSON.parse(document.documentElement.getAttribute('data-engine-audio') ?? 'null'));
const until = (page, predicate, arg, timeout = 30000, polling = 'raf') => page.waitForFunction(predicate, arg, { timeout, polling });
const onMenu = (page, name) => until(page, name => JSON.parse(document.querySelector('#ui-state').textContent).toolbar?.menu?.page === name, name);
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }
async function click(page, action) {
  let m = await menu(page), hit = m?.hits.find(h => h.action === action);
  assert.ok(hit, `${action} on ${m?.page}: ${m?.hits.map(h => h.action)}`);
  // A long list (the Jukebox opens at the playing song) scrolls it into view.
  if (hit.y < 0 || hit.y + hit.height > 720) {
    await page.mouse.move(900, 400); await page.mouse.wheel(0, hit.y < 0 ? -4000 : hit.y - 300); await nextFrames(page, 6);
    m = await menu(page); hit = m.hits.find(h => h.action === action);
  }
  await page.mouse.click(hit.x + hit.width / 2, hit.y + hit.height / 2); await nextFrames(page, 4);
}
const seed = () => {
  localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
  localStorage.setItem('atv-soundtrack-v1', JSON.stringify({ v: 3, enabled: true, volume: 0.45, history: [], historyIndex: -1 }));
};
async function ride(page) {
  await page.goto(url('/engine.html?venue=env01&uiDiagnostics=1&audioDiagnostics=1'));
  await page.waitForSelector('#ui-state', { state: 'attached', timeout: 120000 });
  await nextFrames(page, 20);
  await page.mouse.click(640, 430);
  await until(page, () => JSON.parse(document.documentElement.getAttribute('data-engine-audio') ?? 'null')?.musicTitle, null, 60000);
}
const benign = errors => errors.filter(e => !/m4a|\.bin|ERR_FAILED|MEDIA_ERR|Failed to load|Failed to fetch/i.test(String(e)));

const browser = await launch({ args: ['--autoplay-policy=no-user-gesture-required'] });
try {
  {
    const errors = [];
    const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
    await page.addInitScript(seed);
    await ride(page);
    await until(page, () => JSON.parse(document.documentElement.getAttribute('data-engine-audio')).musicTime > 1.5, null, 30000);
    const title = (await audio(page)).musicTitle;
    await key(page, 'Escape'); await onMenu(page, 'pause');
    await click(page, 'settings'); await click(page, 'music'); await onMenu(page, 'music');
    await click(page, 'musictoggle');
    await until(page, () => JSON.parse(document.documentElement.getAttribute('data-engine-audio')).musicPaused === true);
    const stopped = await audio(page);
    assert.equal(stopped.musicEnabled, false);
    // Gestures while Music is off never restart it.
    await page.mouse.click(4, 716);
    await key(page, 'ShiftLeft');
    await page.waitForTimeout(800);
    let a = await audio(page);
    assert.equal(a.musicPaused, true, 'a gesture does not restart music turned off');
    assert.ok(Math.abs(a.musicTime - stopped.musicTime) < 0.05, `held at ${stopped.musicTime}: ${a.musicTime}`);
    await click(page, 'musictoggle');
    await until(page, () => JSON.parse(document.documentElement.getAttribute('data-engine-audio')).musicPaused === false);
    a = await audio(page);
    assert.equal(a.musicTitle, title, 'the same song resumes');
    assert.ok(a.musicTime >= stopped.musicTime - 0.05 && a.musicTime < stopped.musicTime + 1, `resumes at ${stopped.musicTime}: ${a.musicTime}`);
    assertNoErrors(errors);
    console.log(`  Music Off/On: ${title} held at ${stopped.musicTime.toFixed(2)} s, resumed at ${a.musicTime.toFixed(2)} s`);
    await page.close();
  }
  {
    // No stream and the original songs unreadable: reads stop with Music on;
    // choosing a song once the network is back reads it.
    const manifest = await (await fetch(url('/content/manifest.json'))).json();
    const songs = new Set();
    for (const record of manifest.files.filter(r => r.path.startsWith('/AUDIO/MUSIC/'))) {
      for (const chunk of record.chunks ?? [record]) songs.add(`/content/${chunk.sha256}.bin`);
    }
    let failing = true;
    const errors = [];
    const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
    await page.route('**/*.m4a', route => route.abort());
    await page.route('**/content/*.bin', route =>
      failing && songs.has(new URL(route.request().url()).pathname) ? route.abort() : route.continue()
    );
    await page.addInitScript(seed);
    await ride(page);
    await until(page, () => JSON.parse(document.documentElement.getAttribute('data-engine-audio')).vagFallback === true, null, 60000);
    await page.waitForTimeout(1500);
    let a = await audio(page);
    assert.equal(a.musicQueued, 0, 'nothing decoded from an unreadable song');
    assert.equal(a.musicEnabled, true, 'Music stays on');
    failing = false;
    await page.waitForTimeout(1000);
    assert.equal((await audio(page)).musicQueued, 0, 'no retry until a new selection');
    await key(page, 'Escape'); await onMenu(page, 'pause');
    await click(page, 'jukebox'); await onMenu(page, 'jukebox');
    await click(page, 'song:3');
    await until(page, () => JSON.parse(document.documentElement.getAttribute('data-engine-audio')).musicQueued > 0, null, 30000).catch(async error => {
      throw new Error(`no retry: ${JSON.stringify(await audio(page))} ${JSON.stringify((await menu(page))?.music)}`, { cause: error });
    });
    a = await audio(page);
    assert.equal((await menu(page)).music.playing, 3);
    assertNoErrors(benign(errors));
    console.log(`  Failed read: retried on selection, ${a.musicTitle} queued ${a.musicQueued.toFixed(2)} s`);
    await page.close();
  }
  {
    // audio.test.mjs: returning to the menus ends a race hold and the sting.
    const errors = [];
    const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
    await page.addInitScript(seed);
    // A race link that plays at once still opens the frontend behind it.
    await page.goto(url('/engine.html?mode=single&activity=race&map=env01&laps=1&cpu=0&play=1&uiDiagnostics=1&audioDiagnostics=1'));
    await until(page, () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').race?.phase === 'intro', null, 180000);
    // The first click also skips the intro; the hold lasts until GO.
    await page.mouse.click(640, 430);
    await until(page, () => {
      const a = JSON.parse(document.documentElement.getAttribute('data-engine-audio') ?? 'null');
      const s = JSON.parse(document.querySelector('#ui-state').textContent);
      return a?.musicHeld === true && a.intOut === 1 && ['intro', 'countdown'].includes(s.race?.phase);
    }, null, 30000);
    const held = await audio(page);
    assert.equal(held.musicHeld, true, 'held before GO, with the intro stinger');
    await key(page, 'Escape'); await onMenu(page, 'pause');
    await nextFrames(page, 10);
    assert.equal((await audio(page)).musicHeld, true, 'Pause during a race keeps the race hold');
    await click(page, 'main');
    await until(page, () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === 'main', null, 30000);
    const soundtrack = (await read(page)).soundtrack;
    assert.equal(soundtrack.musicHeld, false, 'Quit to Main Menu ends the hold');
    assertNoErrors(errors);
    console.log('  Race hold: held with the intro stinger before GO, kept by Pause, ended in the menus');
    await page.close();
  }
  console.log('PASS: Rust soundtrack Music Off/On position, gestures, failed-read retry and race hold');
} finally {
  await browser.close();
}
