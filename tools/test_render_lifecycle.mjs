// Actual single-player lifecycle work counters and wake regression, paired with
// main when ATV_PERF_BASELINE is supplied. No phone-battery claim.
import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';
import { assertNoErrors, base as siteBase, launch, newPage, waitForSteps } from './browser/harness.mjs';
const browser = await launch();
const out = process.env.ATV_PERF_OUTPUT || 'work/verification/performance';
await mkdir(out, { recursive: true });
const rows = [];
try {
  for (const [side, base] of [
    ['before', process.env.ATV_PERF_BASELINE],
    ['after', siteBase]
  ].filter(([, url]) => url)) {
    const errors = [];
    const p = await newPage(
      browser,
      { viewport: { width: 390, height: 844 }, deviceScaleFactor: 3, hasTouch: true },
      errors
    );
    await p.addInitScript(() => {
      localStorage.setItem('atv.diagnostics.v1', 'performance');
      window.__work = { enabled: false, raf: 0, draws: 0, hudClears: 0 };
      const a = window.__work,
        r = requestAnimationFrame;
      window.requestAnimationFrame = cb =>
        r.call(window, t => {
          if (a.enabled && cb.name === 'render') a.raf++;
          cb(t);
        });
      for (const method of ['drawElements', 'drawElementsInstanced', 'drawArraysInstanced']) {
        const f = WebGL2RenderingContext.prototype[method];
        WebGL2RenderingContext.prototype[method] = function (...args) {
          if (a.enabled) a.draws++;
          return f.apply(this, args);
        };
      }
      const clear = CanvasRenderingContext2D.prototype.clearRect;
      CanvasRenderingContext2D.prototype.clearRect = function (...args) {
        if (a.enabled && this.canvas.id === 'native-hud') a.hudClears++;
        return clear.apply(this, args);
      };
    });
    await p.goto(`${base}/play.html?drive=1&environment=env01`);
    await p.waitForFunction(() => window.atvTerrainStatus?.simulation && window.atvTerrainStatus.minimap.ready, null, {
      timeout: 90000
    });
    await p.bringToFront();
    await p.locator('#game').focus();
    const measure = async phase => {
      // Time-based on purpose: settle after the phase change, then count work over a 2 s wall-clock window.
      await p.waitForTimeout(200);
      const start = await p.evaluate(() => {
        Object.assign(window.__work, { enabled: true, raf: 0, draws: 0, hudClears: 0 });
        const s = window.atvTerrainStatus;
        return { t: performance.now(), steps: s.simulationSteps, frames: s.frames };
      });
      await p.waitForTimeout(2000);
      return p.evaluate(
        ({ phase, start }) => {
          window.__work.enabled = false;
          const s = window.atvTerrainStatus;
          return {
            phase,
            seconds: (performance.now() - start.t) / 1000,
            counters: window.__work,
            steps: s.simulationSteps - start.steps,
            frames: s.frames - start.frames,
            hud: s.hud,
            hudPixels: document.querySelector('#native-hud').width * document.querySelector('#native-hud').height,
            performance: s.performance
          };
        },
        { phase, start }
      );
    };
    const phases = [];
    phases.push(await measure('active'));
    await p.getByRole('button', { name: 'Pause', exact: true }).click();
    phases.push(await measure('paused'));
    await p.getByRole('button', { name: 'Resume', exact: true }).click();
    await waitForSteps(p, 31, { timeout: 10000 });
    await p.evaluate(() => window.dispatchEvent(new Event('blur')));
    phases.push(await measure('blur-visible'));
    if (side === 'after')
      for (const row of phases.slice(1)) {
        assert.equal(row.counters.draws, 0);
        assert.equal(row.counters.hudClears, 0);
        assert.equal(row.counters.raf, 0);
        assert.equal(row.steps, 0);
        assert.equal(row.hudPixels, 1);
      }
    await p.evaluate(() => window.dispatchEvent(new Event('focus')));
    await p.locator('#game').focus();
    await p.keyboard.down('KeyW');
    await p.waitForFunction(
      () => window.atvTerrainStatus.input.throttle === 1 && window.atvTerrainStatus.frame.speed > 5,
      null,
      { timeout: 15000 }
    );
    await p.keyboard.up('KeyW');
    const lost = await p.evaluate(() => {
      const ext = document.querySelector('#game').getContext('webgl2').getExtension('WEBGL_lose_context');
      window.__restoreContext = ext;
      ext.loseContext();
      return true;
    });
    if (lost) {
      await p.waitForFunction(() => !window.atvTerrainStatus.ready);
      await p.evaluate(() => window.__restoreContext.restoreContext());
      await p.waitForFunction(() => window.atvTerrainStatus.ready && window.atvTerrainStatus.frames > 0, null, {
        timeout: 15000
      });
      assert.equal(await p.evaluate(() => document.querySelector('#game').getContext('webgl2').getError()), 0);
    }
    assertNoErrors(errors);
    rows.push({ side, phases, errors });
    console.log(JSON.stringify(rows.at(-1)));
    await p.close();
  }
  await writeFile(
    `${out}/paired-lifecycle.json`,
    JSON.stringify(
      {
        scope:
          'Serial same-host Chromium Metal; 390x844 DPR3 actual game, 2s windows; synthetic blur event; not physical-phone battery measurement',
        rows
      },
      null,
      2
    )
  );
  console.log('PASS: idle render/HUD work stops; focus, driving and context restoration wake the game');
} finally {
  await browser.close();
}
