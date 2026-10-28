// Kinetic menu scrolling (crate atv-ui scroll.rs): lists and overflowing pages
// follow the finger at once, keep moving after a flick and settle, stretch
// past the ends and spring back; a drag never selects, a tap still selects and
// a touch on a moving list catches it without selecting. Keyboard focus glides
// rows into view. Also samples frame intervals during a fling.
// Chromium uses CDP touch events (real touch input); WebKit gets synthetic
// touch PointerEvents. Run: node tools/run_browser_tests.mjs --only test_engine_menu_scroll.mjs
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, unlockStore, url } from './browser/harness.mjs';

const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front);
const onPage = (page, name) =>
  page.waitForFunction(
    name => {
      const front = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front;
      return front?.page === name && !front.motion && front.photos > 0;
    },
    name,
    { timeout: 120000 }
  );
const settled = page =>
  page.waitForFunction(() => !JSON.parse(document.querySelector('#ui-state').textContent).front.kinetic.active, null, {
    timeout: 10000
  });
const hit = (front, id) => front.hits.find(h => h.id === id);
const centre = h => ({ x: h.x + h.width / 2, y: h.y + h.height / 2 });

/** Touch helpers for one page: down/move/up at CSS points ('lift' = a last move and the
 *  release together, as a finger lifts right after moving). */
async function toucher(page, engine) {
  const cdp = engine === 'chromium' ? await page.context().newCDPSession(page) : null;
  const send = async (phase, at) => {
    await cdp.send('Input.dispatchTouchEvent', {
      type: { down: 'touchStart', move: 'touchMove', up: 'touchEnd' }[phase],
      touchPoints: phase === 'up' ? [] : [{ id: 1, x: at.x, y: at.y }]
    });
  };
  return async (phase, at) => {
    const phases = phase === 'lift' ? ['move', 'up'] : [phase];
    if (cdp) {
      // Queued together: a loaded renderer must not stretch the lift's gap.
      await Promise.all(phases.map(p => send(p, at)));
    } else {
      await page.evaluate(
        ({ phases, at }) => {
          for (const phase of phases) {
            const type = { down: 'pointerdown', move: 'pointermove', up: 'pointerup' }[phase];
            const event = new PointerEvent(type, {
              pointerId: 7,
              pointerType: 'touch',
              isPrimary: true,
              clientX: at.x,
              clientY: at.y,
              button: phase === 'move' ? -1 : 0,
              buttons: phase === 'up' ? 0 : 1,
              bubbles: true,
              cancelable: true
            });
            if (phase === 'move') event.getCoalescedEvents = () => [event];
            document.querySelector('canvas').dispatchEvent(event);
          }
        },
        { phases, at }
      );
    }
    await nextFrames(page, 1);
  };
}

/** A drag from `from` by `dy` in `steps` frame-paced moves; returns the offsets seen while held. */
async function swipe(touch, page, from, dy, steps, pick) {
  const seen = [];
  await touch('down', from);
  for (let i = 1; i < steps; i++) {
    await touch('move', { x: from.x, y: from.y + (dy * i) / steps });
    seen.push(pick(await read(page)));
  }
  await touch('lift', { x: from.x, y: from.y + dy });
  seen.push(pick(await read(page)));
  return seen;
}

/** Offsets every frame until the motion rests, with frame intervals (ms). */
async function follow(page, path) {
  return page.evaluate(async path => {
    const out = [];
    const t0 = performance.now();
    let last = t0;
    for (;;) {
      await new Promise(requestAnimationFrame);
      const now = performance.now();
      const front = JSON.parse(document.querySelector('#ui-state').textContent).front;
      const value = path.reduce((v, k) => v?.[k], front);
      out.push({ t: now - t0, dt: now - last, value, active: front.kinetic.active });
      last = now;
      if (!front.kinetic.active || now - t0 > 6000) return out;
    }
  }, path);
}

