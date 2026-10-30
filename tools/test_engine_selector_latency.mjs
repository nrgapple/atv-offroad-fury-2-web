// Selector responsiveness (research/RUST_UI.md "Selector input"): on a phone
// (390x844, DPR 3, touch; Chromium at x4 CPU throttle, WebKit unthrottled),
// rapid taps on Single Race's course arrows and the garage's vehicle arrows,
// overlapping two-finger taps and rapid arrow keys each step exactly once, and
// every input shows in the next frame drawn after it. A held arrow (finger or
// key) repeats. Single Race's photo catches up with the course once the taps
// stop, and its photo opens the Choose Course picker.
// Run: node tools/run_browser_tests.mjs --only test_engine_selector_latency.mjs
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, unlockStore, url } from './browser/harness.mjs';

const PREV = 2402;
const NEXT = 2403;
const QUAD_PREV = 2405;
const QUAD_NEXT = 2406;
const COURSES = 2416;
const TAPS = 10;
// Taps a second; touches overlap in the two-finger run.
const RATE = 10;
// Host Chromium at x4 throttle: an input shows within this long (ms).
const BUDGET = 250;

const SELECTORS = [
  { page: 'race', value: f => f.race.course, path: ['race', 'course'], prev: PREV, next: NEXT },
  { page: 'garage', value: f => f.quad, path: ['quad'], prev: QUAD_PREV, next: QUAD_NEXT }
];

/** Records every ui-state publish (one per frontend frame) and each input. */
function instrument(path) {
  const log = (window.__sel = { publishes: [], inputs: [] });
  const read = f => path.reduce((o, k) => (o == null ? o : o[k]), f);
  const watch = () => {
    const el = document.getElementById('ui-state');
    if (!el) return setTimeout(watch, 50);
    new MutationObserver(() => {
      try {
        const f = JSON.parse(el.textContent).front;
        log.publishes.push([performance.now(), read(f)]);
      } catch {}
    }).observe(el, { childList: true, characterData: true, subtree: true });
  };
  watch();
  for (const type of ['pointerup', 'keydown'])
    addEventListener(type, e => log.inputs.push([type, performance.now(), e.repeat]), true);
}

const front = page => page.evaluate(() => JSON.parse(document.getElementById('ui-state').textContent).front);
const centre = (f, id) => {
  const h = f.hits.find(h => h.id === id);
  assert.ok(h, `hit ${id} on ${f.page}`);
  return { x: h.x + h.width / 2, y: h.y + h.height / 2 };
};
const sleep = ms => new Promise(r => setTimeout(r, ms));

async function open(page, sel) {
  await page.goto(url(`/play.html?uiDiagnostics=1&front=${sel.page}`));
  await page.waitForFunction(
    p => {
      const el = document.getElementById('ui-state');
      const f = el && JSON.parse(el.textContent).front;
      return f?.page === p && !f.motion && !f.exit && f.photos > 0;
    },
    sel.page,
    { timeout: 120000 }
  );
  await nextFrames(page, 4);
}

/** Waits for the selection to stop changing; returns it. */
async function settle(page, sel) {
  let last = sel.value(await front(page));
  for (let still = 0; still < 6; ) {
    await sleep(100); // Deliberately wall-clock: input queued in the browser drains.
    const now = sel.value(await front(page));
    still = now === last ? still + 1 : 0;
    last = now;
  }
  return last;
}

/** Steps back one at a time, waiting for each; returns the value. */
async function stepBack(page, sel, count) {
  const prev = centre(await front(page), sel.prev);
  for (let i = 0; i < count; i++) {
    const before = sel.value(await front(page));
    await page.touchscreen.tap(prev.x, prev.y);
    await page.waitForFunction(
      ([path, before]) => {
        const f = JSON.parse(document.getElementById('ui-state').textContent).front;
        return path.reduce((o, k) => o?.[k], f) !== before;
      },
      [sel.path, before],
      { timeout: 20000 }
    );
  }
  return sel.value(await front(page));
}

