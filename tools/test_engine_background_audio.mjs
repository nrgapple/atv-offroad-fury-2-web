// Leaving the page in the Rust engine's browser build (music.js park,
// audio.js setActive(false), releaseMediaSession): hiding the page
// suspends every AudioContext, unloads the soundtrack element and leaves no
// Media Session entry; returning reloads the song where it was. The page
// hide is simulated (document.visibilityState and its event), as an iOS app
// switch fires it. Host browser evidence; real iOS is not exercised here.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const audio = page => page.evaluate(() => JSON.parse(document.documentElement.getAttribute('data-engine-audio') ?? 'null'));
const until = (page, predicate, arg) => page.waitForFunction(predicate, arg, { timeout: 60000 });

const browser = await launch({ args: ['--autoplay-policy=no-user-gesture-required'] });
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await page.addInitScript(() => {
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.setItem('atv-soundtrack-v1', JSON.stringify({ v: 3, enabled: true, volume: 0.45, history: [], historyIndex: -1 }));
    // Record context suspends/resumes and the soundtrack element.
    window.contexts = [];
    const Base = window.AudioContext;
    window.AudioContext = class extends Base { constructor(...a) { super(...a); window.contexts.push(this); } };
    const create = document.createElement.bind(document);
    window.audioElements = [];
    const Audio = window.Audio;
    window.Audio = class extends Audio { constructor(...a) { super(...a); window.audioElements.push(this); } };
    document.createElement = (tag, ...rest) => { const e = create(tag, ...rest); if (String(tag).toLowerCase() === 'audio') window.audioElements.push(e); return e; };
    let state = 'visible';
    Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => state });
    Object.defineProperty(document, 'hidden', { configurable: true, get: () => state !== 'visible' });
    window.setVisibility = next => { state = next; document.dispatchEvent(new Event('visibilitychange')); };
  });
  await page.goto(url('/play.html?venue=env01&uiDiagnostics=1&audioDiagnostics=1'));
  await page.waitForSelector('#ui-state', { state: 'attached', timeout: 120000 });
  await nextFrames(page, 20);
  await page.mouse.click(640, 430);
  await until(page, () => JSON.parse(document.documentElement.getAttribute('data-engine-audio') ?? 'null')?.musicTime > 1);
  const before = await audio(page);
  // Hide the page: everything stops at once.
  const hidden = await page.evaluate(() => {
    window.setVisibility('hidden');
    const media = navigator.mediaSession;
    return { contexts: window.contexts.map(c => c.state), elements: window.audioElements.map(e => ({ src: e.getAttribute('src'), paused: e.paused })),
      session: media ? { metadata: media.metadata, state: media.playbackState } : null };
  });
  await page.waitForTimeout(500);
  const settled = await page.evaluate(() => ({ contexts: window.contexts.map(c => c.state), elements: window.audioElements.map(e => ({ src: e.getAttribute('src'), paused: e.paused })) }));
  assert.ok(settled.contexts.length > 0 && settled.contexts.every(s => s !== 'running'), JSON.stringify(settled));
  assert.ok(hidden.elements.length > 0 && hidden.elements.every(e => e.paused), JSON.stringify(hidden));
  assert.ok(hidden.elements.every(e => !e.src || e.src.startsWith('blob:')), `the song unloads: ${JSON.stringify(hidden)}`);
  if (hidden.session) assert.deepEqual([hidden.session.metadata, hidden.session.state], [null, 'none']);
  // Return: the song reloads where it was and plays on.
  await page.evaluate(() => window.setVisibility('visible'));
  await page.mouse.click(640, 430);
  await until(page, time => { const a = JSON.parse(document.documentElement.getAttribute('data-engine-audio') ?? 'null'); return a && !a.musicPaused && a.musicTime > time; }, before.musicTime);
  const after = await audio(page);
  assert.equal(after.musicTitle, before.musicTitle);
  const contexts = await page.evaluate(() => window.contexts.map(c => c.state));
  assert.ok(contexts.some(s => s === 'running'), JSON.stringify(contexts));
  assertNoErrors(errors);
  console.log(`PASS: Rust audio parks when the page hides (contexts ${settled.contexts.join(', ')}; song unloaded at ${before.musicTime.toFixed(1)} s) and resumes on return (${after.musicTime.toFixed(1)} s)`);
} finally {
  await browser.close();
}
