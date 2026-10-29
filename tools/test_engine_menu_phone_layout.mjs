// Phone menu layout (research/RUST_UI.md "Phone menu layout"): at phone
// sizes including the visible area under Safari's toolbars (390x664 portrait,
// 844x330 landscape), iPad and desktop, the menus keep every fixed control on
// screen, rows never run under Back, Single Player shows its modes without
// scrolling, the title keeps Press Start visible, and Single Race's course
// picker opens from its Course row and closes on a pick, by touch, keyboard
// and controller. ATV_PHONE_SHOTS=<dir> saves screenshots.
// Run: node tools/run_browser_tests.mjs --only test_engine_menu_phone_layout.mjs
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { assertNoErrors, launch, newPage, nextFrames, unlockStore, url } from './browser/harness.mjs';

const ROW = 2000;
const COURSE = 2200;
const LIST = 2400;
const BACK = 2401;
const START = 2411;
const SHARE = 2412;
const ROWS = 2450;
const ROWS_END = 2451;
const STAGE = 2414;
const VEHICLE = 2300;
const STEP_PREV = 2430;
const DETAILS = 2415;
const shots = process.env.ATV_PHONE_SHOTS || '';
const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const front = async page => (await read(page)).front;
const onPage = (page, name) =>
  page.waitForFunction(
    name => {
      const f = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front;
      return f?.page === name && !f.motion && !f.exit;
    },
    name,
    { timeout: 120000 }
  );
const hit = (f, id) => f.hits.find(h => h.id === id);
const centre = h => ({ x: h.x + h.width / 2, y: h.y + h.height / 2 });
const overlaps = (a, b) =>
  a.x < b.x + b.width - 0.5 && b.x < a.x + a.width - 0.5 && a.y < b.y + b.height - 0.5 && b.y < a.y + a.height - 0.5;
const inside = (h, { width, height }) =>
  h.x >= -0.5 && h.y >= -0.5 && h.x + h.width <= width + 0.5 && h.y + h.height <= height + 0.5;
async function key(page, code) {
  await page.keyboard.down(code);
  await nextFrames(page, 2);
  await page.keyboard.up(code);
  await nextFrames(page, 4);
}
async function tap(page, h) {
  const c = centre(h);
  await page.mouse.click(c.x, c.y);
  await nextFrames(page, 4);
}
/** Rows, steppers and cards live in scroll panes; Back, Start and the link must be on screen and clear of them. */
function assertFixed(f, label, viewport) {
  const fixed = [BACK, START, SHARE].map(id => hit(f, id)).filter(Boolean);
  for (const h of fixed) assert.ok(inside(h, viewport), `${label}: ${h.id} on screen ${JSON.stringify(h)}`);
  for (const pane of [hit(f, ROWS), hit(f, LIST)].filter(Boolean)) {
    for (const h of fixed) assert.ok(!overlaps(pane, h), `${label}: ${h.id} over the scroll pane ${pane.id}`);
  }
  // Without a pane, every row is on screen and clear of Back.
  if (!hit(f, ROWS)) {
    for (const h of f.hits.filter(h => h.id >= ROW && h.id < ROW + 100)) {
      for (const b of fixed) assert.ok(!overlaps(h, b), `${label}: row ${h.id} under ${b.id}`);
    }
  }
}

/**
 * Portrait: the stage's silver sweeps up at the bottom left (an elliptical
 * corner 0.9 x the width across and 56px tall, from 0.1 x the width left of
 * the stage). Its text table must stay above that curve where it starts.
 */
function assertClearOfRim(f, label, viewport) {
  const [stage, table] = [hit(f, STAGE), hit(f, DETAILS)];
  if (!stage || !table || viewport.width > viewport.height) return;
  const left = stage.x - 0.1 * viewport.width;
  const [rx, ry] = [0.9 * viewport.width, 56];
  const bottom = stage.y + stage.height;
  const curve = x => {
    const dx = Math.max(0, left + rx - x);
    return dx >= rx ? bottom : bottom - ry + ry * Math.sqrt(1 - (dx / rx) ** 2);
  };
  const x = table.x + 8;
  assert.ok(table.y + table.height <= curve(x) + 0.5, `${label}: stage text crosses the rim (${table.y + table.height} > ${curve(x).toFixed(1)})`);
}

