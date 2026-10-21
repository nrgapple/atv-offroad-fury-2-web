// Parity captures for the shared Rust engine. Renders one venue from the same
// camera in (1) the current JavaScript game, (2) the Rust engine in the browser
// (WebGL2) and (3) the native Rust build, and writes the three PNGs.
//
//   node tools/compare_engine.mjs [venue] [out-dir]
//   node tools/compare_engine.mjs ride [venue] [script.json] [out-dir]
// Ride mode replays one scripted control sequence through the JavaScript
// game's own loop (its op 50 batches carry the script's input), the native
// headless replay (`fury --trace`) and the Rust browser build, then compares
// vehicle pose and camera step by step and captures the held final frame at
// landscape and portrait sizes.
// Environment: ATV_TEST_URL (served site with private content), FURY_BIN
// (native binary; skipped when unset) and FURY_CONTENT (package directory).
// Outputs belong in ignored private storage; they contain game imagery.
import { execFileSync } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { assertNoErrors, launch, newPage, url, waitForGame } from './browser/harness.mjs';


// Default script: settle, accelerate, turn both ways, brake into reverse.
const DEFAULT_SCRIPT = [
  { frames: 60, input: {} },
  { frames: 180, input: { keyboard_drive: { held: true, axis: 1 } } },
  { frames: 120, input: { steer: -1, keyboard_drive: { held: true, axis: 1 } } },
  { frames: 120, input: { steer: 1, keyboard_drive: { held: true, axis: 1 } } },
  { frames: 120, input: { keyboard_drive: { held: true, axis: -1 } } },
];
const PORTRAIT = { width: 390, height: 844 };

// Rewrites the game's own op 50 batches so each step carries the script's
// input; once the script is exhausted the next batch is never sent, holding
// the final state on screen.
async function scriptedGame(browser, venue, steps, viewport, errors) {
  const page = await newPage(browser, { viewport }, errors);
  await page.addInitScript(({ steps }) => {
    // The Original graphics preset, without the first-ride keyboard panel.
    localStorage.setItem('atv-graphics', 'original');
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    const trace = window.rideTrace = { frames: [], cursor: 0, held: false };
    const post = Worker.prototype.postMessage, pending = new Set();
    Worker.prototype.postMessage = function (message, ...rest) {
      if (message?.op !== 50 || !message.bytes) return post.call(this, message, ...rest);
      if (!this.rideListener) {
        this.rideListener = true;
        this.addEventListener('message', ({ data }) => {
          if (!pending.delete(data.id) || data.error || !data.bytes) return;
          for (const { frame, camera } of JSON.parse(new TextDecoder().decode(data.bytes))) {
            trace.frames.push({ step: trace.frames.length + 1, origin: frame.origin, orientation: frame.orientation, speed: frame.speed,
              wheel_compression: frame.wheel_compression, grounded: frame.grounded, phase: frame.wreck.phase, reset_count: frame.reset_count, camera });
          }
        });
      }
      if (trace.cursor >= steps.length) { trace.held = true; return; }
      const request = JSON.parse(new TextDecoder().decode(message.bytes));
      const same = JSON.stringify(steps[trace.cursor]);
      let count = 1;
      while (count < request.count && trace.cursor + count < steps.length && JSON.stringify(steps[trace.cursor + count]) === same) count++;
      const keep = Object.fromEntries(['funny_crashes', 'crash_mode', 'flips', 'arcade_flips', 'realistic_train'].map(key => [key, request.input[key]]));
      Object.assign(request, { count, orbit: 0, cycle: false,
        input: { throttle: 0, brake: 0, steer: 0, lean: 0, reverse: false, input_generation: 0, ...keep, ...steps[trace.cursor] } });
      trace.cursor += count; pending.add(message.id);
      return post.call(this, { ...message, bytes: new TextEncoder().encode(JSON.stringify(request)) }, ...rest);
    };
  }, { steps });
  await page.goto(url(`/play.html?drive=1&environment=${venue}`));
  await waitForGame(page);
  await page.waitForFunction(() => window.rideTrace.held || /stopped/.test(document.querySelector('#status').textContent), null, { timeout: 120000 });
  const stopped = await page.locator('#status').textContent();
  if (!await page.evaluate(() => window.rideTrace.held)) throw new Error(`JavaScript game: ${stopped}`);
  await page.waitForTimeout(500);
  return page;
}