/** Steps back until `start` shows (at most `max`); returns the steps taken. */
async function countBack(page, sel, start, max) {
  for (let n = 0; n < max; n++) {
    if (sel.value(await front(page)) === start) return n;
    await stepBack(page, sel, 1);
  }
  return max;
}

/**
 * Every input shows in the first frame published after it: the value
 * published then differs from the one before the input.
 */
async function assertEachShown(page, label, from, kind) {
  const log = await page.evaluate(from => ({ p: window.__sel.publishes.filter(x => x[0] >= from), i: window.__sel.inputs.filter(x => x[1] >= from) }), from);
  const inputs = log.i.filter(i => i[0] === kind);
  const before = await page.evaluate(from => window.__sel.publishes.filter(x => x[0] < from).at(-1)?.[1], from);
  let shown = before;
  let worst = 0;
  for (const [, at] of inputs) {
    const prior = [before, ...log.p.filter(p => p[0] < at).map(p => p[1])].at(-1);
    const next = log.p.find(p => p[0] >= at);
    assert.ok(next, `${label}: a frame after the input at ${at.toFixed(0)}`);
    assert.notEqual(next[1], prior, `${label}: the input at ${at.toFixed(0)} shows in the next frame`);
    worst = Math.max(worst, next[0] - at);
    shown = next[1];
  }
  assert.ok(worst <= BUDGET, `${label}: slowest step ${worst.toFixed(0)} ms (budget ${BUDGET})`);
  return { worst, shown };
}