if (shots) await mkdir(shots, { recursive: true });
const errors = [];
const sizes = [
  ['safari-portrait', { width: 390, height: 664 }, 3, true],
  ['phone-portrait', { width: 390, height: 844 }, 3, true],
  ['safari-landscape', { width: 844, height: 330 }, 3, true],
  ['phone-landscape', { width: 844, height: 390 }, 3, true],
  ['ipad', { width: 820, height: 1180 }, 2, true],
  ['desktop', { width: 1280, height: 800 }, 1, false]
];
const browser = await launch();
try {
  for (const [name, viewport, dpr, touch] of sizes) {
    const page = await newPage(browser, { viewport, deviceScaleFactor: dpr, hasTouch: touch }, errors);
    await unlockStore(page);
    await page.addInitScript(() => {
      localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
      localStorage.removeItem('atv-player-selection');
    });
    // The title keeps Press Start and its links on screen.
    await page.goto(url('/play.html?uiDiagnostics=1'));
    await onPage(page, 'title');
    let f = await front(page);
    const start = hit(f, ROW + f.choices.indexOf('main'));
    assert.ok(start && inside(start, viewport), `${name}: Press Start on screen ${JSON.stringify(start)}`);
    if (shots) await page.screenshot({ path: `${shots}/${name}-title.png` });

    for (const p of ['main', 'single', 'race', 'tracks', 'multiplayer', 'profile', 'store', 'passkey', 'profile-overview', 'profile-records']) {
      await page.goto(url(`/play.html?uiDiagnostics=1&front=${p}`));
      await onPage(page, p);
      f = await front(page);
      assertFixed(f, `${name} ${p}`, viewport);
      assertClearOfRim(f, `${name} ${p}`, viewport);
      if (p === 'tracks') {
        const list = hit(f, LIST);
        const whole = f.hits.filter(h => h.id >= COURSE && h.id < COURSE + 100 && h.y >= list.y && h.y + h.height <= list.y + list.height);
        assert.ok(whole.length >= 2, `${name}: Select Track shows at least two whole cards (${whole.length})`);
      }
      if (shots) await page.screenshot({ path: `${shots}/${name}-${p}.png` });
    }
    // Single Player: its modes fit without scrolling on every phone.
    await page.goto(url('/play.html?uiDiagnostics=1&front=single'));
    await onPage(page, 'single');
    f = await front(page);
    if (viewport.height > viewport.width) {
      assert.ok(f.kinetic.rows[1] <= 12, `${name}: Single Player modes fit (${f.kinetic.rows[1]}px to scroll)`);
    }

    // Pickers show three whole entries (two on a landscape phone).
    const least = viewport.height <= 400 && viewport.width > viewport.height ? 2 : 3;
    // Single Race: four setup rows; the Course row opens the picker.
    await page.goto(url('/play.html?uiDiagnostics=1&front=race'));
    await onPage(page, 'race');
    f = await front(page);
    assert.equal(f.courseSheet, false);
    assert.ok(!hit(f, LIST), `${name}: no squeezed course list on the setup`);
    const course = f.race.course;
    const row = hit(f, ROW + f.choices.indexOf('racetrack'));
    assert.ok(row && row.height >= 44, `${name}: Course row target ${JSON.stringify(row)}`);
    await tap(page, row);
    f = await front(page);
    assert.equal(f.courseSheet, true, `${name}: a tap on Course opens the picker`);
    assert.equal(f.page, 'race');
    const list = hit(f, LIST);
    assert.ok(list, `${name}: course list`);
    const cards = f.hits.filter(h => h.id >= COURSE && h.id < COURSE + 100 && h.y >= list.y && h.y + h.height <= list.y + list.height);
    assert.ok(cards.length >= least, `${name}: at least ${least} whole cards in the picker (${cards.length})`);
    for (const c of cards) assert.ok(c.height >= 44, `${name}: card target ${JSON.stringify(c)}`);
    assertFixed(f, `${name} picker`, viewport);
    if (shots) await page.screenshot({ path: `${shots}/${name}-race-picker.png` });
    // A tap on another card picks it and closes the picker.
    // The current course leads the list; the last whole card is another.
    const other = cards.at(-1);
    await tap(page, other);
    f = await front(page);
    assert.equal(f.courseSheet, false, `${name}: picking a card closes the picker`);
    assert.notEqual(f.race.course, course, `${name}: the card picked the course`);
    assert.equal(f.choices[f.selection], 'racetrack', 'back on the Course row');
    // Keyboard: Enter opens, Down steps, Escape closes on the page.
    await page.mouse.move(viewport.width - 4, 4);
    await key(page, 'Enter');
    f = await front(page);
    assert.equal(f.courseSheet, true, `${name}: Enter on Course opens the picker`);
    const before = f.race.course;
    await key(page, 'ArrowDown');
    f = await front(page);
    assert.notEqual(f.race.course, before, `${name}: Down steps the course`);
    await key(page, 'Escape');
    f = await front(page);
    assert.deepEqual([f.page, f.courseSheet], ['race', false], `${name}: Escape closes the picker only`);
    // Done (the Back button) closes it by touch.
    await key(page, 'Enter');
    f = await front(page);
    assert.equal(f.courseSheet, true);
    await tap(page, hit(f, BACK));
    f = await front(page);
    assert.deepEqual([f.page, f.courseSheet], ['race', false], `${name}: Done closes the picker`);
    // The garage: the vehicle row opens the vehicle picker; a pick closes it.
    await page.goto(url('/play.html?uiDiagnostics=1&front=garage'));
    await onPage(page, 'garage');
    f = await front(page);
    assert.equal(f.vehicleSheet, false);
    assert.ok(!hit(f, LIST), `${name}: no squeezed vehicle list in the garage`);
    assertFixed(f, `${name} garage`, viewport);
    const quad = f.quad;
    await tap(page, hit(f, ROW + f.choices.indexOf('quadprev')));
    f = await front(page);
    assert.equal(f.vehicleSheet, true, `${name}: a tap on the vehicle opens the picker`);
    const vehicles = hit(f, LIST);
    const items = f.hits.filter(h => h.id >= VEHICLE && h.id < VEHICLE + 100 && h.y >= vehicles.y && h.y + h.height <= vehicles.y + vehicles.height);
    assert.ok(items.length >= least, `${name}: at least ${least} whole vehicles in the picker (${items.length})`);
    if (shots) await page.screenshot({ path: `${shots}/${name}-garage-picker.png` });
    const pick = items.find(h => h.id !== VEHICLE + quad);
    await tap(page, pick);
    f = await front(page);
    assert.deepEqual([f.vehicleSheet, f.quad], [false, pick.id - VEHICLE], `${name}: a pick closes the picker`);
    await key(page, 'Enter');
    assert.equal((await front(page)).vehicleSheet, true, `${name}: Enter on the vehicle opens the picker`);
    await key(page, 'ArrowRight');
    await key(page, 'Escape');
    f = await front(page);
    assert.deepEqual([f.page, f.vehicleSheet], ['garage', false], `${name}: Escape closes the vehicle picker only`);
    // A store item's long label ellipsizes before its value.
    await page.goto(url('/play.html?uiDiagnostics=1&front=store-atv'));
    await onPage(page, 'store-atv');
    f = await front(page);
    const item = f.choices.indexOf('storeitem');
    if (item >= 0) {
      const [labelBox, prev] = [hit(f, ROW + item), hit(f, STEP_PREV + item)];
      assert.ok(labelBox && prev && labelBox.x + labelBox.width <= prev.x + 0.5, `${name}: store label clear of its arrows`);
    }
    await page.goto(url('/play.html?uiDiagnostics=1&front=race'));
    await onPage(page, 'race');
    f = await front(page);
    // The rows column: every setup row reachable (on screen or scrollable).
    const rows = hit(f, ROWS);
    for (const action of ['racetrack', 'racelaps', 'racecpu', 'racedifficulty']) {
      const r = hit(f, ROW + f.choices.indexOf(action));
      assert.ok(r, `${name}: ${action} row`);
      if (rows && !inside(r, viewport)) assert.ok(f.kinetic.rows[1] > 0, `${name}: ${action} scrolls into view`);
    }
    await page.close();
  }

  // A controller: A opens the picker on the Course row, Down steps, B closes.
  const page = await newPage(browser, { viewport: { width: 390, height: 664 }, deviceScaleFactor: 3 }, errors);
  await unlockStore(page);
  await page.addInitScript(() => {
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    const buttons = Array.from({ length: 17 }, () => ({ pressed: false, touched: false, value: 0 }));
    const pad = { id: 'Test pad', index: 0, connected: true, mapping: 'standard', timestamp: 1, axes: [0, 0, 0, 0], buttons };
    window.padButton = (index, down) => {
      buttons[index] = { pressed: down, touched: down, value: down ? 1 : 0 };
      pad.timestamp++;
    };
    navigator.getGamepads = () => [pad];
  });
  await page.goto(url('/play.html?uiDiagnostics=1&front=race'));
  await onPage(page, 'race');
  const press = async index => {
    await page.evaluate(i => window.padButton(i, true), index);
    await nextFrames(page, 4);
    await page.evaluate(i => window.padButton(i, false), index);
    await nextFrames(page, 4);
  };
  let f = await front(page);
  assert.equal(f.choices[f.selection], 'racetrack');
  await press(0);
  f = await front(page);
  assert.equal(f.courseSheet, true, 'controller A opens the picker');
  const before = f.race.course;
  await press(13);
  f = await front(page);
  assert.notEqual(f.race.course, before, 'D-pad Down steps the course');
  await press(1);
  f = await front(page);
  assert.deepEqual([f.page, f.courseSheet], ['race', false], 'controller B closes the picker');
  await page.close();
} finally {
  await browser.close();
}
assertNoErrors(errors);
console.log('PASS phone menu layout and the Single Race course picker');
