// Shared-engine sound and soundtrack lifecycle with actual hosted resources.
// Diagnostics are opt-in; regular gameplay does not serialize this state.
import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';
import { assertNoErrors, launch, newPage, url } from './browser/harness.mjs';

const evidence = [];
for (const engine of ['chromium', 'webkit']) {
  const browser = await launch({ engine });
  try {
    const errors = [];
    const page = await newPage(browser, { viewport: { width: 960, height: 540 } }, errors);
    const manifest = await (await page.request.get(url('/content/manifest.json'))).json();
    const bank = manifest.files.find(f => f.path === '/AUDIO/AUDIO.RES');
    assert.ok(bank, 'hosted sound bank exists');
    const bankUrls = new Set((bank.chunks ?? [bank]).map(c => `/content/${c.sha256}.bin`));
    let release;
    const soundGate = new Promise(resolve => { release = resolve; });
    await page.route(/\/content\/[a-f0-9]{64}\.bin$/, async route => {
      if (bankUrls.has(new URL(route.request().url()).pathname)) await soundGate;
      await route.continue();
    });
    await page.addInitScript(() => {
      const play = HTMLMediaElement.prototype.play;
      let first = true;
      HTMLMediaElement.prototype.play = function (...args) {
        if (first) {
          first = false;
          return Promise.reject(new DOMException('Test interrupted activation', 'NotAllowedError'));
        }
        return play.apply(this, args);
      };
    });
    await page.goto(url('/engine.html?venue=env01&audioDiagnostics=1'));
    const state = () => page.evaluate(() => JSON.parse(document.documentElement.dataset.engineAudio));
    const press = async key => {
      await page.keyboard.down(key);
      await page.waitForTimeout(100);
      await page.keyboard.up(key);
    };
    await page.waitForFunction(() => /\d+ fps/.test(document.querySelector('#status').textContent), null, { timeout: 120000 });
    assert.equal(await page.evaluate(() => document.documentElement.dataset.engineAudio), undefined,
      'ride renders while sound-bank network reads are blocked');
    release();
    await page.waitForFunction(() => document.documentElement.dataset.engineAudio, null, { timeout: 120000 });
    await page.locator('canvas').click();
    await page.keyboard.down('KeyW');
    await page.waitForFunction(() => {
      const s = JSON.parse(document.documentElement.dataset.engineAudio);
      return s.started && s.engineCommands >= 1 && s.voices > 0 && s.musicTime > 0.25 && !s.musicPaused;
    }, null, { timeout: 30000 }).catch(async error => {
      console.error(engine, await state(), errors);
      throw error;
    });
    await page.keyboard.up('KeyW');
    const driving = await state();
    await press('Escape');
    await page.waitForFunction(() => !JSON.parse(document.documentElement.dataset.engineAudio).active);
    const paused = await state();
    await page.waitForTimeout(750);
    const menu = await state();
    assert.equal(menu.steps, paused.steps, 'pause freezes simulation');
    assert.ok(menu.musicActive && !menu.musicPaused, 'soundtrack continues in pause');
    assert.ok(menu.musicTime > paused.musicTime + 0.2, 'paused music advances');
    await page.evaluate(() => document.querySelector('canvas').blur());
    await page.waitForFunction(() => !JSON.parse(document.documentElement.dataset.engineAudio).musicActive);
    const blurred = await state();
    await page.waitForTimeout(350);
    const away = await state();
    assert.equal(away.steps, blurred.steps, 'blur preserves paused simulation');
    assert.ok(away.musicPaused, 'blur pauses soundtrack');
    assert.ok(Math.abs(away.musicTime - blurred.musicTime) < 0.05, 'hidden song time freezes');
    assert.ok(Math.abs(away.clock - blurred.clock) < 0.05, 'hidden mixer clock freezes');
    await page.evaluate(() => document.querySelector('canvas').focus());
    await page.waitForFunction(() => JSON.parse(document.documentElement.dataset.engineAudio).musicActive);
    await press('Escape');
    await page.waitForFunction(steps => {
      const s = JSON.parse(document.documentElement.dataset.engineAudio);
      return s.active && s.musicActive && !s.musicPaused && s.steps > steps + 5;
    }, away.steps).catch(async error => {
      console.error(engine, 'resume', await state(), errors);
      throw error;
    });
    const resumed = await state();
    await page.evaluate(() => document.querySelector('canvas').blur());
    await page.waitForFunction(() => !JSON.parse(document.documentElement.dataset.engineAudio).musicActive);
    const unfocused = await state();
    await page.waitForTimeout(350);
    assert.equal((await state()).steps, unfocused.steps, 'blur also freezes an unpaused ride');
    await page.evaluate(() => document.querySelector('canvas').focus());
    await page.waitForFunction(steps => JSON.parse(document.documentElement.dataset.engineAudio).steps > steps + 5, unfocused.steps);
    assertNoErrors(errors);
    await mkdir('private/reports/engine-parity/audio-recovery', { recursive: true });
    await page.screenshot({ path: `private/reports/engine-parity/audio-recovery/${engine}.png` });
    evidence.push({ engine, driving, paused, menu, blurred, away, resumed });
    console.log(`PASS: ${engine} Rust engine sound, paused soundtrack and focus recovery`);
  } finally {
    await browser.close();
  }
}
await writeFile('private/reports/engine-parity/audio-recovery/lifecycle.json', JSON.stringify(evidence, null, 2));
