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
    let releaseContent;
    const contentGate = new Promise(resolve => { releaseContent = resolve; });
    await page.route('**/content/manifest.json', async route => {
      await contentGate;
      await route.continue();
    });
    let release;
    const soundGate = new Promise(resolve => { release = resolve; });
    await page.route(/\/content\/[a-f0-9]{64}\.bin$/, async route => {
      if (bankUrls.has(new URL(route.request().url()).pathname)) await soundGate;
      await route.continue();
    });
    await page.addInitScript(() => {
      const contexts = new Set();
      const Context = window.AudioContext || window.webkitAudioContext;
      const resume = Context.prototype.resume;
      Context.prototype.resume = function (...args) { contexts.add(this); return resume.apply(this, args); };
      window.__audioContexts = contexts;
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
    await page.goto(url('/engine.html?venue=env01&audioDiagnostics=1&uiDiagnostics=1'));
    const state = () => page.evaluate(() => JSON.parse(document.documentElement.dataset.engineAudio));
    const press = async key => {
      await page.keyboard.down(key);
      await page.waitForTimeout(100);
      await page.keyboard.up(key);
    };
    // Actual canvas focus/blur before Ready: a late content load must not
    // restore focus, begin simulation or start a late sound bank.
    await page.locator('canvas').waitFor({ state: 'visible' });
    await page.locator('canvas').focus();
    await page.waitForTimeout(100);
    await page.evaluate(() => document.querySelector('canvas').blur());
    await page.waitForTimeout(100);
    releaseContent();
    // An unfocused ride draws its first frame and then sleeps (no fps meter).
    await page.waitForFunction(() => document.querySelector('#ui-state'), null, { timeout: 120000 });
    assert.equal(await page.evaluate(() => document.documentElement.dataset.engineAudio), undefined,
      'ride renders while sound-bank network reads are blocked');
    const beforeSound = await page.locator('#ui-state').textContent().then(JSON.parse);
    assert.equal(beforeSound.active, false, 'pre-ready blur survives content loading');
    assert.equal(beforeSound.step, 0, 'pre-ready blur prevents simulation starting');
    release();
    await page.waitForFunction(() => document.documentElement.dataset.engineAudio, null, { timeout: 120000 });
    const loadedAway = await state();
    assert.ok(!loadedAway.active && !loadedAway.musicActive, 'late sound install keeps inactive lifecycle');
    await page.waitForTimeout(350);
    assert.equal((await state()).steps, loadedAway.steps, 'sound readiness cannot wake an unfocused ride');
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
    await page.evaluate(() => Promise.all([...window.__audioContexts].map(context => context.suspend())));
    assert.ok(await page.evaluate(() => [...window.__audioContexts].every(context => context.state === 'suspended')),
      'fixture suspends the previously activated audio context while blurred');
    await page.evaluate(() => document.querySelector('canvas').focus());
    await page.waitForFunction(() => [...window.__audioContexts].every(context => context.state === 'running'));
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
    assert.ok(resumed.clock > away.clock, 'focus recovery advances the previously suspended mixer');
    await page.evaluate(() => document.querySelector('canvas').blur());
    await page.waitForFunction(() => !JSON.parse(document.documentElement.dataset.engineAudio).musicActive);
    const unfocused = await state();
    await page.waitForTimeout(350);
    assert.equal((await state()).steps, unfocused.steps, 'blur also freezes an unpaused ride');
    await page.evaluate(() => document.querySelector('canvas').focus());
    await page.waitForFunction(steps => JSON.parse(document.documentElement.dataset.engineAudio).steps > steps + 5, unfocused.steps);
    // Headless browsers do not provide a portable OS tab/background action.
    // Control the DOM visibilityState getter and emit the genuine event that
    // winit maps to Occluded; this is a fixture, not physical app-hide evidence.
    const visibility = async value => page.evaluate(value => {
      Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => value });
      document.dispatchEvent(new Event('visibilitychange'));
    }, value);
    await visibility('hidden');
    await page.waitForFunction(() => !JSON.parse(document.documentElement.dataset.engineAudio).musicActive);
    const hidden = await state();
    await page.waitForTimeout(350);
    const hiddenLater = await state();
    assert.equal(hiddenLater.steps, hidden.steps, 'visibility loss freezes an unpaused ride');
    assert.ok(hiddenLater.musicPaused, 'visibility loss pauses music');
    assert.ok(Math.abs(hiddenLater.clock - hidden.clock) < 0.05, 'hidden mixer clock freezes');
    await page.evaluate(() => document.querySelector('canvas').blur());
    await visibility('visible');
    await page.waitForTimeout(350);
    const visibleBlurred = await state();
    assert.ok(!visibleBlurred.active && !visibleBlurred.musicActive, 'visibility regain cannot undo canvas blur');
    assert.equal(visibleBlurred.steps, hidden.steps);
    await page.locator('canvas').focus();
    await page.waitForFunction(steps => {
      const s = JSON.parse(document.documentElement.dataset.engineAudio);
      return s.active && s.musicActive && !s.musicPaused && s.steps > steps + 5;
    }, hidden.steps);
    const visibleFocused = await state();
    // Controlled persisted page transitions cover winit Suspended/Resumed.
    // Visibility events are ignored by winit while suspended; pageshow must
    // refresh visibility without overriding an independently blurred canvas.
    await visibility('hidden');
    await page.waitForFunction(() => !JSON.parse(document.documentElement.dataset.engineAudio).musicActive);
    await page.evaluate(() => window.dispatchEvent(new PageTransitionEvent('pagehide', { persisted: true })));
    await visibility('visible');
    await page.evaluate(() => window.dispatchEvent(new PageTransitionEvent('pageshow', { persisted: true })));
    await page.waitForFunction(steps => {
      const s = JSON.parse(document.documentElement.dataset.engineAudio);
      return s.active && s.musicActive && s.steps > steps + 5;
    }, visibleFocused.steps);
    const pageRestored = await state();
    await page.evaluate(() => { delete document.visibilityState; });
    assertNoErrors(errors);
    await mkdir('private/reports/engine-parity/audio-recovery', { recursive: true });
    await page.screenshot({ path: `private/reports/engine-parity/audio-recovery/${engine}.png` });
    evidence.push({ engine, beforeSound, loadedAway, driving, paused, menu, blurred, away, resumed, hidden, hiddenLater, visibleBlurred, visibleFocused, pageRestored, visibilityFixture: 'controlled document.visibilityState' });
    console.log(`PASS: ${engine} Rust engine sound, loading/focus recovery and controlled visibility lifecycle`);
  } finally {
    await browser.close();
  }
}
await writeFile('private/reports/engine-parity/audio-recovery/lifecycle.json', JSON.stringify(evidence, null, 2));
