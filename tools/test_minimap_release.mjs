// Returning-browser regression: keep old CSS/HUD in a real HTTP cache, then
// load the new release without clearing cache or saved settings.
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { execFileSync } from 'node:child_process';
import { readFile, writeFile, mkdir, rm } from 'node:fs/promises';
import { assertNoErrors, button, gpuArgs, playwright, watchErrors } from './browser/harness.mjs';
const { chromium, webkit } = playwright();
const engine = process.env.ATV_UI_BROWSER === 'webkit' ? webkit : chromium;
const out = process.env.ATV_MINIMAP_OUTPUT || 'work/verification/minimap-cache';
const fixture = 'work/verification/minimap-cache-fixtures';
const commit = 'b'.repeat(40),
  requests = [],
  errors = [];
await mkdir(`${fixture}/old`, { recursive: true });
for (const name of ['game.css', 'hud.js'])
  await writeFile(`${fixture}/old/${name}`, execFileSync('git', ['show', `f2b59ec:web/${name}`]));
// Use the production packager's transformation, not hand-written test URLs.
execFileSync('python3', [
  '-c',
  `
from pathlib import Path
import sys, shutil
sys.path.insert(0, 'tools')
from package_release import STATIC_FILES, versioned_source
out = Path('${fixture}/current'); out.mkdir(parents=True, exist_ok=True)
for name in STATIC_FILES:
    source = Path('web/dist') / name
    if name.endswith(('.html', '.js', '.css')):
        (out / name).write_text(versioned_source(source.read_text(), '${commit}'))
    else:
        shutil.copy2(source, out / name)
`
]);
let upgraded = false;
const mime = name =>
  name.endsWith('.js')
    ? 'text/javascript'
    : name.endsWith('.css')
      ? 'text/css'
      : name.endsWith('.wasm')
        ? 'application/wasm'
        : name.endsWith('.svg')
          ? 'image/svg+xml'
          : name.endsWith('.png')
            ? 'image/png'
            : name.endsWith('.webmanifest')
              ? 'application/manifest+json'
              : name.endsWith('.json')
                ? 'application/json'
                : 'text/html';