const errors = [];
const report = {};
for (const engine of ['chromium', 'webkit']) {
  const browser = await launch({ engine });
  try {
    const options = { viewport: { width: 390, height: 844 }, deviceScaleFactor: 3, hasTouch: true };
    if (engine === 'chromium') options.isMobile = true;
    for (const sel of SELECTORS) {
      const page = await newPage(browser, options, errors);
      await unlockStore(page);
      await page.addInitScript(() => {
        localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
        localStorage.removeItem('atv-player-selection');
      });
      await page.addInitScript(instrument, sel.path);
      await open(page, sel);
      const cdp = engine === 'chromium' ? await page.context().newCDPSession(page) : null;
      if (cdp) await cdp.send('Emulation.setCPUThrottlingRate', { rate: 4 });
      const label = `${engine} ${sel.page}`;
      const next = centre(await front(page), sel.next);
      const now = () => page.evaluate(() => performance.now());

      // Rapid single-finger taps.
      let start = sel.value(await front(page));
      let from = await now();
      if (cdp) {
        const sent = [];
        for (let i = 0; i < TAPS; i++) {
          const p = { x: next.x, y: next.y, radiusX: 4, radiusY: 4, force: 1, id: 1 };
          sent.push(cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [p] }));
          await sleep(40);
          sent.push(cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] }));
          await sleep(1000 / RATE - 40);
        }
        await Promise.all(sent);
      } else {
        for (let i = 0; i < TAPS; i++) await page.touchscreen.tap(next.x, next.y);
      }
      await settle(page, sel);
      const taps = await assertEachShown(page, `${label} taps`, from, 'pointerup');
      assert.equal(await stepBack(page, sel, TAPS), start, `${label}: ${TAPS} taps stepped exactly ${TAPS} times`);

      // Two thumbs: each tap lands before the other finger lifts.
      if (cdp) {
        start = sel.value(await front(page));
        const active = new Map();
        const events = [];
        const period = 1000 / RATE;
        for (let i = 0; i < TAPS; i++) events.push([i * period, true, i], [i * period + period * 1.5, false, i]);
        events.sort((a, b) => a[0] - b[0]);
        const t0 = Date.now();
        const sent = [];
        for (const [at, down, i] of events) {
          const wait = t0 + at - Date.now();
          if (wait > 0) await sleep(wait);
          const id = (i % 2) + 1;
          if (down) active.set(id, { x: next.x + (id === 1 ? -5 : 5), y: next.y + (id === 1 ? -4 : 4), radiusX: 4, radiusY: 4, force: 1, id });
          else active.delete(id);
          sent.push(cdp.send('Input.dispatchTouchEvent', { type: down ? 'touchStart' : 'touchEnd', touchPoints: [...active.values()] }));
        }
        await Promise.all(sent);
        await settle(page, sel);
        assert.equal(await stepBack(page, sel, TAPS), start, `${label}: ${TAPS} overlapping taps stepped exactly ${TAPS} times`);
      }

      // Rapid arrow keys (the selector's row has the focus).
      start = sel.value(await front(page));
      from = await now();
      if (cdp) {
        const k = { key: 'ArrowRight', code: 'ArrowRight', windowsVirtualKeyCode: 39 };
        const sent = [];
        for (let i = 0; i < TAPS; i++) {
          sent.push(cdp.send('Input.dispatchKeyEvent', { type: 'rawKeyDown', ...k }));
          await sleep(30);
          sent.push(cdp.send('Input.dispatchKeyEvent', { type: 'keyUp', ...k }));
          await sleep(1000 / RATE - 30);
        }
        await Promise.all(sent);
      } else {
        for (let i = 0; i < TAPS; i++) await page.keyboard.press('ArrowRight');
      }
      await settle(page, sel);
      const keys = await assertEachShown(page, `${label} keys`, from, 'keydown');
      assert.equal(await stepBack(page, sel, TAPS), start, `${label}: ${TAPS} keys stepped exactly ${TAPS} times`);

      // A held arrow repeats: a finger (after 0.4 s, ~9 a second) and a key
      // (the system's key repeat).
      if (cdp) {
        start = sel.value(await front(page));
        const p = { x: next.x, y: next.y, radiusX: 4, radiusY: 4, force: 1, id: 1 };
        await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [p] });
        await sleep(1300); // Deliberately wall-clock: a held finger.
        await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
        await settle(page, sel);
        const held = await countBack(page, sel, start, 20);
        assert.ok(held >= 3, `${label}: a held arrow repeats (${held} steps in 1.3 s)`);
        start = sel.value(await front(page));
        const k = { key: 'ArrowRight', code: 'ArrowRight', windowsVirtualKeyCode: 39 };
        await cdp.send('Input.dispatchKeyEvent', { type: 'rawKeyDown', ...k });
        for (let i = 0; i < 8; i++) {
          await sleep(50);
          await cdp.send('Input.dispatchKeyEvent', { type: 'rawKeyDown', autoRepeat: true, ...k });
        }
        await cdp.send('Input.dispatchKeyEvent', { type: 'keyUp', ...k });
        await settle(page, sel);
        assert.equal(await stepBack(page, sel, 9), start, `${label}: a held key steps on each of its 9 key events`);
      }

      if (sel.page === 'race') {
        // The photo catches up with the course, and the photo opens the picker.
        await page.waitForFunction(
          () => {
            const a = JSON.parse(document.getElementById('ui-state').textContent).front.courseArt;
            return !a.pending && a.shown.hero === a.wanted.hero && a.shown.bg === a.wanted.bg;
          },
          null,
          { timeout: 20000 }
        );
        let f = await front(page);
        assert.ok(f.hits.some(h => h.id === COURSES), `${label}: All courses on screen`);
        const photo = centre(f, COURSES);
        await page.touchscreen.tap(photo.x, photo.y);
        await page.waitForFunction(() => JSON.parse(document.getElementById('ui-state').textContent).front.courseSheet, null, { timeout: 10000 });
        await page.keyboard.press('Escape');
        await page.waitForFunction(() => !JSON.parse(document.getElementById('ui-state').textContent).front.courseSheet, null, { timeout: 10000 });
      }
      report[label] = { tapsWorstMs: Math.round(taps.worst), keysWorstMs: Math.round(keys.worst) };
      await page.context().close();
    }
  } finally {
    await browser.close();
  }
}
assertNoErrors(errors);
console.log(JSON.stringify(report));
console.log('PASS rapid taps, overlapping taps, keys and held arrows step every time on Single Race and the garage');
