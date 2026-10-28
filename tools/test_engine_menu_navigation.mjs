// Menu navigation (research/MENU_FLOW.md). The original keeps a page history:
// leaving a page pushes it with its focused row (`0025d760`), every page's
// Triangle pops it (`0025d8b0`) and the page restores that row (`0025d430`).
// From the title this crawls every menu page reachable without starting a
// ride, opening each from its row and leaving it with Back, and asserts
// that Back returns to the page it came from with the same row selected.
// It runs with the keyboard (Enter / Escape) and a standard gamepad
// (A / B) on desktop, and with the keyboard on phone portrait and phone
// landscape. Back on the title does nothing; Backspace is Back too.
// Host Chromium with a stand-in standard gamepad; ATV_MENU_NAV_SCREENSHOTS
// (an ignored directory) keeps one screenshot of each page per viewport.
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { assertNoErrors, launch, newPage, nextFrames, unlockStore, url } from './browser/harness.mjs';

// Rows that open another menu page. Everything else (rides, links, value
// rows, purchases, captures, form submits) is left alone.
const NAV = new Set([
  'main',
  'single',
  'multiplayer',
  'profile',
  'halloffame',
  'settings',
  'jukebox',
  'split',
  'splitfree',
  'splitrace',
  'create-lobby',
  'join-lobby',
  'hosttrack',
  'tracks',
  'race',
  'championship',
  'games',
  'objective-courses',
  'garage',
  'gear',
  'profile-create',
  'profile-load',
  'profile-copy',
  'profile-delete',
  'profile-view',
  'profile-store',
  'view-overview',
  'view-trophies',
  'view-records',
  'view-atvs',
  'view-gear',
  'view-games',
  'store-atv',
  'store-gear',
  'store-games',
  'store-tracks',
  'store-cheats',
  // Profile Editor → Pass Key & Saves (research/CLOUD_SAVE.md).
  'passkey',
  'enterkey',
  'entershare',
  'savefiles',
  // Options (the shared settings menu) and its pages.
  'game-settings',
  'controls',
  'hud-settings',
  'music',
  'extras',
  'alpha',
  'minimap-config',
  'race-overlays',
  'keyboard',
  'controller',
  'padbindings'
]);
// Pages whose rows are explored further. The others are only opened and left:
// their arrows step courses, vehicles or values, or their rows ride.
const EXPLORE = new Set([
  'title',
  'main',
  'single',
  'multiplayer',
  'split',
  'create-lobby',
  'join-lobby',
  'garage',
  'profile',
  'profile-view',
  'store',
  'passkey',
  'menu:settings',
  'menu:controls',
  'menu:hud-settings',
  'menu:controller-settings'
]);

const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
async function where(page) {
  const f = (await read(page)).front;
  if (f?.menu) {
    const m = f.menu;
    return { at: `menu:${m.page}`, choices: m.choices, selection: m.selection, history: m.history, front: f };
  }
  return { at: f?.page, choices: f?.choices || [], selection: f?.selection, history: f?.history, front: f };
}
const selected = s => s.choices[s.selection];

function inputs(page, kind) {
  const tap = async code => {
    await page.keyboard.down(code);
    await nextFrames(page, 2);
    await page.keyboard.up(code);
    await nextFrames(page, 3);
  };
  const press = async index => {
    await page.evaluate(i => window.padButton(i, true), index);
    await nextFrames(page, 3);
    await page.evaluate(i => window.padButton(i, false), index);
    await nextFrames(page, 3);
  };
  return kind === 'keyboard'
    ? { down: () => tap('ArrowDown'), accept: () => tap('Enter'), back: () => tap('Escape') }
    : { down: () => press(13), accept: () => press(0), back: () => press(1) };
}

// Down (which wraps) until the row is selected: Up from a form's first row
// would focus its text fields instead.
async function select(page, io, action) {
  for (let i = 0; i < 40; i++) {
    const s = await where(page);
    if (selected(s) === action) return;
    await io.down();
  }
  assert.fail(`could not select ${action} on ${(await where(page)).at}`);
}

async function settle(page, from) {
  for (let i = 0; i < 30; i++) {
    const s = await where(page);
    if (s.at !== from) return s;
    await nextFrames(page, 2);
  }
  return where(page);
}

