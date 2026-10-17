// Real-content gate for the iOS/PWA integrations: silent-switch audio session,
// app shortcut menus, the riding screen wake lock and Media Session metadata.
// iOS APIs are synthetic stand-ins here; physical-device behavior is unverified.
import assert from 'node:assert/strict';
import { assertNoErrors, base, button, launch, newPage, waitForGame, waitForScreen } from './browser/harness.mjs';
const browser = await launch();
const errors = [];
const checks = [];
// Stand-ins for Safari's navigator.audioSession and the Screen Wake Lock API.
function fakeIos() {
  window.fakeSession = { type: 'auto', history: [] };
  Object.defineProperty(navigator, 'audioSession', { configurable: true, value: {
    get type() { return window.fakeSession.type; },
    set type(value) { window.fakeSession.type = value; window.fakeSession.history.push(value); }
  } });
  window.fakeWake = { requests: 0, held: 0 };
  Object.defineProperty(navigator, 'wakeLock', { configurable: true, value: { request: async () => {
    window.fakeWake.requests++; window.fakeWake.held++;
    const listeners = [];
    return { addEventListener: (_, fn) => listeners.push(fn), release: async () => { window.fakeWake.held--; listeners.forEach(fn => fn()); } };
  } } });
}
async function page({ width = 390, height = 844, ios = true } = {}) {
  const p = await newPage(browser, { viewport: { width, height }, hasTouch: true }, errors);
  if (ios) await p.addInitScript(fakeIos);
  return p;
}
async function openExtras(p) {
  await button(p, 'Press START').click();
  await waitForScreen(p, 'main');
  await button(p, 'Settings').click();
  await waitForScreen(p, 'settings');
  await button(p, 'Extras').click();
  await waitForScreen(p, 'extras');
}
try {
  // Silent switch: default plays sound (playback session); the row toggles and persists.
  for (const [width, height] of [[390, 844], [844, 390]]) {
    const p = await page({ width, height });
    await p.goto(`${base}/play.html`);
    await waitForScreen(p, 'title');
    assert.equal(await p.evaluate(() => window.fakeSession.type), 'playback');
    await openExtras(p);
    await button(p, 'Silent switch: Play sound').click();
    await button(p, 'Silent switch: Mute game').waitFor();
    assert.equal(await p.evaluate(() => window.fakeSession.type), 'ambient');
    assert.equal(await p.evaluate(() => JSON.parse(localStorage.getItem('atv-audio-v1')).silent), 'mute');
    // The settings panel scrolls on short landscape screens; Back stays reachable.
    await button(p, 'Back').scrollIntoViewIfNeeded();
    const back = await button(p, 'Back').boundingBox();
    assert.ok(back.y >= 0 && back.y + back.height <= height + 1 && back.height >= 44, `Back reachable with a 44px target: ${JSON.stringify(back)} at ${width}x${height}`);
    await p.reload();
    await waitForScreen(p, 'title');
    assert.equal(await p.evaluate(() => window.fakeSession.type), 'ambient', 'saved preference applies at startup');
    await p.close();
  }
  const plain = await page({ ios: false });
  await plain.goto(`${base}/play.html`);
  await waitForScreen(plain, 'title');
  await openExtras(plain);
  assert.equal(await plain.getByRole('button', { name: /^Silent switch/ }).count(), 0, 'row hidden without an audio session');
  await plain.close();
  checks.push('silent switch row in portrait/landscape, playback/ambient session types, persistence, hidden without the API');

  // Shortcuts open a menu and keep the saved selection.
  const s = await page();
  await s.goto(`${base}/play.html`);
  await waitForScreen(s, 'title');
  await s.evaluate(() => localStorage.setItem('atv-player-selection', JSON.stringify({ quadId: 7, riderId: 1, playerName: 'Shortcut' })));
  const saved = await s.evaluate(() => localStorage.getItem('atv-player-selection'));
  for (const [menu, screen] of [['freeride', 'tracks'], ['race', 'race']]) {
    await s.goto(`${base}/play.html?menu=${menu}`);
    await waitForScreen(s, screen);
    assert.equal(await s.locator('#frontend').getAttribute('data-error'), null);
  }
  assert.equal(await s.evaluate(() => localStorage.getItem('atv-player-selection')), saved, 'shortcuts never overwrite the saved rider');
  await s.close();
  checks.push('?menu=freeride and ?menu=race shortcuts open their menus without replacing the saved selection');

  // Wake lock: held while riding, released by the pause menu, reacquired on resume.
  const r = await page();
  await r.goto(`${base}/play.html?mode=single&map=env01`);
  await waitForGame(r);
  await r.waitForFunction(() => window.atvTerrainStatus.simulation && document.querySelector('#frontend').hidden, null, { timeout: 120000 });
  await r.waitForFunction(() => window.fakeWake.held === 1);
  await button(r, 'Pause').click();
  await waitForScreen(r, 'pause');
  await r.waitForFunction(() => window.fakeWake.held === 0);
  await button(r, 'Resume').click();
  await r.waitForFunction(() => window.fakeWake.held === 1 && window.fakeWake.requests === 2);
  checks.push('screen wake lock held while riding, released on pause, reacquired on resume');

  // Media Session mirrors the streamed soundtrack.
  await r.waitForFunction(() => Boolean(navigator.mediaSession?.metadata?.title), null, { timeout: 60000 });
  const media = await r.evaluate(() => ({ title: navigator.mediaSession.metadata.title, album: navigator.mediaSession.metadata.album,
    playing: navigator.mediaSession.playbackState, song: window.atvTerrainStatus.audio.music.title }));
  assert.equal(media.title, media.song);
  assert.equal(media.album, 'Fury Remastered soundtrack');
  assert.equal(media.playing, 'playing');
  await r.close();
  checks.push('Media Session metadata and playback state follow the current song');
  assertNoErrors(errors);
  console.log(`PASS: ${checks.join('; ')}. Synthetic iOS APIs; physical iPhone/iPad behavior is unverified.`);
} finally {
  await browser.close();
}
