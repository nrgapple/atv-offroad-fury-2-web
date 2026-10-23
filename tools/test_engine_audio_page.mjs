// Pause → Settings → Audio in the Rust engine (frontend.js musicPage): the
// soundtrack list four rows a page with the playing song marked, the status
// line, Previous/Next paging that wraps, and a row choice that plays the
// song. Then music.js's MusicStream fallback: when the hosted stream cannot
// load, the original VAGs play through the mixer's music bus instead.
// Host browser evidence.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state').textContent));
const menu = async page => (await read(page)).toolbar.menu;
const audio = page => page.evaluate(() => JSON.parse(document.documentElement.getAttribute('data-engine-audio') ?? 'null'));
const until = (page, predicate, arg, timeout = 30000) => page.waitForFunction(predicate, arg, { timeout });
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }
async function click(page, action) {
  const m = await menu(page), hit = m.hits.find(h => h.action === action);
  assert.ok(hit, `${action} on ${m.page}: ${m.hits.map(h => h.action)}`);
  await page.mouse.click(hit.x + hit.width / 2, hit.y + hit.height / 2); await nextFrames(page, 4);
}
const songs = m => m.hits.filter(h => h.action.startsWith('song:')).map(h => ({ ...h, label: m.labels[h.action] ?? '' }));
const seed = () => {
  localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
  localStorage.setItem('atv-soundtrack-v1', JSON.stringify({ v: 3, enabled: true, volume: 0.45, history: [], historyIndex: -1 }));
};
async function ride(page) {
  await page.goto(url('/play.html?venue=env01&uiDiagnostics=1&audioDiagnostics=1'));
  await page.waitForSelector('#ui-state', { state: 'attached', timeout: 120000 });
  await nextFrames(page, 20);
  await page.mouse.click(640, 430);
  await until(page, () => JSON.parse(document.documentElement.getAttribute('data-engine-audio') ?? 'null')?.musicTitle, null, 60000);
}

const browser = await launch({ args: ['--autoplay-policy=no-user-gesture-required'] });
try {
  {
    const errors = [];
    const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
    await page.addInitScript(seed);
    await ride(page);
    await key(page, 'Escape');
    await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).toolbar?.menu?.page === 'pause');
    await click(page, 'settings');
    await click(page, 'music');
    await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).toolbar.menu.page === 'music');
    let m = await menu(page);
    const first = songs(m).map(h => h.label);
    // The status line: "On / Volume 45% / <song>" (screenshot).
    assert.equal(first.length, 4, JSON.stringify(first));
    assert.equal(m.musicPage, 0);
    assert.equal(songs(m).filter(h => h.label.includes('Playing / ')).length, m.music.playing < 4 ? 1 : 0);
    for (const action of ['musicprev', 'musicnext', 'musictoggle', 'musicvolume', 'sfxvolume', 'back']) assert.ok(m.hits.some(h => h.action === action), action);
    await click(page, 'musicnext');
    m = await menu(page);
    assert.notDeepEqual(songs(m).map(h => h.label), first);
    assert.equal(m.musicPage, 1);
    await click(page, 'musicprev');
    await click(page, 'musicprev');
    m = await menu(page);
    assert.equal(m.musicPage, 5, 'Previous wraps to the last of six pages');
    assert.equal(songs(m).length, 1, 'the last page holds the 21st song');
    // Choosing a row plays it and marks it Playing.
    const chosen = songs(m)[0].action;
    await click(page, chosen);
    await until(page, id => JSON.parse(document.querySelector('#ui-state').textContent).toolbar.menu.labels[id]?.includes('Playing / '), chosen);
    console.log(`  Audio page: ${first.join(' | ')}; chose ${chosen}: ${(await audio(page)).musicTitle}`);
    if (process.env.ATV_SHOT) await page.screenshot({ path: process.env.ATV_SHOT });
    assertNoErrors(errors);
    await page.close();
  }
  {
    // No stream can load: the original VAG plays on the mixer's music bus.
    const errors = [];
    const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
    await page.route('**/*.m4a', route => route.abort());
    await page.addInitScript(seed);
    await ride(page);
    await until(page, () => {
      const a = JSON.parse(document.documentElement.getAttribute('data-engine-audio') ?? 'null');
      return a?.vagFallback === true && a.musicQueued > 0;
    }, null, 60000);
    const a = await audio(page);
    console.log(`  Fallback: ${a.musicTitle}, queued ${a.musicQueued}`);
    assertNoErrors(errors.filter(e => !/m4a|ERR_FAILED|MEDIA_ERR|Failed to load/i.test(String(e))));
    await page.close();
  }
  console.log('PASS: Rust Audio page song list and VAG soundtrack fallback');
} finally {
  await browser.close();
}
