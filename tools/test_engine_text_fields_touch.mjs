// Every Host/Join text field takes a phone's tap: the page cancels native touch
// gestures (engine.js) but not on the fields, so a tap focuses the <input>
// (opening the soft keyboard) and typed text reaches the Rust form. Runs in
// Chromium and WebKit (iOS Safari's engine) with touch emulation.
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4173 node tools/test_engine_text_fields_touch.mjs
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, url } from './browser/harness.mjs';

const errors = [];
const report = {};
const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front);
const onPage = (page, name) =>
  page.waitForFunction(
    name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name,
    name,
    { timeout: 90000 }
  );
async function key(page, code) {
  await page.keyboard.press(code);
  await page.waitForTimeout(150);
}
async function choose(page, action) {
  const f = await read(page);
  const at = f.choices.indexOf(action);
  assert.ok(at >= 0, `${action} in ${f.choices}`);
  for (let i = 0; i < at; i++) await key(page, 'ArrowDown');
  await key(page, 'Enter');
}
async function tapAndType(page, label, text) {
  const input = page.getByLabel(label, { exact: true });
  const box = await input.boundingBox();
  assert.ok(box, `${label} input is laid out`);
  await page.touchscreen.tap(box.x + box.width / 2, box.y + box.height / 2);
  await page.waitForFunction(label => document.activeElement?.getAttribute('aria-label') === label, label, {
    timeout: 5000
  });
  // Edit what is there (the name starts as the profile's), then type.
  for (let i = (await input.inputValue()).length; i > 0; i--) await page.keyboard.press('Backspace');
  await page.keyboard.type(text);
  assert.equal(await input.inputValue(), text);
}

for (const engine of ['chromium', 'webkit']) {
  const browser = await launch({ engine });
  try {
    const page = await newPage(
      browser,
      { viewport: { width: 844, height: 390 }, hasTouch: true, isMobile: engine === 'chromium' },
      errors
    );
    await page.addInitScript(() => {
      localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
      Object.defineProperty(document, 'hasFocus', { value: () => true });
    });
    await page.goto(url('/play.html?uiDiagnostics=1'));
    await onPage(page, 'title');
    await page.mouse.click(8, 8);
    await key(page, 'Enter');
    await onPage(page, 'main');
    await choose(page, 'multiplayer');
    await onPage(page, 'multiplayer');
    // Host Game: the name and the optional custom code.
    await choose(page, 'create-lobby');
    await onPage(page, 'create-lobby');
    await tapAndType(page, 'Rider name', 'Hostess');
    await tapAndType(page, 'Lobby code (8 letters/numbers, optional)', 'host2345');
    await page.waitForFunction(
      () => /"createCode":"HOST2345"/.test(document.querySelector('#ui-state')?.textContent ?? ''),
      null,
      { timeout: 5000 }
    );
    await key(page, 'Escape');
    await onPage(page, 'multiplayer');
    // Join Game: the name and the code.
    await choose(page, 'join-lobby');
    await onPage(page, 'join-lobby');
    await tapAndType(page, 'Rider name', 'Thumbs');
    await tapAndType(page, 'Lobby code', 'abc12345');
    await page.waitForFunction(
      () => {
        const text = document.querySelector('#ui-state')?.textContent ?? '';
        return /"roomCode":"ABC12345"/.test(text) && /"playerName":"Thumbs"/.test(text);
      },
      null,
      { timeout: 5000 }
    );
    report[engine] = 'tap focused all Host and Join fields; the forms have the name and codes';
  } finally {
    await browser.close();
  }
}
assertNoErrors(errors);
console.log(JSON.stringify(report, null, 2));
