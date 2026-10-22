// The Rust engine page's shell (engine.html): app manifest and icons, the
// install banner on the title screen (iPhone), anonymous usage events through
// analytics.js, adaptive render resolution, and the offline service worker
// precaching engine.html with atv_app_bg.wasm. Like test_offline.mjs it serves
// its own packaged copy of web/dist (the production ?v= rewrite) with the
// ignored hosted content, then stops that server to relaunch offline.
// Run: node tools/run_browser_tests.mjs --only test_engine_shell.mjs (content group).
import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { createServer } from 'node:net';
import { assertNoErrors, launch, watchErrors } from './browser/harness.mjs';

const COMMIT = '5e11'.repeat(10);
const fixture = 'work/verification/engine-shell-site';
execFileSync('python3', [
  '-c',
  `
from pathlib import Path
import sys, shutil
sys.path.insert(0, 'tools')
from package_release import STATIC_FILES, versioned_source
out = Path('${fixture}')
shutil.rmtree(out, ignore_errors=True); out.mkdir(parents=True)
for name in STATIC_FILES:
    source = Path('web/dist') / name
    if name.endswith(('.html', '.js', '.css')):
        (out / name).write_text(versioned_source(source.read_text(), '${COMMIT}'))
    else:
        shutil.copy2(source, out / name)
`
]);
const port = await new Promise(resolve => {
  const probe = createServer().listen(0, '127.0.0.1', () => {
    const { port } = probe.address();
    probe.close(() => resolve(port));
  });
});
const origin = `http://127.0.0.1:${port}`;
const server = spawn('python3', ['tools/serve_web.py', '--port', String(port), '--root', fixture], {
  stdio: ['ignore', 'pipe', 'pipe']
});
await new Promise((resolve, reject) => {
  server.stdout.on('data', data => String(data).includes('listening') && resolve());
  server.on('exit', code => reject(Error(`fixture server exited ${code}`)));
});

const report = {};
const errors = [];
const events = [];
const browser = await launch();
// An iPhone-like page: the banner offers Add to Home Screen on the title.
const context = await browser.newContext({
  viewport: { width: 844, height: 390 },
  deviceScaleFactor: 2,
  hasTouch: true,
  userAgent:
    'Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1'
});
await context.exposeBinding('furyUsageSendNode', (_, event) => events.push(event));
await context.addInitScript(() => {
  Object.defineProperty(document, 'hasFocus', { value: () => true });
  window.furyUsageSend = event => window.furyUsageSendNode(event);
});
const page = await context.newPage();
watchErrors(page, errors);
const ui = () => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const wait = (predicate, arg, timeout = 120000) => page.waitForFunction(predicate, arg, { timeout });
const backing = () =>
  page.evaluate(() => {
    const canvas = document.querySelector('#game');
    return canvas.width / canvas.getBoundingClientRect().width;
  });

