// Real-content Jukebox gate: Main → Jukebox and Pause → Jukebox play the
// original soundtrack as ranged AAC streams, with the original Jukebox art,
// transport, seeking and track selection. Set ATV_JUKEBOX_SCREENSHOTS to a
// path prefix (under ignored private storage) to keep inspected captures.
import assert from 'node:assert/strict';
import { assertNoErrors, base, button, launch, newPage, optionalScreenshot, waitForScreen } from './browser/harness.mjs';

const browser = await launch();
const errors = [];
const music = page => page.evaluate(() => window.atvTerrainStatus.audio.music);
const streamed = [];
async function open(viewport) {
  const page = await newPage(browser, { viewport, hasTouch: true, deviceScaleFactor: 2 }, errors);
  await page.addInitScript(() => Object.defineProperty(document, 'hasFocus', { value: () => true }));
  page.on('response', response => {
    const url = new URL(response.url());
    if (url.pathname.startsWith('/content/') && url.pathname.endsWith('.m4a'))
      streamed.push({ status: response.status(), type: response.headers()['content-type'], range: response.headers()['content-range'] ?? null });
  });
  await page.goto(base + '/play.html');
  await button(page, 'Press START').click({ timeout: 90000 });
  await waitForScreen(page, 'main');
  return page;
}
async function playing(page, check = () => true) {
  await page.waitForFunction(() => {
    const state = window.atvTerrainStatus?.audio?.music;
    return state?.mode === 'stream' && state.playing && state.positionSeconds > 0.2;
  }, null, { timeout: 60000 });
  const state = await music(page);
  assert(check(state), JSON.stringify(state));
  return state;
}
async function advances(page, from) {
  await page.waitForFunction(position => window.atvTerrainStatus.audio.music.positionSeconds > position + 0.4, from, { timeout: 20000 });
}
try {
  const page = await open({ width: 1280, height: 800 });
  await button(page, 'Jukebox').click();
  await waitForScreen(page, 'jukebox');
  const first = await playing(page);
  assert.equal(first.songs, 21);
  assert.equal(first.voices, 1);
  assert.equal(first.chunks >= 1, true);
  assert(first.durationSeconds > 150 && first.durationSeconds < 275, 'stream duration comes from the manifest');
  const audio = await page.evaluate(() => window.atvTerrainStatus.audio);
  assert.equal(audio.menuMusic.voices, 0, 'the Jukebox replaces the title tune');
  await page.waitForFunction(() => document.querySelector('.pause-panel').dataset.art === 'original', null, { timeout: 30000 });
  const layout = await page.evaluate(() => {
    const panel = document.querySelector('.pause-panel');
    return {
      tracks: panel.querySelectorAll('.jb-track').length,
      disc: getComputedStyle(panel.querySelector('.jb-disc')).backgroundImage,
      photo: getComputedStyle(panel.querySelector('.jb-art-photo')).backgroundImage,
      title: panel.querySelector('.jb-title').textContent,
      source: panel.querySelector('.jb-source').textContent,
      overflow: panel.scrollWidth - panel.clientWidth,
    };
  });
  assert.equal(layout.tracks, 21);
  assert.match(layout.disc, /^url\("blob:/); assert.match(layout.photo, /^url\("blob:/);
  assert.equal(layout.title, first.title);
  assert.equal(layout.source, 'Streaming');
  assert(layout.overflow <= 0, 'no horizontal overflow');

  // Track selection.
  const target = (first.playingIndex + 5) % 21;
  await page.locator(`.jb-track[data-action="song:${target}"]`).click();
  await page.waitForFunction(index => { const state = window.atvTerrainStatus.audio.music; return state.playingIndex === index && state.playing; }, target, { timeout: 30000 });
  assert.equal(await page.locator(`.jb-track[data-action="song:${target}"]`).getAttribute('data-playing'), 'true');
  const chosen = await music(page);
  assert.equal(await page.locator('.jb-title').textContent(), chosen.title);

  // Seeking through the slider lands at the requested time and keeps playing.
  const seekTo = Math.floor(chosen.durationSeconds / 2);
  await page.locator('input[data-setting=jukeseek]').evaluate((input, value) => {
    input.value = String(value);
    input.dispatchEvent(new Event('input', { bubbles: true }));
    input.dispatchEvent(new Event('change', { bubbles: true }));
  }, seekTo);
  await page.waitForFunction(value => { const state = window.atvTerrainStatus.audio.music; return state.playing && Math.abs(state.positionSeconds - value) < 3; }, seekTo, { timeout: 30000 });
  await advances(page, seekTo);
  await optionalScreenshot(page, 'ATV_JUKEBOX_SCREENSHOTS', 'main-1280');

  // Pause freezes the position; Play resumes the same song.
  await page.locator('[data-action=jukeplay]').click();
  await page.waitForFunction(() => !window.atvTerrainStatus.audio.music.playing);
  const paused = await music(page);
  assert.equal(paused.enabled, false);
  await page.waitForTimeout(400); // Time-based on purpose: a paused stream's position must not move.
  assert.equal((await music(page)).positionSeconds, paused.positionSeconds);
  await page.locator('[data-action=jukeplay]').click();
  await page.waitForFunction(() => window.atvTerrainStatus.audio.music.playing);
  assert.equal((await music(page)).playingIndex, target);
  await advances(page, paused.positionSeconds);

  // Next / previous walk the original shuffle and history.
  await page.locator('[data-action=jukenext]').click();
  await page.waitForFunction(index => { const state = window.atvTerrainStatus.audio.music; return state.playing && state.playingIndex !== index; }, target, { timeout: 30000 });
  await page.locator('[data-action=jukeprev]').click();
  await page.waitForFunction(index => { const state = window.atvTerrainStatus.audio.music; return state.playing && state.playingIndex === index; }, target, { timeout: 30000 });

  // Keyboard: Left/Right on the seek bar scrub by ten seconds.
  const before = (await music(page)).positionSeconds;
  await page.locator('input[data-setting=jukeseek]').focus();
  await page.keyboard.press('ArrowRight');
  await page.waitForFunction(value => window.atvTerrainStatus.audio.music.positionSeconds > value + 8, before, { timeout: 10000 });

  // Fixed layout at every size: the panel never scrolls, only the track list
  // does; Back and the player never cover the list; touch targets stay usable.
  for (const size of [{ width: 390, height: 844 }, { width: 320, height: 568 }, { width: 667, height: 375 }, { width: 844, height: 390 }, { width: 1280, height: 800 }]) {
    await page.setViewportSize(size);
    await page.waitForTimeout(400); // Time-based on purpose: the menu re-lays out through the worker after a resize.
    const layout = await page.evaluate(() => {
      const panel = document.querySelector('.pause-panel'), list = panel.querySelector('.jb-tracks');
      const rect = node => node.getBoundingClientRect();
      const back = rect(panel.querySelector('[data-action=back]')), tracks = rect(list), now = rect(panel.querySelector('.jb-now'));
      const rows = [...panel.querySelectorAll('.jb-track')].map(row => rect(row).height);
      return { pageScroll: panel.scrollHeight - panel.clientHeight, overflow: panel.scrollWidth - panel.clientWidth, listScrolls: list.scrollHeight > list.clientHeight,
        backClear: back.bottom <= tracks.top + 1 && back.bottom <= now.top + 1, listVisible: tracks.height, bottom: tracks.bottom - window.innerHeight,
        play: rect(panel.querySelector('[data-action=jukeplay]')).width, rows: Math.min(...rows), volume: rect(panel.querySelector('input[data-setting=jukevolume]')).width };
    });
    const label = `${size.width}x${size.height} ${JSON.stringify(layout)}`;
    assert(layout.pageScroll <= 1, `panel must not scroll: ${label}`);
    assert(layout.overflow <= 0, `no horizontal overflow: ${label}`);
    assert(layout.listScrolls && layout.listVisible >= 120, `the track list scrolls in its own region: ${label}`);
    assert(layout.backClear, `Back stays above the player and list: ${label}`);
    assert(layout.bottom <= 1, `list ends on screen: ${label}`);
    assert(layout.play >= 44 && layout.rows >= 44 && layout.volume >= 60, `touch targets: ${label}`);
    await optionalScreenshot(page, 'ATV_JUKEBOX_SCREENSHOTS', `main-${size.width}x${size.height}`);
  }

  await page.locator('[data-action=back]').click();
  await waitForScreen(page, 'main');
  await page.waitForFunction(() => !document.getAnimations().some(a => a.effect?.target?.id === 'menu-stage'));
  // The Jukebox's flex layout must not keep the hidden panel over the main menu.
  assert.equal(await page.evaluate(() => getComputedStyle(document.querySelector('.pause-panel')).display), 'none', 'Back closes the Jukebox panel');

  // Pause → Jukebox during a ride; the soundtrack keeps streaming.
  await button(page, 'Single Player').click();
  await button(page, 'Freestyle').click();
  await button(page, 'Ride Caribou Peak').click();
  await page.waitForFunction(() => window.atvTerrainStatus?.simulation && document.querySelector('#frontend').hidden, null, { timeout: 120000 });
  await button(page, 'Pause').click();
  await waitForScreen(page, 'pause');
  await button(page, 'Jukebox').click();
  await waitForScreen(page, 'jukebox');
  const ride = await playing(page);
  const rideLayout = await page.evaluate(() => {
    const panel = document.querySelector('.pause-panel');
    return { pageScroll: panel.scrollHeight - panel.clientHeight, volume: panel.querySelector('input[data-setting=jukevolume]').getBoundingClientRect().width };
  });
  assert(rideLayout.pageScroll <= 1 && rideLayout.volume >= 60, `ride Jukebox layout ${JSON.stringify(rideLayout)}`);
  assert.equal(await page.evaluate(() => document.querySelector('#frontend').dataset.context), '', 'ride Jukebox keeps the pause overlay');
  await advances(page, ride.positionSeconds);
  await optionalScreenshot(page, 'ATV_JUKEBOX_SCREENSHOTS', 'pause-1280');
  await page.keyboard.press('Escape');
  await waitForScreen(page, 'pause');

  assert(streamed.length > 0, 'songs stream from /content/*.m4a');
  assert(streamed.every(item => item.type === 'audio/mp4' && [200, 206].includes(item.status)), JSON.stringify(streamed.slice(0, 4)));
  assert(streamed.some(item => item.status === 206), 'media element uses ranged reads');
  const final = await music(page);
  assert.equal(final.failed, false);
  assertNoErrors(errors);
  console.log(JSON.stringify({ passed: true, streamResponses: streamed.length, ranged: streamed.filter(item => item.status === 206).length, underruns: final.underruns, songs: final.songs }));
} finally {
  await browser.close();
}