async function crawl(page, io, report, shots) {
  const visited = new Set();
  const edges = [];
  async function visit(depth) {
    const here = await where(page);
    visited.add(here.at);
    if (shots && !report.shots.has(here.at)) {
      report.shots.add(here.at);
      await page.waitForTimeout(450); // Time-based on purpose: let the slide-in finish for the screenshot.
      await page.screenshot({ path: `${shots}/${report.name}-${here.at.replace(':', '-')}.png` });
    }
    if (!EXPLORE.has(here.at) || depth > 8) return;
    for (const action of here.choices.filter(a => NAV.has(a))) {
      // A page reached again (e.g. the Garage from Host Game) is only
      // left with Back, not explored twice.
      await select(page, io, action);
      const before = await where(page);
      await io.accept();
      const opened = await settle(page, here.at);
      if (opened.at === here.at) {
        // Refused (e.g. a notice that a profile is needed): nothing opened.
        report.stayed.push(`${here.at} ${action}: ${opened.front.notice || 'no page'}`);
        continue;
      }
      assert.notEqual(opened.at, 'loading', `${here.at} → ${action} started a ride`);
      edges.push(`${here.at} → ${opened.at}`);
      if (!visited.has(opened.at)) await visit(depth + 1);
      else visited.add(opened.at);
      // Back from wherever the page's exploration ended (the page itself).
      const leaving = await where(page);
      assert.equal(leaving.at, opened.at, `exploring ${opened.at} came back to it`);
      await io.back();
      const after = await settle(page, opened.at);
      assert.equal(after.at, here.at, `Back from ${opened.at} returned to ${after.at}, not ${here.at}`);
      assert.equal(
        selected(after),
        selected(before),
        `Back from ${opened.at} to ${here.at} selected ${selected(after)}, not ${selected(before)}`
      );
      report.checked++;
    }
  }
  await visit(0);
  return { visited: [...visited], edges };
}

async function run(browser, { name, viewport, kind, shots }) {
  const errors = [];
  const page = await newPage(browser, { viewport, serviceWorkers: 'block' }, errors);
  // Profile pages and Single Player picks need a loaded profile.
  await unlockStore(page);
  await page.addInitScript(() => {
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    const buttons = Array.from({ length: 17 }, () => ({ pressed: false, touched: false, value: 0 }));
    const pad = {
      id: 'Menu stand-in (STANDARD GAMEPAD)',
      index: 0,
      connected: true,
      mapping: 'standard',
      timestamp: 1,
      axes: [0, 0, 0, 0],
      buttons
    };
    window.padButton = (index, down) => {
      buttons[index] = { pressed: down, touched: down, value: down ? 1 : 0 };
      pad.timestamp++;
    };
    navigator.getGamepads = () => [pad];
  });
  await page.goto(url('/play.html?uiDiagnostics=1'));
  await page.waitForFunction(
    () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === 'title',
    null,
    {
      timeout: 120000
    }
  );
  // Single Player's rows wait for the content's race and game lists.
  await page.waitForFunction(
    () => {
      const f = JSON.parse(document.querySelector('#ui-state').textContent).front;
      return f.store?.courses > 0 && f.race?.courses > 0;
    },
    null,
    { timeout: 120000 }
  );
  await page.mouse.click(8, 8);
  const io = inputs(page, kind);
  // Back on the title stays on the title.
  await io.back();
  let s = await where(page);
  assert.equal(s.at, 'title', 'Back on the title');
  const report = { name, checked: 0, stayed: [], shots: new Set() };
  const { visited, edges } = await crawl(page, io, report, shots);
  s = await where(page);
  assert.equal(s.at, 'title', 'the crawl ends where it began');
  assert.ok(visited.includes('menu:settings'), `Options reached: ${visited}`);
  assert.ok(report.checked >= 25, `only ${report.checked} Back checks`);
  if (kind === 'keyboard') {
    // Backspace is Back on the frontend pages too.
    await io.accept();
    await settle(page, 'title');
    await select(page, io, 'single');
    await io.accept();
    await settle(page, 'main');
    await page.keyboard.press('Backspace');
    s = await settle(page, 'single');
    assert.deepEqual([s.at, selected(s)], ['main', 'single'], 'Backspace from Single Player');
  }
  assertNoErrors(errors);
  await page.close();
  console.log(`  ${name}: ${report.checked} Back checks over ${visited.length} pages (${edges.length} edges)`);
  if (process.env.ATV_MENU_NAV_VERBOSE)
    console.log(`    ${edges.join('\n    ')}\n    stayed: ${report.stayed.join('; ')}`);
  return visited;
}

const shots = process.env.ATV_MENU_NAV_SCREENSHOTS;
if (shots) await mkdir(shots, { recursive: true });
const browser = await launch({ args: ['--autoplay-policy=no-user-gesture-required'] });
try {
  const desktop = await run(browser, {
    name: 'desktop-keyboard',
    viewport: { width: 1280, height: 720 },
    kind: 'keyboard',
    shots
  });
  await run(browser, { name: 'desktop-gamepad', viewport: { width: 1280, height: 720 }, kind: 'gamepad' });
  const portrait = await run(browser, {
    name: 'phone-portrait',
    viewport: { width: 390, height: 844 },
    kind: 'keyboard',
    shots
  });
  const landscape = await run(browser, {
    name: 'phone-landscape',
    viewport: { width: 844, height: 390 },
    kind: 'keyboard',
    shots
  });
  assert.deepEqual([...portrait].sort(), [...desktop].sort(), 'phone portrait reaches the same pages');
  assert.deepEqual([...landscape].sort(), [...desktop].sort(), 'phone landscape reaches the same pages');
  console.log('PASS: Back returns to the page and row it came from across the menus (keyboard and gamepad)');
} finally {
  await browser.close();
}