// Mean absolute RGB difference (0-255) over the frame and a centre crop.
async function difference(browser, a, b, crop = [0.35, 0.35, 0.65, 0.85]) {
  const page = await browser.newPage();
  const data = async file => `data:image/png;base64,${(await readFile(file)).toString('base64')}`;
  const result = await page.evaluate(async ([a, b, crop]) => {
    const pixels = async src => { const image = new Image(); image.src = src; await image.decode();
      const canvas = new OffscreenCanvas(image.width, image.height), context = canvas.getContext('2d');
      context.drawImage(image, 0, 0); return context.getImageData(0, 0, image.width, image.height); };
    const [x, y] = [await pixels(a), await pixels(b)];
    if (x.width !== y.width || x.height !== y.height) return null;
    const mean = (x0, y0, x1, y1) => { let sum = 0, n = 0;
      for (let row = Math.floor(y0 * x.height); row < y1 * x.height; row++) for (let col = Math.floor(x0 * x.width); col < x1 * x.width; col++) {
        const i = (row * x.width + col) * 4; for (let c = 0; c < 3; c++) sum += Math.abs(x.data[i + c] - y.data[i + c]); n += 3; }
      return sum / n; };
    return { frame: mean(0, 0, 1, 1), vehicle: mean(...crop) };
  }, [await data(a), await data(b), crop]);
  await page.close();
  return result;
}

function compareTraces(js, rust) {
  const distance = (a, b) => Math.hypot(...a.map((v, i) => v - b[i]));
  let worst = { origin: 0, orientation: 0, camera: 0, step: 0 }, first = null;
  const steps = Math.min(js.length, rust.length - 1);
  for (let i = 0; i < steps; i++) {
    const a = js[i], b = rust[i + 1];
    const origin = distance(a.origin, b.origin), orientation = distance(a.orientation, b.orientation);
    const camera = Math.max(distance(a.camera.position, b.camera.position), distance(a.camera.target, b.camera.target));
    if (first === null && (origin > 1e-3 || orientation > 1e-4)) first = a.step;
    if (origin > worst.origin) worst = { ...worst, origin, step: a.step };
    worst.orientation = Math.max(worst.orientation, orientation); worst.camera = Math.max(worst.camera, camera);
  }
  return { steps, firstDivergence: first, ...worst, phases: [js.at(-1)?.phase, rust.at(-1)?.phase] };
}

