// A returning visitor after a deploy: an older offline worker (sw.js) controls
// the page while the new deploy's page loads its ?v= module graph through it.
// Production regression (2026-10-27, iPhone Safari on LTE): the worker passed
// atv_app_bg.wasm on while it was still downloading, the transfer dropped and
// the page stopped at "Engine failed: FetchEvent.respondWith received an error:
// Load failed". Here a proxy cuts the new WASM's transfer half way:
//   1. one dropped transfer: the worker retries it and the game starts and rides;
//   2. every transfer through the worker dropped: the page unregisters the
//      worker, reloads once without it and reaches the title.
// Serves two packaged copies of web/dist (versions A and B, the production ?v=
// rewrite) with the ignored hosted content; the proxy keeps serving version A's
// sw.js so the old worker stays in control, as on a phone whose tab stays open.
// On the previous shell code WebKit (Playwright, iPhone user agent) fails step 1
// with the production message verbatim; Chromium says "Response body loading was aborted".
// Run: node tools/run_browser_tests.mjs --only test_engine_sw_update.mjs (content group).
import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { createServer as createHttpServer, request as httpRequest } from 'node:http';
import { createServer } from 'node:net';
import { assertNoErrors, launch, watchErrors } from './browser/harness.mjs';

const A = 'a1'.repeat(20), B = 'b2'.repeat(20);
const fixture = version => `work/verification/sw-update-site-${version.slice(0, 2)}`;
for (const version of [A, B]) {
  execFileSync('python3', [
    '-c',
    `
from pathlib import Path
import sys, shutil
sys.path.insert(0, 'tools')
from package_release import STATIC_FILES, versioned_source
out = Path('${fixture(version)}')
shutil.rmtree(out, ignore_errors=True); out.mkdir(parents=True)
for name in STATIC_FILES:
    source = Path('web/dist') / name
    if name.endswith(('.html', '.js', '.css')):
        (out / name).write_text(versioned_source(source.read_text(), '${version}'))
    else:
        shutil.copy2(source, out / name)
`
  ]);
}

const freePort = () => new Promise(resolve => {
  const probe = createServer().listen(0, '127.0.0.1', () => {
    const { port } = probe.address();
    probe.close(() => resolve(port));
  });
});
const servers = [];
async function serve(root) {
  const port = await freePort();
  const server = spawn('python3', ['tools/serve_web.py', '--port', String(port), '--root', root], { stdio: ['ignore', 'pipe', 'pipe'] });
  servers.push(server);
  await new Promise((resolve, reject) => {
    server.stdout.on('data', data => String(data).includes('listening') && resolve());
    server.on('exit', code => reject(Error(`fixture server exited ${code}`)));
  });
  return port;
}
const upstream = { [A]: await serve(fixture(A)), [B]: await serve(fixture(B)) };

// The deployed site: `current` picks the deploy; sw.js always comes from A.
// `drops` transfers of B's WASM are cut after half of their body.
const site = { current: A, drops: 0, dropped: 0 };
const wasmB = `/atv_app_bg.wasm?v=${B}`;
const proxy = createHttpServer((req, res) => {
  const version = req.url.split('?')[0] === '/sw.js' ? A : site.current;
  const forward = httpRequest({ host: '127.0.0.1', port: upstream[version], path: req.url, method: req.method, headers: req.headers }, answer => {
    res.writeHead(answer.statusCode, answer.headers);
    if (req.url !== wasmB || site.drops <= 0) return void answer.pipe(res);
    site.drops--;
    site.dropped++;
    const length = Number(answer.headers['content-length']);
    let sent = 0;
    answer.on('data', chunk => {
      if (sent >= length / 2) return;
      sent += chunk.length;
      res.write(chunk);
      if (sent >= length / 2) setTimeout(() => res.socket?.destroy(), 50);
    });
  });
  forward.on('error', () => res.socket?.destroy());
  req.pipe(forward);
});
const port = await freePort();
await new Promise(resolve => proxy.listen(port, '127.0.0.1', resolve));
const origin = `http://127.0.0.1:${port}`;

const report = {};
const errors = [];
const browser = await launch();
const context = await browser.newContext({ viewport: { width: 844, height: 390 }, deviceScaleFactor: 2, hasTouch: true });
const page = await context.newPage();
watchErrors(page, errors);
let navigations = 0;
page.on('framenavigated', frame => { if (frame === page.mainFrame()) navigations++; });
const wait = (predicate, arg, timeout = 120000) => page.waitForFunction(predicate, arg, { timeout });
const title = () => wait(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === 'title'
  || /Engine failed/.test(document.querySelector('#status')?.textContent ?? ''));
const status = () => page.evaluate(() => document.querySelector('#status')?.textContent ?? '');
const onTitle = async () => {
  await title();
  assert.doesNotMatch(await status(), /Engine failed/);
};

try {
  // Version A installs its worker, which then controls the page.
  await page.goto(`${origin}/play.html?uiDiagnostics=1`);
  await onTitle();
  await page.waitForFunction(() => navigator.serviceWorker.controller?.state === 'activated', null, { timeout: 120000 });
  await page.reload();
  await onTitle();
  assert.ok(await page.evaluate(() => Boolean(navigator.serviceWorker.controller)), 'version A worker controls the page');

  // 1. Deploy B; its first WASM transfer drops. The worker retries and the ride starts.
  site.current = B;
  site.drops = 1;
  navigations = 0;
  await page.goto(`${origin}/play.html?map=env01&play=1&uiDiagnostics=1`);
  await wait(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').active === true
    || /Engine failed/.test(document.querySelector('#status')?.textContent ?? ''));
  assert.doesNotMatch(await status(), /Engine failed/);
  assert.equal(site.dropped, 1, 'the first transfer was cut');
  assert.equal(navigations, 1, 'the worker recovered without a reload');
  assert.ok(await page.evaluate(() => Boolean(navigator.serviceWorker.controller)), 'still served by the old worker');
  assert.match(await page.evaluate(() => document.querySelector('script[src*="engine.js"]').src), new RegExp(B));
  report.retried = true;

  // 2. Every transfer through the worker drops: one reload without the worker.
  site.drops = 2;
  site.dropped = 0;
  navigations = 0;
  await page.goto(`${origin}/play.html?uiDiagnostics=1`);
  for (let i = 0; i < 1200 && navigations < 2; i++) await new Promise(resolve => setTimeout(resolve, 100));
  await page.waitForLoadState('load');
  await onTitle();
  assert.equal(await page.evaluate(() => Boolean(navigator.serviceWorker.controller)), false, 'the reload ran without the worker');
  assert.equal(site.dropped, 2);
  assert.equal(navigations, 2, 'exactly one recovery reload');
  report.recoveredByReload = true;
  // The cut transfers and the loads cancelled by leaving a page are expected failures (Chromium and WebKit wording).
  assertNoErrors(errors.filter(error => !/Load failed|network error|Failed to fetch|ERR_|served by service worker is an error|access control checks/i.test(String(error))));
  console.log(JSON.stringify(report));
  console.log('PASS: a returning visitor\'s older worker survives a dropped WASM transfer (retry) and a dead one (reload without the worker).');
} finally {
  await browser.close();
  proxy.close();
  for (const server of servers) if (server.exitCode === null) server.kill();
}
