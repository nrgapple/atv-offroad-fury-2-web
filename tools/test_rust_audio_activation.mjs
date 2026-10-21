// A sole click during blocked sound loading unlocks the eventual shared output.
// Diagnostics are opt-in; regular gameplay does not serialize this state.
import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';
import { assertNoErrors, launch, newPage, url } from './browser/harness.mjs';

const evidence = [];
for (const engine of ['chromium', 'webkit']) {
  const browser = await launch({
    engine,
    args: engine === 'chromium' ? ['--autoplay-policy=user-gesture-required'] : []
  });
  try {
    for (const gestureKind of ['click', 'touch-release']) {
      const errors = [];
      const page = await newPage(browser, { viewport: { width: 960, height: 540 }, hasTouch: gestureKind === 'touch-release' }, errors);
      const manifest = await (await page.request.get(url('/content/manifest.json'))).json();
      const bank = manifest.files.find(f => f.path === '/AUDIO/AUDIO.RES');
      assert.ok(bank, 'hosted sound bank exists');
      const bankUrls = new Set((bank.chunks ?? [bank]).map(c => `/content/${c.sha256}.bin`));
      let release;
      const soundGate = new Promise(resolve => {
        release = resolve;
      });
      await page.route(/\/content\/[a-f0-9]{64}\.bin$/, async route => {
        if (bankUrls.has(new URL(route.request().url()).pathname)) await soundGate;
        await route.continue();
      });
      await page.addInitScript(({ gestureKind }) => {
        const activated = new WeakSet();
        const contexts = new Set();
        const evidence = (window.__audioActivation = { primed: 0, gestureResumes: 0, deniedUnprimed: 0, deniedTouchStart: 0 });
        let gesture = false;
        for (const type of ['pointerdown', 'pointerup', 'keydown', 'touchend']) {
          window.addEventListener(
            type,
            event => {
              gesture = gestureKind === 'touch-release' ? event.type === 'pointerup' || event.type === 'touchend' : true;
              // Model the activation dispatch task; a microtask may run between
              // the browser's capture listener and winit's canvas listener.
              setTimeout(() => {
                gesture = false;
              }, 0);
            },
            true
          );
        }
        const Context = window.AudioContext || window.webkitAudioContext;
        const resume = Context.prototype.resume;
        Context.prototype.resume = function (...args) {
          contexts.add(this);
          if (gesture) evidence.gestureResumes += 1;
          return resume.apply(this, args);
        };
        window.__audioContextStates = () => [...contexts].map(context => context.state);
        const play = HTMLMediaElement.prototype.play;
        let firstSong = true;
        HTMLMediaElement.prototype.play = function (...args) {
          if (gestureKind === 'touch-release' && !gesture && this.src.startsWith('blob:')) {
            evidence.deniedTouchStart += 1;
            return Promise.reject(new DOMException('Test requires completed touch', 'NotAllowedError'));
          }
          // Require per-element activation even when a desktop browser's policy
          // would otherwise allow later playback after any page interaction.
          if (!gesture && !activated.has(this)) {
            evidence.deniedUnprimed += 1;
            return Promise.reject(new DOMException('Test requires element priming', 'NotAllowedError'));
          }
          if (firstSong && !this.src.startsWith('blob:')) {
            firstSong = false;
            return Promise.reject(new DOMException('Test interrupted activation', 'NotAllowedError'));
          }
          const priming = gesture && !activated.has(this);
          return play.apply(this, args).then(result => {
            if (priming) {
              activated.add(this);
              evidence.primed += 1;
            }
            return result;
          });
        };
      }, { gestureKind });
      await page.goto(url('/engine.html?venue=env01&audioDiagnostics=1'));
      const state = () => page.evaluate(() => JSON.parse(document.documentElement.dataset.engineAudio));
      await page.waitForFunction(() => /\d+ fps/.test(document.querySelector('#status').textContent), null, {
        timeout: 120000
      });
      assert.equal(
        await page.evaluate(() => document.documentElement.dataset.engineAudio),
        undefined,
        'ride renders while sound-bank network reads are blocked'
      );
      if (gestureKind === 'touch-release') {
        await page.touchscreen.tap(480, 270);
      } else {
        await page.locator('canvas').click();
      }
      await page
        .waitForFunction(() => window.__audioActivation.primed === 1 && window.__audioContextStates().includes('running'))
        .catch(async error => {
          console.error(
            engine,
            'early activation',
            await page.evaluate(() => ({ ...window.__audioActivation, contexts: window.__audioContextStates() })),
            errors
          );
          throw error;
        });
      const activation = await page.evaluate(() => window.__audioActivation);
      if (gestureKind === 'touch-release') assert.ok(activation.deniedTouchStart > 0, 'touch start priming was denied before release');
      assert.ok(activation.gestureResumes > 0, 'output resumes inside the early gesture');
      assert.equal(
        await page.evaluate(() => document.documentElement.dataset.engineAudio),
        undefined,
        'the sole activation click happened before bank loading completed'
      );
      release();
      await page.waitForFunction(() => document.documentElement.dataset.engineAudio, null, { timeout: 120000 });
      await page
        .waitForFunction(
          () => {
            const s = JSON.parse(document.documentElement.dataset.engineAudio);
            return s.started && s.engineCommands >= 1 && s.voices > 0 && s.musicTime > 0.25 && !s.musicPaused;
          },
          null,
          { timeout: 30000 }
        )
        .catch(async error => {
          console.error(engine, await state(), errors);
          throw error;
        });
      const earlyReady = await state();
      assert.deepEqual(
        await page.evaluate(() => window.__audioContextStates()),
        ['running'],
        'the loaded ride retains the same single output context unlocked during loading'
      );
      assert.equal(
        await page.evaluate(() => window.__audioActivation.deniedUnprimed),
        0,
        'deferred sound uses the element primed by the only early click'
      );
      assertNoErrors(errors);
      evidence.push({ engine, gestureKind, activation, earlyReady });
      console.log(`PASS: ${engine} ${gestureKind} early-only activation with blocked bank and a rejected first song play`);
      await page.close();
    }
  } finally {
    await browser.close();
  }
}
await mkdir('private/reports/engine-parity/audio-activation', { recursive: true });
await writeFile('private/reports/engine-parity/audio-activation/early-only.json', JSON.stringify(evidence, null, 2));