async function rideMode() {
  const venue = process.argv[3] ?? 'env01';
  const script = process.argv[4] ? JSON.parse(await readFile(process.argv[4], 'utf8')) : DEFAULT_SCRIPT;
  const out = path.resolve(process.argv[5] ?? 'private/reports/engine-parity/ride');
  await mkdir(out, { recursive: true });
  const scriptFile = path.join(out, 'script.json');
  await writeFile(scriptFile, JSON.stringify(script));
  // The worker's intent decoder requires a generation; the Rust replay defaults it to 0.
  const normalized = input => input?.keyboard_drive ? { ...input, keyboard_drive: { generation: 0, ...input.keyboard_drive } } : input ?? {};
  const steps = script.flatMap(({ frames, input }) => Array.from({ length: frames }, () => normalized(input)));
  const browser = await launch();
  const report = { venue, steps: steps.length };
  try {
    // Screen boxes around the held quad and rider of the default script.
    for (const [name, viewport, crop] of [['landscape', { width: 1280, height: 720 }, [0.43, 0.37, 0.59, 0.69]], ['portrait', PORTRAIT, [0.33, 0.49, 0.7, 0.7]]]) {
      const errors = [];
      const page = await scriptedGame(browser, venue, steps, viewport, errors);
      const jsFile = path.join(out, `${venue}-${name}-javascript.png`);
      report[`javascriptGraphics-${name}`] = await page.evaluate(() => atvTerrainStatus.graphics?.setting);
      // The canvas only: toolbar, HUD and minimap are DOM overlays.
      await page.evaluate(() => { document.body.style.visibility = 'hidden'; document.querySelector('#game').style.visibility = 'visible'; });
      await page.locator('#game').screenshot({ path: jsFile });
      if (name === 'landscape') {
        const trace = await page.evaluate(() => window.rideTrace.frames);
        await writeFile(path.join(out, 'javascript-trace.json'), JSON.stringify(trace));
        report.javascriptSteps = trace.length;
        report.javascriptCamera = await page.evaluate(() => window.atvTerrainStatus.renderCamera);
      }
      assertNoErrors(errors);
      await page.close();

      const rustErrors = [];
      const rust = await newPage(browser, { viewport }, rustErrors);
      await rust.goto(url(`/engine.html?venue=${venue}&script=${encodeURIComponent(JSON.stringify(script))}`));
      await rust.waitForFunction(() => document.querySelector('#trace') && /fps/.test(document.querySelector('#status').textContent)
        || /fail|error|stopped/i.test(document.querySelector('#status').textContent), null, { timeout: 120000 });
      await rust.waitForTimeout(1000);
      report[`rustWeb-${name}`] = await rust.locator('#status').textContent();
      if (name === 'landscape') report.rustWebFinal = JSON.parse(await rust.locator('#trace').textContent());
      await rust.evaluate(() => { document.querySelector('#status').hidden = true; });
      const webFile = path.join(out, `${venue}-${name}-rust-web.png`);
      await rust.locator('canvas').screenshot({ path: webFile });
      assertNoErrors(rustErrors);
      await rust.close();
      report[`difference-${name}-web`] = await difference(browser, jsFile, webFile, crop);

      if (process.env.FURY_BIN) {
        const nativeFile = path.join(out, `${venue}-${name}-rust-native.png`);
        const content = process.env.FURY_CONTENT ? ['--content', process.env.FURY_CONTENT] : [];
        execFileSync(process.env.FURY_BIN, [...content, '--venue', venue, '--script', scriptFile, '--capture', nativeFile, `${viewport.width}x${viewport.height}`], { stdio: 'inherit' });
        report[`difference-${name}-native`] = await difference(browser, jsFile, nativeFile, crop);
      }
    }
    if (process.env.FURY_BIN) {
      const traceFile = path.join(out, 'rust-native-trace.json');
      const content = process.env.FURY_CONTENT ? ['--content', process.env.FURY_CONTENT] : [];
      execFileSync(process.env.FURY_BIN, [...content, '--venue', venue, '--script', scriptFile, '--trace', traceFile], { stdio: 'inherit' });
      const js = JSON.parse(await readFile(path.join(out, 'javascript-trace.json'), 'utf8'));
      const native = JSON.parse(await readFile(traceFile, 'utf8'));
      report.native = compareTraces(js, native);
      const final = native.at(-1), web = report.rustWebFinal;
      report.webVersusNative = { origin: Math.hypot(...web.origin.map((v, i) => v - final.origin[i])), step: [web.step, final.step] };
    }
    await writeFile(path.join(out, 'report.json'), JSON.stringify(report, null, 2));
    console.log(JSON.stringify(report, null, 2));
    console.log(`PASS: wrote ${venue} ride parity to ${out}`);
  } finally {
    await browser.close();
  }
}

if (process.argv[2] === 'ride') {
  await rideMode();
  process.exit(0);
}

const venue = process.argv[2] ?? 'env01';
const out = path.resolve(process.argv[3] ?? 'private/reports/engine-parity');
const viewport = { width: 1280, height: 720 };
await mkdir(out, { recursive: true });
const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport }, errors);
  await page.goto(url(`/play.html?drive=1&environment=${venue}`));
  await waitForGame(page);
  // Let the start camera settle before sampling it.
  await page.waitForTimeout(2500);
  const camera = await page.evaluate(() => atvTerrainStatus.renderCamera);
  await page.locator('#game').screenshot({ path: path.join(out, `${venue}-javascript.png`) });
  assertNoErrors(errors);
  const view = [camera.x, camera.y, camera.z, camera.yaw, camera.pitch, camera.focalLength].map(v => Number(v.toFixed(6))).join(',');
  console.log(`camera ${view}`);

  const rustErrors = [];
  const rust = await newPage(browser, { viewport }, rustErrors);
  await rust.goto(url(`/engine.html?venue=${venue}&camera=${view}`));
  await rust.waitForFunction(() => /fps/.test(document.querySelector('#status').textContent) || /fail|error/i.test(document.querySelector('#status').textContent), null, { timeout: 120000 });
  await rust.waitForTimeout(2000);
  const status = await rust.locator('#status').textContent();
  console.log(`rust web: ${status}`);
  await rust.evaluate(() => { document.querySelector('#status').hidden = true; });
  await rust.locator('canvas').screenshot({ path: path.join(out, `${venue}-rust-web.png`) });
  assertNoErrors(rustErrors);

  if (process.env.FURY_BIN) {
    const target = path.join(out, `${venue}-rust-native.png`);
    execFileSync(process.env.FURY_BIN, ['--venue', venue, '--camera', view, '--capture', target, `${viewport.width}x${viewport.height}`], { stdio: 'inherit' });
  }
  console.log(`PASS: wrote ${venue} parity captures to ${out}`);
} finally {
  await browser.close();
}
