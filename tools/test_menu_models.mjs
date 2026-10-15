// Real-WASM original-content UI gate. Reports/screenshots remain private.
// Scrollable garage lists are measured by their visible area; full size still sets the 44px target.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { assertNoErrors, base, button as harnessButton, launch, newPage, nextFrames } from './browser/harness.mjs';
const output = process.env.ATV_MENU_SCREENSHOTS;
const browser = await launch();
const errors = [],
  page = await newPage(browser, { viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true }, errors);
page.on('response', r => {
  if (r.status() >= 400) errors.push(`${r.status()} ${r.url()}`);
});
const button = name => harnessButton(page, name);
const ready = key =>
  page.waitForFunction(
    key =>
      [...document.querySelectorAll('.model-view canvas')].length === 2 &&
      [...document.querySelectorAll('.model-view canvas')].every(
        c => c.dataset.ready === 'true' && c.dataset.selection === key
      ),
    key,
    { timeout: 90000 }
  );
const shot = async name => {
  if (output) await page.screenshot({ path: `${output}-${name}.png` });
};
const report = { viewports: [], models: [], errors };
try {
  await page.goto(new URL('/play.html', base).href);
  await button('Press START').click({ timeout: 120000 });
  await page.waitForFunction(() => window.atvTerrainStatus.audio.menuMusic.voices > 0);
  assert.equal(await page.evaluate(() => window.atvTerrainStatus.simulation), false);
  const music = await page.evaluate(() => window.atvTerrainStatus.audio);
  assert.equal(music.music.songs, 21);
  assert.equal(music.voices, 0);
  assert.equal(music.commands, 0);
  assert.equal(music.music.failed, false);
  assert.equal(music.menuMusic.stereo, true);
  assert.equal(music.menuMusic.track, 'UI/uitune1');
  report.menuMusic = music.menuMusic;
  await button('Single Player').click();
  await button('Quad and Rider').click();
  await ready('1:0');
  await page.waitForFunction(() =>
    document.querySelectorAll('.model-view canvas')[1]?.dataset.clip?.startsWith('mrider')
  );
  for (const viewport of [
    { width: 390, height: 844 },
    { width: 320, height: 568 },
    { width: 844, height: 390 },
    { width: 740, height: 360 },
    { width: 1280, height: 720 },
    { width: 768, height: 1024 }
  ]) {
    await page.setViewportSize(viewport);
    await page.waitForFunction(({ width, height }) => {
      const c = document.querySelector('#menu-canvas');
      return c.width / c.height > width / height - 0.01 && c.width / c.height < width / height + 0.01;
    }, viewport);
    const bounds = await page.evaluate(() => ({
      views: [...document.querySelectorAll('.model-view canvas')].map(c => ({
        ...c.getBoundingClientRect().toJSON(),
        triangles: Number(c.dataset.triangles),
        gl: c.dataset.glError
      })),
      buttons: [...document.querySelectorAll('#menu-buttons button')].flatMap(b => {
        const r = b.getBoundingClientRect(),
          c = b.closest('.ms-scroll')?.getBoundingClientRect();
        if (!c) return [{ ...r.toJSON(), label: b.getAttribute('aria-label') }];
        const x = Math.max(r.left, c.left),
          y = Math.max(r.top, c.top),
          right = Math.min(r.right, c.right),
          bottom = Math.min(r.bottom, c.bottom);
        return right <= x || bottom <= y
          ? []
          : [{ x, y, right, bottom, width: r.width, height: r.height, label: b.getAttribute('aria-label') }];
      }),
      overflow: document.documentElement.scrollWidth > innerWidth
    }));
    assert.equal(bounds.overflow, false);
    for (const b of [...bounds.views, ...bounds.buttons]) {
      assert.ok(
        b.x >= 0 && b.y >= 0 && b.right <= viewport.width + 1 && b.bottom <= viewport.height + 1,
        JSON.stringify(b)
      );
    }
    for (const b of bounds.buttons) {
      assert.ok(b.height >= 44 && b.width >= 44, `${viewport.width}: ${b.label}`);
      for (const v of bounds.views)
        assert.ok(
          b.right <= v.x || b.x >= v.right || b.bottom <= v.y || b.y >= v.bottom,
          'Controls must not cover models'
        );
    }
    for (const v of bounds.views) {
      assert.ok(v.width > 70 && v.height > 90);
      assert.ok(v.triangles > 1000);
      assert.equal(v.gl, '0');
    }
    report.viewports.push({ viewport, bounds });
    await shot(`${viewport.width}x${viewport.height}`);
  }
  await page.waitForFunction(() =>
    document.querySelectorAll('.model-view canvas')[1]?.dataset.clip?.includes('mrider_body')
  );
  const rider = page.locator('.model-view canvas').last();
  const idleStart = await rider.screenshot();
  // Time-based on purpose: the menu rider idle animation advances on the wall clock.
  await page.waitForTimeout(650);
  assert.notDeepEqual(await rider.screenshot(), idleStart, 'Original rider motion must change rendered pixels');
  // Touch/pointer orbit and keyboard orbit redraw actual framebuffer pixels.
  await page.setViewportSize({ width: 390, height: 844 });
  await nextFrames(page, 9);
  const canvas = page.locator('.model-view canvas').first();
  const before = await canvas.screenshot();
  const box = await canvas.boundingBox();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 + 55, box.y + box.height / 2);
  await page.mouse.up();
  assert.notDeepEqual(await canvas.screenshot(), before);
  await canvas.focus();
  const rotated = await canvas.screenshot();
  await page.keyboard.press('ArrowRight');
  assert.notDeepEqual(await canvas.screenshot(), rotated);
  // Keyboard menu navigation selects the rider row; controller navigation works
  // with the same Rust choices (release after every simulated button press).
  await page.mouse.click(10, 80);
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('Enter');
  await ready('1:1');
  await page.waitForFunction(() =>
    document.querySelectorAll('.model-view canvas')[1]?.dataset.clip?.startsWith('frider')
  );
  await shot('female-portrait');
  const pad = async index => {
    await page.evaluate(index => {
      window.__menuPad = {
        connected: true,
        axes: [0, 0, 0, 0],
        buttons: Array.from({ length: 17 }, (_, i) => ({ pressed: i === index, value: i === index ? 1 : 0 }))
      };
      Object.defineProperty(navigator, 'getGamepads', { configurable: true, value: () => [window.__menuPad] });
    }, index);
    await nextFrames(page, 3);
    await page.evaluate(() =>
      window.__menuPad.buttons.forEach(b => {
        b.pressed = false;
        b.value = 0;
      })
    );
    await nextFrames(page, 3);
  };
  await pad(15);
  await ready('2:1');
  await pad(14);
  await ready('1:1');
  // All original quads and both original bind rigs are GPU drawn via real UI.
  for (let quad = 1; quad <= 26; quad++) {
    if (quad > 1) await button('Next quad').click();
    await ready(`${quad}:1`);
    const views = await page
      .locator('.model-view canvas')
      .evaluateAll(cs =>
        cs.map(c => ({ key: c.dataset.selection, triangles: Number(c.dataset.triangles), gl: c.dataset.glError }))
      );
    assert.ok(views.every(v => v.triangles > 1000 && v.gl === '0'));
    report.models.push({ quad, views });
  }
  await page.waitForFunction(() =>
    document.querySelectorAll('.model-view canvas')[1]?.dataset.clip?.startsWith('frider')
  );
  await shot('quad26-female');
  for (let i = 0; i < 21; i++) await button('Next quad').click();
  await ready('1:1');
  // Rapid changes cannot show a late response for an old selected model.
  for (let i = 0; i < 5; i++) await button('Next quad').click();
  await ready('6:1');
  const view = page.locator('.model-view canvas').first();
  await view.evaluate(c => {
    c.__loss = c.getContext('webgl2').getExtension('WEBGL_lose_context');
    c.__loss.loseContext();
  });
  await page.waitForFunction(() => document.querySelector('.model-view canvas').dataset.ready === undefined);
  await view.evaluate(c => c.__loss.restoreContext());
  await ready('6:1');
  await button('Done').click();
  assert.equal(await page.locator('.model-view').count(), 0);
  await button('Freestyle').click();
  await button('Ride Caribou Peak').click();
  await page.waitForFunction(() => document.querySelector('#frontend').dataset.page === 'loading');
  assert.equal(await page.locator('#menu-buttons button').count(), 0);
  await shot('loading-portrait');
  await page.setViewportSize({ width: 844, height: 390 });
  await nextFrames(page, 6);
  await shot('loading-landscape');
  await page.waitForFunction(
    () => document.querySelector('#frontend').hidden && window.atvTerrainStatus.simulation,
    null,
    { timeout: 120000 }
  );
  const driven = await page.evaluate(() => window.atvTerrainStatus);
  assert.equal(driven.playerModel.quad_id, 6);
  assert.equal(driven.playerModel.rider_id, 1);
  assert.equal(driven.glError, 0);
  await page.keyboard.press('Escape');
  await button('Resume').waitFor();
  // Pause keeps the racing soundtrack playing (PR #40); gameplay voices stay silent.
  await page.waitForFunction(() => window.atvTerrainStatus.audio.musicPreview);
  const paused = await page.evaluate(() => window.atvTerrainStatus.audio);
  assert.equal(paused.voices, 0);
  await button('Quit to Main Menu').click();
  await page.waitForFunction(
    () => window.atvTerrainStatus.audio.musicPreview && window.atvTerrainStatus.audio.context === 'running'
  );
  assertNoErrors(errors);
  report.passed = true;
  console.log(JSON.stringify(report));
  if (process.env.ATV_MENU_REPORT) fs.writeFileSync(process.env.ATV_MENU_REPORT, JSON.stringify(report, null, 2));
} finally {
  await browser.close();
}