try {
  // 1. Title: manifest, icons, the banner and the shell's page mirror.
  await page.goto(`${origin}/engine.html?uiDiagnostics=1`);
  await wait(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === 'title');
  const head = await page.evaluate(() => ({
    manifest: document.querySelector('link[rel=manifest]')?.getAttribute('href'),
    icon: document.querySelector('link[rel=apple-touch-icon]')?.getAttribute('href'),
    theme: document.querySelector('meta[name=theme-color]')?.content
  }));
  assert.deepEqual(head, {
    manifest: '/manifest.webmanifest',
    icon: '/fury-remastered-home-180-v1.png',
    theme: '#9c1414'
  });
  await wait(() => !document.querySelector('#app-install-banner').hidden, null, 10000);
  assert.equal(await page.evaluate(() => document.querySelector('#frontend').dataset.page), 'title');
  // Tapping Add app opens the iOS help (engine.js leaves the banner its taps).
  await page.tap('#app-install-open');
  await wait(() => document.querySelector('#app-install-help').open, null, 5000);
  report.installSteps = await page.evaluate(() => document.querySelectorAll('#app-install-steps li').length);
  await page.tap('#app-install-close');
  // 2x display: the frontend draws at the 1.5 render-ratio cap.
  report.titleBacking = await backing();
  assert.ok(Math.abs(report.titleBacking - 1.5) < 0.02, `title backing ratio ${report.titleBacking}`);
  assert.deepEqual(
    events.map(e => [e.name, e.entry]),
    [['session_start', 'menu']]
  );

  // 2. A ride link: ride_start; the banner hides while riding.
  events.length = 0;
  await page.goto(`${origin}/engine.html?map=env01&play=1&uiDiagnostics=1`);
  await wait(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').active === true);
  assert.equal(await page.evaluate(() => document.querySelector('#frontend').hidden), true);
  assert.equal(await page.evaluate(() => document.querySelector('#app-install-banner').hidden), true);
  const start = events.find(e => e.name === 'ride_start');
  assert.ok(start, `ride_start: ${JSON.stringify(events)}`);
  assert.deepEqual(
    { mode: start.mode, activity: start.activity, map: start.map, vehicle: start.vehicle, quad: start.quad },
    { mode: 'single', activity: 'freestyle', map: 'env01', vehicle: 'atv', quad: 1 }
  );
  assert.ok(start.seconds > 0 && ['touch', 'desktop'].includes(start.device) && start.display === 'browser');
  assert.equal(events.find(e => e.name === 'session_start')?.entry, 'link');
  report.rideStart = start;

  // 3. Slow frames step the render ratio 1.5 → 1.25 → 1.0 (CPU throttled).
  const cdp = await context.newCDPSession(page);
  await cdp.send('Emulation.setCPUThrottlingRate', { rate: 20 });
  const ratios = [await backing()];
  for (let i = 0; i < 40 && ratios.at(-1) > 1.01; i++) {
    await page.waitForTimeout(500); // Time-based on purpose: the scale reacts to seconds of slow frames.
    ratios.push(await backing());
  }
  await cdp.send('Emulation.setCPUThrottlingRate', { rate: 1 });
  report.renderRatios = [...new Set(ratios.map(r => Math.round(r * 100) / 100))];
  assert.ok(Math.abs(ratios[0] - 1.5) < 0.02, `riding starts at 1.5: ${report.renderRatios}`);
  assert.ok(Math.abs(ratios.at(-1) - 1) < 0.02, `slow frames step down to 1.0: ${report.renderRatios}`);

  // 4. Riding time stops in the pause menu (setRiding false): report it.
  await page.waitForTimeout(1500); // Time-based on purpose: accrue riding seconds.
  await page.keyboard.press('Escape');
  await wait(() => JSON.parse(document.querySelector('#ui-state').textContent).toolbar?.menu?.page === 'pause', null, 10000);
  await page.waitForTimeout(1500); // Paused seconds must not count.
  const paused = await page.evaluate(() => window.furyUsage.active);
  await page.evaluate(() => window.furyUsage.report(1));
  const time = events.find(e => e.name === 'ride_time');
  assert.equal(paused, false, 'the pause menu stops riding time');
  assert.ok(time && time.seconds >= 1 && time.map === 'env01', `ride_time: ${JSON.stringify(time)}`);
  report.rideTime = time.seconds;

  // 5. Offline: the worker precached engine.html and the Rust module graph.
  await page.waitForFunction(() => navigator.serviceWorker.controller?.state === 'activated', null, { timeout: 120000 });
  const cached = await page.evaluate(async () => {
    const out = {};
    for (const name of await caches.keys())
      out[name] = (await (await caches.open(name)).keys()).map(({ url }) => new URL(url).pathname + new URL(url).search);
    return out;
  });
  const shell = cached[`fury-shell-v1-${COMMIT}`] ?? [];
  for (const name of ['engine.js', 'engine.css', 'atv_app.js', 'atv_app_bg.wasm', 'install.js', 'analytics.js', 'offline.js'])
    assert.ok(shell.includes(`/${name}?v=${COMMIT}`), `shell precaches ${name}`);
  assert.ok((cached['fury-pages-v1'] ?? []).includes('/engine.html'), 'engine.html page cached');
  report.shellFiles = shell.length;
  server.kill();
  await new Promise(resolve => server.once('exit', resolve));
  await context.setOffline(true);
  await page.goto(`${origin}/engine.html?uiDiagnostics=1`);
  await wait(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === 'title');
  report.offlineTitle = true;
  assertNoErrors(errors);
  console.log(JSON.stringify(report, null, 2));
} finally {
  await browser.close();
  if (server.exitCode === null) server.kill();
}
