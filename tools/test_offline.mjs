// Offline / fast-relaunch gate for the service worker (research/OFFLINE.md).
// The worker only registers in packaged builds, so this test serves its own
// packaged copy of web/dist (the production ?v= rewrite) with the ignored hosted
// content on a separate local port, then stops that server to go offline.
// Set ATV_UI_BROWSER=webkit to use WebKit instead of Chromium.
// Run: node tools/run_browser_tests.mjs --only test_offline.mjs (content group).
import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { createServer } from 'node:net';
import { assertNoErrors, button, launch, waitForGame, waitForScreen, watchErrors } from './browser/harness.mjs';

const COMMIT = '0ff1'.repeat(10);
const fixture = 'work/verification/offline-site';
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
const served = [];
const server = spawn('python3', ['tools/serve_web.py', '--port', String(port), '--root', fixture], {
  stdio: ['ignore', 'pipe', 'pipe']
});
server.stderr.on('data', data => {
  for (const match of String(data).matchAll(/"(?:GET|HEAD|POST) (\S+) HTTP/g)) served.push(match[1]);
});
await new Promise((resolve, reject) => {
  server.stdout.on('data', data => String(data).includes('listening') && resolve());
  server.on('exit', code => reject(Error(`fixture server exited ${code}`)));
});

const engine = process.env.ATV_UI_BROWSER === 'webkit' ? 'webkit' : 'chromium';
const browser = await launch({ engine });
const errors = [];
const responses = [];
const context = await browser.newContext({ viewport: { width: 1280, height: 800 }, hasTouch: true });
const page = await context.newPage();
watchErrors(page, errors);
await page.addInitScript(() => Object.defineProperty(document, 'hasFocus', { value: () => true }));
// ATV_OFFLINE_VERBOSE=1 lists failed requests (expected offline: streams, multiplayer, analytics).
if (process.env.ATV_OFFLINE_VERBOSE)
  page.on('requestfailed', request => console.log('FAILED', request.url(), request.failure()?.errorText));
page.on('response', response => {
  const url = new URL(response.url());
  responses.push({ path: url.pathname + url.search, worker: response.fromServiceWorker(), status: response.status() });
});
const cached = () =>
  page.evaluate(async () => {
    const out = {};
    for (const name of await caches.keys())
      out[name] = (await (await caches.open(name)).keys()).map(
        ({ url }) => new URL(url).pathname + new URL(url).search
      );
    return out;
  });
const blobs = list => (list['fury-content-v1'] ?? []).filter(path => /^\/content\/[a-f0-9]{64}\.bin$/.test(path));
async function menu(path = '/play.html') {
  await page.goto(origin + path);
  await button(page, 'Press START').click({ timeout: 90000 });
  await waitForScreen(page, 'main');
}
async function ride() {
  await page.goto(`${origin}/play.html?mode=single&map=env01`);
  return waitForGame(page, {
    timeout: 120000,
    until: () => window.atvTerrainStatus.simulation && document.querySelector('#frontend').hidden
  });
}
// Content caching finishes after the responses (hashing, cache writes, warm-up).
async function settledBlobs() {
  let previous = -1;
  for (let i = 0; i < 60; i++) {
    const count = blobs(await cached()).length;
    if (count > 0 && count === previous) return blobs(await cached());
    previous = count;
    await page.waitForTimeout(1000); // Deliberately time-based: waits for idle background cache writes.
  }
  throw Error('content cache never settled');
}

try {
  // First visit: uncontrolled page; the worker installs the whole shell and takes control.
  await menu();
  await page.waitForFunction(() => navigator.serviceWorker.controller?.state === 'activated', null, { timeout: 60000 });
  const first = await cached();
  const shell = first[`fury-shell-v1-${COMMIT}`] ?? [];
  for (const name of [
    'game.js',
    'frontend.js',
    'content.js',
    'worker.js',
    'atv_web.wasm',
    'game.css',
    'install.js',
    'offline.js',
    'landing.js'
  ])
    assert.ok(shell.includes(`/${name}?v=${COMMIT}`), `shell precaches ${name}: ${shell.join(', ')}`);
  for (const path of [
    '/manifest.webmanifest',
    '/fury-remastered.svg',
    '/fury-remastered-home-512-v1.png',
    '/fury-remastered-maskable-512-v1.png',
    '/favicon.ico',
    '/__fury-offline/shell-complete'
  ])
    assert.ok(shell.includes(path), `shell precaches ${path}`);
  assert.ok(!shell.some(path => path.includes('/content/')), 'content is not part of the shell');
  assert.deepEqual([...(first['fury-pages-v1'] ?? [])].sort(), ['/index.html', '/play.html']);
  // Content the uncontrolled first visit fetched is reported to the worker and cached.
  const menuBlobs = await settledBlobs();
  assert.ok((await cached())['fury-manifests-v1']?.includes('/content/manifest.json'), 'content manifest cached');
  console.log(`first visit: ${shell.length} shell files, ${menuBlobs.length} menu content blobs cached`);

  // Ride online once so that venue's content is cached.
  await ride();
  const rideBlobs = await settledBlobs();
  assert.ok(rideBlobs.length > menuBlobs.length, 'venue content cached');

  // Relaunch online: the shell and cached content come from the worker, not the server.
  responses.length = 0;
  served.length = 0;
  await menu();
  const content = responses.filter(({ path }) => /^\/content\/[a-f0-9]{64}\.bin$/.test(path));
  assert.ok(content.length > 0, 'menu loads content');
  assert.ok(
    content.every(({ worker }) => worker),
    'cached content blobs are served by the worker'
  );
  assert.deepEqual(
    served.filter(path => rideBlobs.includes(path)),
    [],
    'no cached blob was downloaded again'
  );
  assert.ok(
    responses.some(({ path, worker }) => path === `/game.js?v=${COMMIT}` && worker),
    'shell served by the worker'
  );
  assert.ok(!served.some(path => path.startsWith('/game.js')), 'game.js not downloaded again');

  // Streams, ranged reads, multiplayer and analytics go straight to the network.
  responses.length = 0;
  const stream = await page.evaluate(async () => {
    const manifest = await (await fetch('/content/manifest.json')).json();
    const url = manifest.files.find(file => file.stream)?.stream.url;
    await fetch(url, { headers: { Range: 'bytes=0-15' } });
    await fetch(url);
    const blob = manifest.files[0].chunks?.[0].url ?? manifest.files[0].url;
    await fetch(blob, { headers: { Range: 'bytes=0-15' } });
    await fetch('/api/events', { method: 'POST', body: '{}' }).catch(() => {});
    await fetch('/multiplayer/freeride/riders').catch(() => {});
    const music = manifest.files
      .filter(file => file.path.startsWith('/AUDIO/MUSIC/'))
      .flatMap(file => (file.chunks ?? [file]).map(chunk => chunk.url));
    return { url, blob, music };
  });
  const passed = responses.filter(
    ({ path }) => path === stream.url || path === '/api/events' || path.startsWith('/multiplayer/')
  );
  assert.ok(passed.length >= 4, `pass-through requests observed: ${JSON.stringify(passed)}`);
  assert.ok(
    passed.every(({ worker }) => !worker),
    `never intercepted: ${JSON.stringify(passed)}`
  );
  const ranged = responses.filter(({ path }) => path === stream.blob);
  assert.ok(
    ranged.length && ranged.every(({ worker }) => !worker),
    'a ranged blob read is not answered from the cache'
  );

  // Offline: stop the only server and emulate no network (navigator.onLine
  // false), then relaunch. Playwright's WebKit offline emulation also fails
  // worker-served navigations ("internal error"), so WebKit gets the stopped
  // server plus only the onLine signal.
  server.kill();
  await new Promise(resolve => server.once('exit', resolve));
  const missing = [];
  page.on('requestfailed', request => missing.push(new URL(request.url()).pathname));
  if (engine === 'chromium') await context.setOffline(true);
  else await context.addInitScript(() => Object.defineProperty(Navigator.prototype, 'onLine', { get: () => false }));
  await page.goto(origin + '/');
  await page.waitForURL(/\/play\.html$/, { timeout: 30000 });
  await button(page, 'Press START').click({ timeout: 90000 });
  await waitForScreen(page, 'main');
  const offline = await ride();
  assert.ok(offline.ready, 'previously loaded venue plays offline');
  // Offline misses are content never fully loaded online: the soundtrack's
  // fallback chunks (the stream is never cached) and lazy files whose online
  // download was cut short by navigation. They are non-fatal; list them.
  const missingContent = missing.filter(path => /^\/content\/[a-f0-9]{64}\.bin$/.test(path));
  const missingOther = missingContent.filter(path => !stream.music.includes(path));
  console.log(
    `offline misses: ${missingContent.length - missingOther.length} soundtrack chunks, ${missingOther.length} other`
  );
  // WebKit reports those expected worker network errors as page errors.
  const expected =
    engine === 'webkit' && missingContent.length ? /^Response served by service worker is an error$/ : null;
  assertNoErrors(errors.filter(message => !expected?.test(message)));
  console.log(
    `PASS: ${engine} service worker controls the page, precaches the ${COMMIT.slice(0, 8)} shell (${shell.length} files), serves ${rideBlobs.length} cached content blobs without re-downloading, passes streams/ranged/multiplayer/analytics through, and relaunches to the main menu and env01 offline`
  );
} finally {
  await browser.close();
  if (server.exitCode === null) server.kill();
}
