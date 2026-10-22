// Host-only gate: a Rust engine room rider stays present while its browser
// tab is hidden. The hidden tab runs no animation frames (emulated by holding
// requestAnimationFrame and reporting visibilityState hidden); the engine's
// 1 s presence timer keeps sending paused presence, so the JavaScript host
// never marks it stale. Visible again, it rides on.
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_multiplayer_presence.mjs
import assert from 'node:assert/strict';
import { assertNoErrors, base, button, jsBase, launch, newPage } from './browser/harness.mjs';

const report = {};
const errors = [];
const browser = await launch();
const engine = p => p.evaluate(() => JSON.parse(window.furyMultiplayer ?? 'null'));
try {
  const host = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await host.addInitScript(() => Object.defineProperty(document, 'hasFocus', { value: () => true }));
  await host.goto(jsBase + '/play.html');
  await button(host, 'Press START').click({ timeout: 90000 });
  await button(host, 'Multiplayer').click();
  await button(host, 'Host Game').click();
  await host.getByLabel('Rider name').fill('JsHost');
  await button(host, 'Select Track').click();
  await host.locator('button.ms-ride').click({ timeout: 60000 });
  await host.waitForFunction(() => window.atvTerrainStatus?.multiplayer?.state?.code, null, { timeout: 120000 });
  const code = (await host.evaluate(() => window.atvTerrainStatus)).multiplayer.state.code;

  const guest = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await guest.addInitScript(() => {
    Object.defineProperty(document, 'hasFocus', { value: () => !window.testHidden });
    Object.defineProperty(document, 'visibilityState', {
      get: () => (window.testHidden ? 'hidden' : 'visible')
    });
    Object.defineProperty(document, 'hidden', { get: () => Boolean(window.testHidden) });
    // A hidden tab runs no animation frames; queue them until visible.
    const native = window.requestAnimationFrame.bind(window);
    let held = [];
    window.requestAnimationFrame = callback => {
      if (window.testHidden) {
        held.push(callback);
        return 0;
      }
      return native(callback);
    };
    window.testShow = () => {
      window.testHidden = false;
      document.dispatchEvent(new Event('visibilitychange'));
      const queued = held;
      held = [];
      for (const callback of queued) native(callback);
    };
  });
  await guest.goto(`${base}/engine.html?room=${code}&name=RustGuest`);
  await guest.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 1, null, {
    timeout: 180000
  });
  await host.waitForFunction(() => window.atvTerrainStatus.multiplayer?.remote?.some(r => !r.stale), null, {
    timeout: 60000
  });
  const before = await engine(guest);
  const rustPeer = before.peer;

  // Hide the Rust tab for 12 s; the host samples its freshness every second.
  await guest.evaluate(() => {
    window.testHidden = true;
    document.dispatchEvent(new Event('visibilitychange'));
  });
  const fresh = [];
  for (let i = 0; i < 12; i++) {
    await host.waitForTimeout(1000);
    fresh.push(
      await host.evaluate(
        peer => window.atvTerrainStatus.multiplayer.remote.some(r => r.peer === peer && !r.stale),
        rustPeer
      )
    );
  }
  const hidden = await engine(guest);
  report.hidden = {
    freshSamples: fresh.filter(Boolean).length,
    samples: fresh.length,
    messagesWhileHidden: hidden.network.messages - before.network.messages,
    closed: hidden.closed
  };
  assert.ok(fresh.every(Boolean), `the hidden Rust rider should stay fresh on the host: ${JSON.stringify(report)}`);
  assert.ok(report.hidden.messagesWhileHidden >= 1, JSON.stringify(report));

  await guest.evaluate(() => window.testShow());
  await guest.waitForFunction(
    count => JSON.parse(window.furyMultiplayer ?? 'null')?.network?.messages > count,
    hidden.network.messages,
    { timeout: 30000 }
  );
  report.visibleAgain = (await engine(guest)).drawn;
  assertNoErrors(errors);
  console.log(JSON.stringify(report, null, 2));
} finally {
  await browser.close();
}