const errors = [];
const report = [];
for (const engine of ['chromium', 'webkit']) {
  const browser = await launch({ engine });
  try {
    // --- Select Track list at phone portrait -------------------------------
    const page = await newPage(
      browser,
      { viewport: { width: 390, height: 844 }, deviceScaleFactor: 3, hasTouch: true, isMobile: engine === 'chromium' },
      errors
    );
    await unlockStore(page);
    await page.goto(url('/play.html?uiDiagnostics=1&front=tracks'));
    await onPage(page, 'tracks');
    const touch = await toucher(page, engine);
    let f = await read(page);
    const list = hit(f, 2400);
    assert.ok(list, 'course list');
    const [, listMax] = f.kinetic.list;
    assert.ok(listMax > 400, `list scrolls (${listMax})`);
    const track = f.track;
    // A card that is not the current course: a drag starting on it must not select it.
    const second = hit(f, 2201);
    const start = { x: centre(second).x, y: Math.min(list.y + list.height - 20, centre(second).y + 160) };

    // 1. Immediate tracking: the list follows within the first frames past the slop.
    const held = await swipe(touch, page, start, -260, 10, s => s.kinetic.list[0]);
    assert.ok(held[1] > 0, `follows the finger at once: ${held}`);
    for (let i = 1; i < held.length; i++) assert.ok(held[i] >= held[i - 1], `tracks monotonically ${held}`);
    const released = held.at(-1);
    // 2. Momentum: it keeps moving after release, decelerates, then rests.
    const path = await follow(page, ['kinetic', 'list', 0]);
    const rest = path.at(-1).value;
    assert.ok(
      rest > released + 60,
      `${engine}: momentum after release: ${released} -> ${rest} held ${held} path ${JSON.stringify(path.slice(0, 5))}`
    );
    assert.ok(path.length > 4 && !path.at(-1).active, 'settles');
    const intervals = path
      .slice(1, -1)
      .map(p => p.dt)
      .sort((a, b) => a - b);
    const p95 = intervals[Math.floor(intervals.length * 0.95)] ?? 0;
    report.push(
      `${engine} fling: ${released.toFixed(0)} -> ${rest.toFixed(0)}px over ${path.at(-1).t.toFixed(0)} ms, ${path.length} frames, p95 frame ${p95.toFixed(1)} ms`
    );
    f = await read(page);
    assert.equal(f.track, track, 'a drag does not select');

    // 3. A tap still selects: a fully visible card (the current course, the
    // first, has scrolled out of view, so this is another one).
    assert.ok(f.kinetic.list[0] > 200, 'the current card is out of view');
    const visible = f.hits.find(
      h => h.id >= 2201 && h.id < 2300 && h.y > list.y + 4 && h.y + h.height < list.y + list.height - 4
    );
    const before = f.track;
    await touch('down', centre(visible));
    await touch('up', centre(visible));
    await nextFrames(page, 3);
    f = await read(page);
    assert.notEqual(f.track, before, 'a tap on a card selects it');

    // 4. Catch: a touch on a moving list stops it and does not select.
    const now = f.track;
    await swipe(touch, page, { x: start.x, y: list.y + list.height - 30 }, -200, 6, () => 0);
    await nextFrames(page, 3);
    const moving = (await read(page)).kinetic;
    assert.ok(moving.active, `${engine}: flung again ${JSON.stringify(moving)}`);
    const grab = { x: start.x, y: list.y + list.height / 2 };
    await touch('down', grab);
    const caught = (await read(page)).kinetic.list[0];
    await nextFrames(page, 6);
    const still = (await read(page)).kinetic.list[0];
    assert.ok(Math.abs(still - caught) < 0.5, `caught: ${caught} -> ${still}`);
    await touch('up', grab);
    await settled(page);
    assert.equal((await read(page)).track, now, 'the catching touch does not select');

    // 5. Rubber band at the top, then spring back.
    // Scroll to the top with drags, then pull down past it.
    for (let i = 0; i < 12 && (await read(page)).kinetic.list[0] > 0; i++) {
      await swipe(touch, page, { x: start.x, y: list.y + 40 }, 400, 8, () => 0);
      await settled(page);
    }
    const stretch = await swipe(touch, page, { x: start.x, y: list.y + 40 }, 150, 8, s => s.kinetic.list[0]);
    assert.ok(Math.min(...stretch) < -10 && Math.min(...stretch) > -150, `stretches with resistance: ${stretch}`);
    await settled(page);
    assert.equal((await read(page)).kinetic.list[0], 0, 'springs back to the top');
    await page.close();

    // --- Main menu rows at phone landscape: they outgrow the column --------
    const land = await newPage(
      browser,
      { viewport: { width: 844, height: 390 }, deviceScaleFactor: 3, hasTouch: true, isMobile: engine === 'chromium' },
      errors
    );
    await unlockStore(land);
    await land.goto(url('/play.html?uiDiagnostics=1&front=main'));
    await land.waitForFunction(
      () => {
        const f = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front;
        return f?.page === 'main' && !f.motion;
      },
      null,
      { timeout: 120000 }
    );
    const landTouch = await toucher(land, engine);
    f = await read(land);
    const rows = hit(f, 2450);
    assert.ok(rows && f.kinetic.rows[1] > 20, `main rows scroll in landscape (${JSON.stringify(f.kinetic.rows)})`);
    const selection = f.selection;
    const rowsFrom = { x: rows.x + rows.width / 2, y: rows.y + rows.height - 30 };
    const rowsHeld = await swipe(landTouch, land, rowsFrom, -150, 8, s => s.kinetic.rows[0]);
    assert.ok(rowsHeld.at(-1) > 0, `rows follow the finger: ${rowsHeld}`);
    await settled(land);
    f = await read(land);
    assert.equal(f.selection, selection, 'scrolling the rows does not select');
    // Keyboard focus glides the selected row into view (top and back to the last).
    await land.mouse.click(4, 4).catch(() => {});
    // Selection wraps, so step exactly to the first row, then to the last.
    // (Down then Up: the first row, scrolled out of view by the drag, comes back.)
    await land.keyboard.press('ArrowDown');
    for (let i = 0; i <= f.selection; i++) await land.keyboard.press('ArrowUp');
    await nextFrames(land, 3);
    await settled(land);
    f = await read(land);
    assert.equal(f.selection, 0);
    const top = hit(f, 2000 + f.selection);
    assert.ok(top.y >= rows.y - 1, `selected row in view at the top (${top.y} vs ${rows.y})`);
    const sampling = land.evaluate(async () => {
      const seen = new Set();
      const t0 = performance.now();
      while (performance.now() - t0 < 2500) {
        await new Promise(requestAnimationFrame);
        seen.add(JSON.parse(document.querySelector('#ui-state').textContent).front.kinetic.rows[0].toFixed(2));
      }
      return [...seen];
    });
    for (let i = 0; i < f.choices.length - 1; i++) await land.keyboard.press('ArrowDown');
    const glide = await sampling;
    await settled(land);
    f = await read(land);
    assert.equal(f.selection, f.choices.length - 1);
    const last = hit(f, 2000 + f.selection);
    assert.ok(last.y + last.height <= rows.y + rows.height + 1, `selected row in view at the bottom`);
    assert.ok(glide.length >= 4, `keyboard reveal glides through intermediate offsets (${glide})`);
    await land.close();
  } finally {
    await browser.close();
  }
}
assertNoErrors(errors);
for (const line of report) console.log(line);
console.log(
  'PASS kinetic menu scrolling: tracking, momentum, rubber band, catch, tap vs drag, rows column, keyboard glide'
);
