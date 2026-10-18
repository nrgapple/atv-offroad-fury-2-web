// Real-content music lifecycle across menus and a race: the menu tune, a
// transient Jukebox pause (not the saved Music setting), the race intro hold,
// music at GO, Restart Race, Quit to Main Menu, the Jukebox afterwards, and
// leaving/returning to the app (Music stays on; no system media controls).
import assert from 'node:assert/strict';
import { assertNoErrors, base, button, launch, newPage, waitForScreen } from './browser/harness.mjs';
const browser = await launch(); const errors = [];
const page = await newPage(browser, { viewport: { width: 1280, height: 800 } }, errors);
await page.addInitScript(() => Object.defineProperty(document, 'hasFocus', { value: () => true }));
await page.goto(base + '/play.html');
await button(page, 'Press START').click({ timeout: 90000 });
await waitForScreen(page, 'main');
const s = () => page.evaluate(() => { const a = window.atvTerrainStatus?.audio; return { page: document.querySelector('#frontend').dataset.page, phase: window.atvTerrainStatus?.frame?.race?.phase ?? null, menu: a?.menuMusic?.voices, intOut: a?.intOut?.voices?.length ?? 0, m: a?.music && { playing: a.music.playing, enabled: a.music.enabled, paused: a.music.paused, title: a.music.title, pos: +a.music.positionSeconds.toFixed(1) } }; });
const until = async (label, test, timeout = 60000) => { const end = Date.now() + timeout; let st; while (Date.now() < end) { st = await s(); if (test(st)) { console.log('OK', label, JSON.stringify(st)); return st; } await page.waitForTimeout(500); } throw new Error(`${label}: ${JSON.stringify(st)}`); };
await until('menu tune at start', st => st.menu === 1);
await button(page, 'Jukebox').click(); await waitForScreen(page, 'jukebox');
await until('jukebox plays', st => st.m.playing);
await page.locator('[data-action=jukeplay]').click();
await until('jukebox paused, music still enabled', st => !st.m.playing && st.m.paused && st.m.enabled);
await page.locator('[data-action=back]').click(); await waitForScreen(page, 'main');
await until('menu tune after a Jukebox pause', st => st.menu === 1 && !st.m.paused);
await page.waitForTimeout(800); { const st = await s(); assert(!st.m.playing && st.menu === 1, `only the menu tune: ${JSON.stringify(st)}`); console.log('OK only the menu tune'); }
await button(page, 'Single Player').click(); await button(page, 'Race').click(); await waitForScreen(page, 'race');
await button(page, 'Start Race').click();
await until('intro is held', st => st.phase === 'intro' && !st.m.playing, 120000);
await until('music at GO', st => st.phase === 'racing' && st.m.playing && st.m.pos > 1, 60000);
await button(page, 'Pause').click(); await waitForScreen(page, 'pause');
await button(page, 'Restart Race').click();
await until('restart holds', st => ['intro', 'countdown'].includes(st.phase) && !st.m.playing, 120000);
await until('music at GO after restart', st => st.phase === 'racing' && st.m.playing && st.m.pos > 1, 60000);
await button(page, 'Pause').click(); await waitForScreen(page, 'pause');
await button(page, 'Quit to Main Menu').click(); await waitForScreen(page, 'main');
await until('menu tune after quitting a race', st => st.menu === 1 && !st.m.playing);
await button(page, 'Jukebox').click(); await waitForScreen(page, 'jukebox');
await until('jukebox after a race', st => st.m.playing);
// Leaving the app: the song unloads, Music stays on, nothing is published to
// the system media controls, and the same song resumes on return.
const playingBefore = (await until('jukebox playing before leaving', st => st.m.playing)).m;
const setHidden = hidden => page.evaluate(hidden => {
  Object.defineProperty(document, 'hidden', { configurable: true, get: () => hidden });
  Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => (hidden ? 'hidden' : 'visible') });
  document.dispatchEvent(new Event('visibilitychange'));
}, hidden);
await setHidden(true);
const away = await until('unloaded while away', st => !st.m.playing && st.m.enabled);
const session = await page.evaluate(() => ({ metadata: navigator.mediaSession?.metadata ?? null, state: navigator.mediaSession?.playbackState ?? 'none' }));
assert.equal(session.metadata, null); assert.equal(session.state, 'none');
await setHidden(false);
const back = await until('same song resumes on return', st => st.m.playing && st.m.title === playingBefore.title && st.m.pos >= away.m.pos);
assert(back.m.pos >= playingBefore.pos, 'resumes where it was, not from the start');
assertNoErrors(errors);
console.log('PASS: menu tune, transient Jukebox pause, race intro hold, music at GO, Restart Race, quit to menu, Jukebox after a race, leave and return');
await browser.close();