const server = createServer(async (req, res) => {
  const url = new URL(req.url, 'http://host'),
    name = url.pathname.slice(1);
  requests.push({ name, version: url.searchParams.get('v'), upgraded });
  try {
    if (/^content\/(manifest\.json|[a-f0-9]{64}\.bin)$/.test(name)) {
      res.setHeader('Content-Type', name.endsWith('.json') ? 'application/json' : 'application/octet-stream');
      res.end(await readFile(`private/reports/hosted-content/${name.slice(8)}`));
      return;
    }
    if (!/^[a-z0-9_-]+\.(html|css|js|wasm|png|svg|webmanifest)$/.test(name)) {
      res.writeHead(404);
      res.end();
      return;
    }
    const path = upgraded
      ? `${fixture}/current/${name}`
      : ['game.css', 'hud.js'].includes(name)
        ? `${fixture}/old/${name}`
        : `web/dist/${name}`;
    res.setHeader('Content-Type', mime(name));
    res.setHeader('Cache-Control', /\.(css|js|wasm)$/.test(name) ? 'public, max-age=31536000' : 'no-store');
    res.end(await readFile(path));
  } catch {
    res.writeHead(404);
    res.end();
  }
});
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
const base = `http://127.0.0.1:${server.address().port}`;
await mkdir(out, { recursive: true });
const profile = `${out}/browser-profile`;
// A persistent profile keeps the real HTTP cache, so the harness launch() does not apply;
// Chromium still needs the harness WebGL backend flags.
const browser = await engine.launchPersistentContext(profile, {
  headless: true,
  viewport: { width: 390, height: 844 },
  hasTouch: true,
  args: engine === chromium ? gpuArgs() : []
});
try {
  const page = await browser.newPage();
  watchErrors(page, errors);
  const map = page.locator('#freestyle-minimap');
  const layout = () =>
    map.evaluate(e => {
      const c = getComputedStyle(e),
        r = e.getBoundingClientRect();
      return {
        x: r.x,
        y: r.y,
        width: r.width,
        height: r.height,
        radius: c.borderRadius,
        padding: c.padding,
        border: c.borderWidth,
        background: c.backgroundColor
      };
    });
  const nativePixels = () =>
    page.locator('#native-hud').evaluate(c =>
      c
        .getContext('2d')
        .getImageData(0, 0, c.width, c.height)
        .data.some((v, i) => i % 4 === 3 && v > 0)
    );
  const ready = () => page.waitForFunction(() => window.atvTerrainStatus?.minimap.visible, null, { timeout: 90000 });
  await page.goto(`${base}/play.html?drive=1&environment=env02`);
  await ready();
  const broken = await layout();
  assert(broken.y > 100, 'Old stylesheet produces the reported mid-right placement');
  assert.notEqual(broken.padding, '0px', 'Old generic button padding produces the reported dark surround');
  assert(await nativePixels(), 'Old HUD module draws the duplicate standalone MPH');
  await page.screenshot({ path: `${out}/mixed-before.png` });
  upgraded = true;
  await page.goto('about:blank');
  await page.goto(`${base}/play.html?drive=1&environment=env02`);
  await ready();
  const fixed = await layout();
  assert.equal(fixed.radius, '50%');
  assert.equal(fixed.padding, '0px');
  assert.equal(fixed.border, '0px');
  assert.equal(fixed.background, 'rgba(0, 0, 0, 0)');
  assert.equal(fixed.y, 8);
  assert(Math.abs(390 - fixed.x - fixed.width - 8) < 1);
  assert.equal(await nativePixels(), false, 'Enabled GPS is the only speed owner');
  // The original cache entries remain present: passing this test must not rely
  // on clearing the cache, a hard refresh or a new browser context.
  const cachedOld = await page.evaluate(() => fetch('game.css').then(r => r.text()));
  assert.equal(cachedOld, await readFile(`${fixture}/old/game.css`, 'utf8'));
  const graph = requests.filter(r => r.upgraded && /\.(js|css|wasm)$/.test(r.name));
  assert(graph.length >= 12);
  assert(
    graph.every(r => r.version === commit),
    'Every transitive module, CSS, Worker and WASM uses this release key'
  );
  await page.screenshot({ path: `${out}/fixed-after.png` });
  await page.evaluate(() => window.dispatchEvent(new Event('blur')));
  await page.waitForTimeout(100); // Time-based on purpose: the speed flag must stay false, so there is no change to wait for.
  assert.equal(
    await page.locator('#native-hud').getAttribute('data-speed-visible'),
    'false',
    'Temporary map hiding cannot restore standalone MPH'
  );
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await ready();
  await button(page, 'Pause').click();
  await button(page, 'HUD settings').click();
  await button(page, 'Minimap configuration').click();
  await button(page, 'Freestyle minimap: On').click();
  for (const name of ['Back', 'Back', 'Resume']) await button(page, name).click();
  await page.waitForFunction(() => document.querySelector('#native-hud').dataset.speedVisible === 'true');
  assert(await nativePixels(), 'Explicit Off restores standalone speed');
  assertNoErrors(errors);
  await writeFile(
    `${out}/results.json`,
    JSON.stringify({ engine: process.env.ATV_UI_BROWSER || 'chromium', broken, fixed, graph, errors }, null, 2)
  );
  console.log(
    'PASS: reproduced mixed-version rectangle, padding and duplicate MPH; fresh release graph fixes a returning browser without clearing its old HTTP cache; explicit Off alone restores Freestyle speed.'
  );
} finally {
  await browser.close();
  await rm(profile, { recursive: true, force: true });
  server.closeAllConnections();
  await new Promise(resolve => server.close(resolve));
}
