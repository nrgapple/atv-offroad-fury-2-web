// Parity captures for the shared Rust engine. Renders one venue from the same
// camera in (1) the current JavaScript game, (2) the Rust engine in the browser
// (WebGL2) and (3) the native Rust build, writes the PNGs and reports the mean
// absolute difference per screen region.
//
//   node tools/compare_engine.mjs [venue] [out-dir] [--view x,y,z,yaw,pitch,focal]...
//        [--graphics original|enhanced|high] [--actors]
// Without --view the JavaScript start camera is used. The JavaScript game runs
// with the requested preset (Original by default), its actors hidden (unless
// --actors) and its scene clocks held while capturing (window.atvParityView);
// the Rust builds receive the same camera, Weather outcome and clock steps.
// Environment: ATV_TEST_URL (served site with private content), FURY_BIN
// (native binary; skipped when unset) and FURY_CONTENT (package directory).
// Outputs belong in ignored private storage; they contain game imagery.
import { execFileSync } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { assertNoErrors, launch, newPage, url, waitForGame } from './browser/harness.mjs';

const positional = [], views = [];
let graphics = 'original', actors = false;
for (let i = 2; i < process.argv.length; i++) {
  const arg = process.argv[i];
  if (arg === '--view') views.push(process.argv[++i]);
  else if (arg === '--graphics') graphics = process.argv[++i];
  else if (arg === '--actors') actors = true;
  else positional.push(arg);
}
const venue = positional[0] ?? 'env01';
const out = path.resolve(positional[1] ?? 'private/reports/engine-parity');
const viewport = { width: 1280, height: 720 };
await mkdir(out, { recursive: true });

// Hide every overlay (HUD, toolbar, help) so only the game canvas remains.
const canvasOnly = () => {
  const keep = new Set();
  for (let el = document.querySelector('#game'); el; el = el.parentElement) keep.add(el);
  for (const el of document.querySelectorAll('body *')) if (!keep.has(el)) el.style.visibility = 'hidden';
};
const frames = (page, count = 4) => page.evaluate(n => new Promise(resolve => { const step = () => (n-- <= 0 ? resolve() : requestAnimationFrame(step)); step(); }), count);

const browser = await launch();
const report = { venue, graphics, views: [] };
try {
  const errors = [];
  const page = await newPage(browser, { viewport }, errors);
  await page.addInitScript(([preset, world]) => {
    localStorage.setItem('atv-graphics', preset);
    window.atvParityView = { world };
  }, [graphics, !actors]);
  await page.goto(url(`/play.html?drive=1&environment=${venue}`));
  await waitForGame(page);
  // Let the start camera settle, then hold the scene clocks.
  await page.waitForTimeout(2500);
  await page.evaluate(() => { window.atvParityView.freeze = true; });
  await frames(page);
  const state = await page.evaluate(() => ({ camera: atvTerrainStatus.renderCamera, steps: atvTerrainStatus.simulationSteps, weather: atvTerrainStatus.weather?.active ?? false }));
  await page.evaluate(canvasOnly);
  const format = c => [c.x, c.y, c.z, c.yaw, c.pitch, c.focalLength].map(v => Number(v.toFixed(6))).join(',');
  const list = views.length ? views : [format(state.camera)];
  console.log(`steps ${state.steps} weather ${state.weather ? 'on' : 'off'}`);
  for (const [index, view] of list.entries()) {
    const [x, y, z, yaw, pitch, focalLength] = view.split(',').map(Number);
    await page.evaluate(camera => { window.atvParityView.camera = camera; }, { x, y, z, yaw, pitch, focalLength });
    await frames(page);
    const stem = path.join(out, `${venue}-${index}`);
    await page.locator('#game').screenshot({ path: `${stem}-javascript.png` });
    console.log(`view ${index}: ${view}`);
    const rustErrors = [];
    const rust = await newPage(browser, { viewport }, rustErrors);
    await rust.goto(url(`/engine.html?venue=${venue}&camera=${view}&weather=${state.weather ? 'on' : 'off'}&steps=${state.steps}&graphics=${graphics}`));
    await rust.waitForFunction(() => /fps/.test(document.querySelector('#status').textContent) || /fail|error/i.test(document.querySelector('#status').textContent), null, { timeout: 180000 });
    await rust.waitForTimeout(2500);
    const status = await rust.locator('#status').textContent();
    console.log(`rust web: ${status}`);
    await rust.evaluate(() => { document.querySelector('#status').hidden = true; });
    await rust.locator('canvas').screenshot({ path: `${stem}-rust-web.png` });
    assertNoErrors(rustErrors);
    await rust.close();
    const entry = { view, status };
    if (process.env.FURY_BIN) {
      const args = ['--venue', venue, '--camera', view, '--weather', state.weather ? 'on' : 'off', '--steps', String(state.steps), '--graphics', graphics, '--capture', `${stem}-rust-native.png`, `${viewport.width}x${viewport.height}`];
      if (process.env.FURY_CONTENT) args.unshift('--content', process.env.FURY_CONTENT);
      execFileSync(process.env.FURY_BIN, args, { stdio: 'inherit' });
    }
    entry.diff = await compare(browser, stem, Boolean(process.env.FURY_BIN));
    report.views.push(entry);
  }
  assertNoErrors(errors);
  await writeFile(path.join(out, `${venue}-report.json`), JSON.stringify(report, null, 2));
  console.log(JSON.stringify(report.views.map(v => v.diff), null, 1));
  console.log(`PASS: wrote ${venue} parity captures to ${out}`);
} finally {
  await browser.close();
}

