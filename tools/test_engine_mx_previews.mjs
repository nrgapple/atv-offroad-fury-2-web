// Private original MX artwork acceptance through the actual mobile course selector.
import assert from 'node:assert/strict';
import { readFile, mkdir } from 'node:fs/promises';
import { launch, newPage, nextFrames, unlockStore, url, assertNoErrors } from './browser/harness.mjs';
const catalog = JSON.parse(await readFile(new URL('../research/mx-map-catalog.json', import.meta.url)));
const browser = await launch();
const errors = [];
const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front);
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }
async function choose(page, choice) {
  const f = await read(page);
  const index = f.choices.indexOf(choice);
  assert.ok(index >= 0, choice);
  for (let i = f.selection; i < index; i++) await key(page, 'ArrowDown');
  for (let i = f.selection; i > index; i--) await key(page, 'ArrowUp');
  await key(page, 'Enter');
}
try {
  const page = await newPage(browser, { viewport: { width: 390, height: 844 }, hasTouch: true }, errors);
  await unlockStore(page);
  await page.goto(url('/play.html?uiDiagnostics=1'));
  await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === 'title', null, { timeout: 180000 });
  await choose(page, 'main');
  await choose(page, 'single');
  await choose(page, 'tracks');
  await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.photos >= 95, null, { timeout: 180000 });
  const ids = new Set((await read(page)).photoIds);
  for (const course of catalog) assert.ok(ids.has(course.id), `${course.id}: decoded thumbnail source`);
  assert.ok(ids.has('env01') && ids.has('sx01'), 'ATV2 photos retained');
  const seen = new Set();
  const directory = process.env.ATV_MX_PREVIEW_SCREENSHOTS;
  if (directory) await mkdir(directory, { recursive: true });
  for (let i = 0; i < 96; i++) {
    const f = await read(page);
    await page.waitForFunction(id => {
      const f = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front;
      return f?.photoTextures?.includes(`hero:course:${id}`) && f?.photoTextures?.includes(`thumb:${id}`);
    }, f.track, { timeout: 30000 });
    if (f.track.startsWith('mx_')) {
      seen.add(f.track);
      if (directory) await page.screenshot({ path: `${directory}/${f.track}.png` });
    }
    await key(page, 'ArrowRight');
  }
  assert.equal(seen.size, 56, 'all MX maps selected with thumbnail and hero textures');
  assertNoErrors(errors);
  console.log('PASS: all 56 MX thumbnail/hero mappings through portrait selector and carousel; ATV2 retained');
} finally { await browser.close(); }