// Mean absolute RGB difference (0-255) overall and on a 4x3 grid, plus the
// share of pixels differing by more than 24 in any channel; writes a heatmap.
async function compare(browser, stem, native) {
  const page = await browser.newPage();
  const data = async name => `data:image/png;base64,${(await readFile(`${stem}-${name}.png`)).toString('base64')}`;
  const sources = { javascript: await data('javascript'), web: await data('rust-web'), ...(native ? { native: await data('rust-native') } : {}) };
  const result = await page.evaluate(async sources => {
    const pixels = async src => {
      const image = new Image(); image.src = src; await image.decode();
      const canvas = new OffscreenCanvas(image.width, image.height), context = canvas.getContext('2d');
      context.drawImage(image, 0, 0);
      return context.getImageData(0, 0, image.width, image.height);
    };
    const decoded = Object.fromEntries(await Promise.all(Object.entries(sources).map(async ([k, v]) => [k, await pixels(v)])));
    const pair = (a, b, heat) => {
      const { width, height } = a, cells = Array.from({ length: 12 }, () => [0, 0]);
      let sum = 0, large = 0;
      for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
        const i = (y * width + x) * 4;
        const d = [0, 1, 2].map(c => Math.abs(a.data[i + c] - b.data[i + c])), m = (d[0] + d[1] + d[2]) / 3;
        sum += m; if (Math.max(...d) > 24) large++;
        const cell = cells[Math.min(2, Math.floor(y * 3 / height)) * 4 + Math.min(3, Math.floor(x * 4 / width))];
        cell[0] += m; cell[1]++;
        if (heat) { heat.data[i] = Math.min(255, m * 8); heat.data[i + 1] = Math.min(255, m * 2); heat.data[i + 2] = 0; heat.data[i + 3] = 255; }
      }
      const round = v => Number(v.toFixed(2));
      return { mean: round(sum / (width * height)), over24: round(100 * large / (width * height)), grid: [0, 1, 2].map(r => cells.slice(r * 4, r * 4 + 4).map(([s, n]) => round(s / n))) };
    };
    const { width, height } = decoded.javascript, heat = new ImageData(width, height);
    const result = { jsWeb: pair(decoded.javascript, decoded.web, heat) };
    if (decoded.native) result.webNative = pair(decoded.web, decoded.native), result.jsNative = pair(decoded.javascript, decoded.native);
    const canvas = new OffscreenCanvas(width, height); canvas.getContext('2d').putImageData(heat, 0, 0);
    const blob = await canvas.convertToBlob({ type: 'image/png' }), reader = new FileReader();
    result.heat = await new Promise(resolve => { reader.onload = () => resolve(reader.result); reader.readAsDataURL(blob); });
    return result;
  }, sources);
  await writeFile(`${stem}-diff.png`, Buffer.from(result.heat.split(',')[1], 'base64'));
  delete result.heat;
  await page.close();
  return result;
}
