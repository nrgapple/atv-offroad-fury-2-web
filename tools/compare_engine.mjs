// Parity captures for the shared Rust engine. Renders one venue from the same
// camera in (1) the current JavaScript game, (2) the Rust engine in the browser
// (WebGL2) and (3) the native Rust build, writes the PNGs and reports the mean
// absolute difference per screen region.
//
//   node tools/compare_engine.mjs [venue] [out-dir] [--view x,y,z,yaw,pitch,focal]...
//        [--graphics original|enhanced|high] [--actors] [--race]
//   node tools/compare_engine.mjs ride [venue] [script.json] [out-dir] [--graphics ...] [--crash original|crazy|gorn]
//        [--region x0,y0,x1,y1]
// Static mode: without --view the JavaScript start camera is used. The
// JavaScript game runs with the requested preset (Original by default), its
// actors hidden (unless --actors; the Rust builds then ride too) and its scene
// clocks held while capturing (window.atvParityView); the Rust builds receive
// the same camera, Weather outcome and clock steps.
// Ride mode replays one scripted control sequence through the JavaScript
// game's own loop (its op 50 batches carry the script's input), the native
// headless replay (`fury --trace`) and the Rust browser build, then compares
// vehicle pose and camera step by step and captures the held final frame at
// landscape and portrait sizes.
// Ride and static Rust browser captures load with `?ui=0` (scene only: no
// toolbar, HUD, minimap or keyboard card), matching the JavaScript canvas
// capture that leaves out its DOM overlays; race captures keep the HUD.
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
async function scriptedGame(browser, venue, steps, viewport, errors, { measure = false, graphics = 'original', crash = 'original', race = null, quad = 1, appearance = null } = {}) {
  const page = await newPage(browser, { viewport }, errors);
  await page.addInitScript(({ steps, measure, graphics, crash, race, appearance }) => {
    // The Garage appearance (rider gear and quad paint) the ride is dressed in.
    if (appearance) localStorage.setItem('atv-player-selection', JSON.stringify({ appearance }));
    // CPU baseline: animation frames and worker operation time (worker.js
    // reports each operation's Wasm time).
    const cpu = window.rideCpu = { frames: 0, workerMs: 0 };
    const raf = window.requestAnimationFrame.bind(window);
    window.requestAnimationFrame = callback => raf(time => { cpu.frames++; callback(time); });
    if (measure) localStorage.setItem('atv-audio-v1', JSON.stringify({ sfx: 1, mode: 'original', silent: 'play' }));
    // The requested graphics preset (Original by default), without the
    // first-ride keyboard panel.
    localStorage.setItem('atv-graphics', graphics);
    localStorage.setItem('atv-crash-mode', crash);
    // The original crash camera (the Rust engine has not ported the authored tight one).
    localStorage.setItem('atv-crash-camera', 'original');
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    const trace = window.rideTrace = { frames: [], cursor: 0, held: false, lifts: [] };
    // Enhanced/High wheel re-projection: the lift drawn after each delivered step.
    if (graphics !== 'original') {
      const lift = () => { const g = window.atvTerrainStatus?.graphics, step = trace.frames.length; if (g && step && !trace.held && trace.lifts.at(-1)?.[0] !== step) trace.lifts.push([step, g.wheelLift, g.dirtLevel]); requestAnimationFrame(lift); };
      requestAnimationFrame(lift);
    }
    const post = Worker.prototype.postMessage, pending = new Set();
    Worker.prototype.postMessage = function (message, ...rest) {
      // Race parity: a fixed roster seed and no weather (both drawn per load).
      if (race && [105, 120].includes(message?.op) && message.bytes) {
        const request = JSON.parse(new TextDecoder().decode(message.bytes));
        if (message.op === 105) request.seed = race.seed; else request.random = 0x7fffffff;
        return post.call(this, { ...message, bytes: new TextEncoder().encode(JSON.stringify(request)) }, ...rest);
      }
      if (message?.op !== 50 || !message.bytes) return post.call(this, message, ...rest);
      if (!this.rideListener) {
        this.rideListener = true;
        this.addEventListener('message', ({ data }) => { if (Number.isFinite(data.operationMs)) cpu.workerMs += data.operationMs; });
        this.addEventListener('message', ({ data }) => {
          if (!pending.delete(data.id) || data.error || !data.bytes) return;
          for (const { frame, camera, race: state, flashes } of JSON.parse(new TextDecoder().decode(data.bytes))) {
            trace.frames.push({ step: trace.frames.length + 1, origin: frame.origin, orientation: frame.orientation, speed: frame.speed,
              wheel_compression: frame.wheel_compression, grounded: frame.grounded, phase: frame.wreck.phase, reset_count: frame.reset_count, camera,
              gore: frame.funny_crash && [frame.funny_crash.severed_nodes.length, (frame.funny_crash.blood?.drops?.length ?? 0) / 7, (frame.funny_crash.blood?.splats?.length ?? 0) / 10, frame.funny_crash.rider_impact],
              race: state && { phase: state.phase, elapsed: state.elapsed, lap: state.lap, position: state.position, countdown: state.countdown,
                contacts: state.contacts, off_track: state.off_track, opponents: state.opponents.map(o => ({ origin: o.origin, distance: o.distance, finished: o.finished })),
                standings: state.standings.map(s => ({ name: s.name, time: s.time, position: s.position })),
                // Presentation (compared with the Rust trace's events).
                lights: state.lights ?? null, crowd: state.crowd ?? null, gate: state.gate ?? null, flashes: flashes ?? null } });
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
  }, { steps, measure, graphics, crash, race, appearance });
  const session = measure ? await page.context().newCDPSession(page) : null;
  if (session) await session.send('Performance.enable');
  await page.goto(url(race ? `/play.html?activity=race&map=${venue}&laps=${race.laps}&cpu=${race.cpu}&quad=${quad}&play=1` : `/play.html?drive=1&environment=${venue}&quad=${quad}`));
  await waitForGame(page);
  let start;
  if (session) {
    // A click unlocks audio as a player's first input does.
    await page.mouse.click(viewport.width / 2, viewport.height / 2);
    await page.waitForFunction(() => window.rideTrace.cursor > 0, null, { timeout: 60000 });
    start = await sample(page, session);
  }
  await page.waitForFunction(() => window.rideTrace.held || /stopped/.test(document.querySelector("#status").textContent), null, { timeout: 120000 + steps.length / 60 * 1500 });
  const stopped = await page.locator('#status').textContent();
  if (!await page.evaluate(() => window.rideTrace.held)) throw new Error(`JavaScript game: ${stopped}`);
  if (session) page.cpu = cost(start, await sample(page, session));
  await page.waitForTimeout(500);
  return page;
}

// Main-thread task time (Chrome's TaskDuration) and frames for a page.
async function sample(page, session) {
  const { metrics } = await session.send('Performance.getMetrics');
  const value = name => metrics.find(m => m.name === name)?.value ?? 0;
  const page_ = await page.evaluate(() => ({ ...(window.rideCpu ?? {}), now: performance.now() }));
  return { task: value('TaskDuration') * 1000, script: value('ScriptDuration') * 1000, frames: page_.frames ?? 0, workerMs: page_.workerMs ?? 0, now: page_.now };
}
function cost(a, b) {
  const frames = Math.max(1, b.frames - a.frames), seconds = (b.now - a.now) / 1000;
  return { frames, seconds, fps: frames / seconds, mainThreadMsPerFrame: (b.task - a.task) / frames, scriptMsPerFrame: (b.script - a.script) / frames,
    workerMsPerFrame: (b.workerMs - a.workerMs) / frames, totalMsPerFrame: (b.task - a.task + b.workerMs - a.workerMs) / frames,
    // Headless frames are not vsync-capped: CPU per second of riding,
    // expressed per 60 Hz frame, compares builds at different frame rates.
    totalMsPer60HzFrame: (b.task - a.task + b.workerMs - a.workerMs) / seconds / 60 };
}

// The same script in real time: the JavaScript game (main thread plus its
// simulation worker), the Rust engine in the browser (one thread) and the
// native build. GPU process time is excluded everywhere.
async function cpuBaseline(venue, script, steps, scriptFile, quad = 1) {
  const browser = await launch({ args: ['--autoplay-policy=no-user-gesture-required'] });
  const viewport = { width: 1280, height: 720 };
  try {
    const errors = [];
    const page = await scriptedGame(browser, venue, steps, viewport, errors, { measure: true, quad });
    const javascript = page.cpu;
    await page.close();
    const rust = await newPage(browser, { viewport }, []);
    await rust.addInitScript(() => {
      const cpu = window.rideCpu = { frames: 0 };
      const raf = window.requestAnimationFrame.bind(window);
      window.requestAnimationFrame = callback => raf(time => { cpu.frames++; callback(time); });
    });
    const session = await rust.context().newCDPSession(rust);
    await session.send('Performance.enable');
    await rust.goto(url(`/engine.html?venue=${venue}&quad=${quad}&autodrive=${encodeURIComponent(JSON.stringify(script))}`));
    await rust.waitForFunction(() => /fps/.test(document.querySelector('#status').textContent), null, { timeout: 120000 });
    await rust.mouse.click(viewport.width / 2, viewport.height / 2);
    const start = await sample(rust, session);
    await rust.waitForFunction(() => document.querySelector('#trace'), null, { timeout: 120000 });
    const web = { ...cost(start, await sample(rust, session)), engine: JSON.parse(await rust.locator('#trace').textContent()) };
    await rust.close();
    let native = null;
    if (process.env.FURY_BIN) {
      const bench = scriptFile.replace(/\.json$/, '-bench.json');
      const content = process.env.FURY_CONTENT ? ['--content', process.env.FURY_CONTENT] : [];
      execFileSync(process.env.FURY_BIN, [...content, '--venue', venue, '--quad', String(quad), '--autodrive', scriptFile, '--bench', bench], { stdio: 'inherit' });
      native = JSON.parse(await readFile(bench, 'utf8'));
    }
    assertNoErrors(errors);
    return { javascript, rustWeb: web, rustNative: native };
  } finally {
    await browser.close();
  }
}

// Mean absolute RGB difference (0-255) over the frame and a centre crop.
async function difference(browser, a, b, crop = [0.35, 0.35, 0.65, 0.85], region = null) {
  const page = await browser.newPage();
  const data = async file => `data:image/png;base64,${(await readFile(file)).toString('base64')}`;
  const result = await page.evaluate(async ([a, b, crop, region]) => {
    const pixels = async src => { const image = new Image(); image.src = src; await image.decode();
      const canvas = new OffscreenCanvas(image.width, image.height), context = canvas.getContext('2d');
      context.drawImage(image, 0, 0); return context.getImageData(0, 0, image.width, image.height); };
    const [x, y] = [await pixels(a), await pixels(b)];
    if (x.width !== y.width || x.height !== y.height) return null;
    const mean = (x0, y0, x1, y1) => { let sum = 0, n = 0;
      for (let row = Math.floor(y0 * x.height); row < y1 * x.height; row++) for (let col = Math.floor(x0 * x.width); col < x1 * x.width; col++) {
        const i = (row * x.width + col) * 4; for (let c = 0; c < 3; c++) sum += Math.abs(x.data[i + c] - y.data[i + c]); n += 3; }
      return sum / n; };
    return { frame: mean(0, 0, 1, 1), vehicle: mean(...crop), ...(region && { region: mean(...region) }) };
  }, [await data(a), await data(b), crop, region]);
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

// JavaScript wheel lifts sampled per animation frame ([step, lift], possibly
// one step stale) against the Rust browser trace's per-step `wheelLift`.
function compareLifts(lifts, rust) {
  const at = new Map(rust.map(r => [r.step, r.wheelLift ?? 0]));
  let matched = 0, lifted = 0, worst = 0;
  for (const [step, lift] of lifts) {
    if (lift > 0) lifted++;
    const error = Math.min(...[step, step - 1].filter(s => at.has(s)).map(s => Math.abs(at.get(s) - lift)));
    if (error < 1e-3) matched++; else worst = Math.max(worst, error);
  }
  const rustLifted = rust.filter(r => r.wheelLift > 0).length;
  return { samples: lifts.length, matched, javascriptLifted: lifted, rustLiftedSteps: rustLifted, worstUnmatched: worst };
}

// Enhanced/High ATV dirt: the JavaScript `dirtLevel` samples against the Rust
// trace's per-step `dirtLevel` (same staleness allowance as the lifts).
function compareDirt(samples, rust) {
  const at = new Map(rust.map(r => [r.step, r.dirtLevel ?? 0]));
  let matched = 0, worst = 0;
  for (const [step, , level] of samples) {
    const error = Math.min(...[step, step - 1].filter(s => at.has(s)).map(s => Math.abs(at.get(s) - level)));
    if (error < 1e-6) matched++; else worst = Math.max(worst, error);
  }
  return { samples: samples.length, matched, worstUnmatched: worst, javascriptFinal: samples.at(-1)?.[2], rustFinal: rust.at(-1)?.dirtLevel };
}

async function rideMode() {
  // ride [venue] [script.json] [out-dir] [--graphics original|enhanced|high] [--crash original|crazy|gorn] [--region x0,y0,x1,y1] [--quad 1-46]
  const args = process.argv.slice(3), at = args.indexOf('--graphics');
  const graphics = at >= 0 ? args.splice(at, 2)[1] : 'original';
  const crashAt = args.indexOf('--crash');
  const crash = crashAt >= 0 ? args.splice(crashAt, 2)[1] : 'original';
  // --region x0,y0,x1,y1: an extra landscape box (fractions), e.g. around crash blood.
  const regionAt = args.indexOf('--region');
  const region = regionAt >= 0 ? args.splice(regionAt, 2)[1].split(',').map(Number) : null;
  // --quad: the Garage selection (27-46 are the motorcycles).
  const quadAt = args.indexOf('--quad');
  const quad = quadAt >= 0 ? Number(args.splice(quadAt, 2)[1]) : 1;
  // --appearance 1.2.0.3.3.0.1:4: Garage gear indices and quad paint.
  const appearanceAt = args.indexOf('--appearance');
  const [gearText, paintText] = appearanceAt >= 0 ? args.splice(appearanceAt, 2)[1].split(':') : [];
  const appearance = gearText ? { gear: gearText.split('.').map(Number), paint: Number(paintText ?? 0) } : null;
  const dressWeb = appearance ? `&gear=${appearance.gear.join('.')}&paint=${appearance.paint}` : '';
  const dressNative = appearance ? ['--gear', appearance.gear.join(','), '--paint', String(appearance.paint)] : [];
  const venue = args[0] || 'env01';
  const script = args[1] ? JSON.parse(await readFile(args[1], 'utf8')) : DEFAULT_SCRIPT;
  const out = path.resolve(args[2] ?? 'private/reports/engine-parity/ride');
  await mkdir(out, { recursive: true });
  const scriptFile = path.join(out, 'script.json');
  await writeFile(scriptFile, JSON.stringify(script));
  // The worker's intent decoder requires a generation; the Rust replay defaults it to 0.
  const normalized = input => input?.keyboard_drive ? { ...input, keyboard_drive: { generation: 0, ...input.keyboard_drive } } : input ?? {};
  const steps = script.flatMap(({ frames, input }) => Array.from({ length: frames }, () => normalized(input)));
  const browser = await launch();
  const report = { venue, quad, appearance, graphics, crash, steps: steps.length };
  let lifts = [];
  try {
    // Screen boxes around the held quad and rider of the default script.
    for (const [name, viewport, crop] of [['landscape', { width: 1280, height: 720 }, [0.43, 0.37, 0.59, 0.69]], ['portrait', PORTRAIT, [0.33, 0.49, 0.7, 0.7]]]) {
      const errors = [];
      const page = await scriptedGame(browser, venue, steps, viewport, errors, { graphics, crash, quad, appearance });
      const jsFile = path.join(out, `${venue}-${name}-javascript.png`);
      report[`javascriptGraphics-${name}`] = await page.evaluate(() => atvTerrainStatus.graphics?.setting);
      report[`javascriptSpeedRush-${name}`] = await page.evaluate(() => atvTerrainStatus.speedRush);
      report[`javascriptEffects-${name}`] = await page.evaluate(() => ({ crash: { mode: atvTerrainStatus.crashMode, severed: atvTerrainStatus.frame?.funny_crash?.severed_nodes, parts: atvTerrainStatus.frame?.funny_crash?.detached_parts, impact: atvTerrainStatus.frame?.funny_crash?.rider_impact }, blood: atvTerrainStatus.blood, wheel: atvTerrainStatus.wheelEffects && { particles: atvTerrainStatus.wheelEffects.particles, draws: atvTerrainStatus.wheelEffects.draws, wetLive: atvTerrainStatus.wheelEffects.wetLive }, lingeringDust: atvTerrainStatus.lingeringDust }));
      // The canvas only: toolbar, HUD and minimap are DOM overlays.
      await page.evaluate(() => { document.body.style.visibility = 'hidden'; document.querySelector('#game').style.visibility = 'visible'; });
      await page.locator('#game').screenshot({ path: jsFile });
      if (name === 'landscape') {
        const trace = await page.evaluate(() => window.rideTrace.frames);
        await writeFile(path.join(out, 'javascript-trace.json'), JSON.stringify(trace));
        report.javascriptSteps = trace.length;
        lifts = await page.evaluate(() => window.rideTrace.lifts);
        report.javascriptCamera = await page.evaluate(() => window.atvTerrainStatus.renderCamera);
      }
      assertNoErrors(errors);
      await page.close();

      const rustErrors = [];
      const rust = await newPage(browser, { viewport }, rustErrors);
      await rust.goto(url(`/engine.html?venue=${venue}&quad=${quad}${dressWeb}&graphics=${graphics}&crash=${crash}&ui=0&script=${encodeURIComponent(JSON.stringify(script))}`));
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
      const box = name === 'landscape' ? region : null;
      report[`difference-${name}-web`] = await difference(browser, jsFile, webFile, crop, box);

      if (process.env.FURY_BIN) {
        const nativeFile = path.join(out, `${venue}-${name}-rust-native.png`);
        const content = process.env.FURY_CONTENT ? ['--content', process.env.FURY_CONTENT] : [];
        execFileSync(process.env.FURY_BIN, [...content, '--venue', venue, '--quad', String(quad), ...dressNative, '--graphics', graphics, '--crash', crash, '--script', scriptFile, '--capture', nativeFile, `${viewport.width}x${viewport.height}`], { stdio: 'inherit' });
        report[`difference-${name}-native`] = await difference(browser, jsFile, nativeFile, crop, box);
      }
    }
    // Every step of the Rust browser build (`traceAll`, script from storage).
    const js = JSON.parse(await readFile(path.join(out, 'javascript-trace.json'), 'utf8'));
    {
      const rustErrors = [];
      const rust = await newPage(browser, { viewport: { width: 640, height: 360 } }, rustErrors);
      await rust.addInitScript(text => localStorage.setItem('furyScript', text), JSON.stringify(script));
      await rust.goto(url(`/engine.html?venue=${venue}&quad=${quad}&graphics=${graphics}&weather=off&traceAll=1&script=storage`));
      await rust.waitForFunction(() => document.querySelector('#trace') || /fail|error|stopped/i.test(document.querySelector('#status').textContent), null, { timeout: 180000 });
      const webTrace = JSON.parse(await rust.locator('#trace').textContent());
      await writeFile(path.join(out, 'rust-web-trace.json'), JSON.stringify(webTrace));
      // traceAll records start at step 1; compareTraces expects step 0 first.
      report.web = compareTraces(js, webTrace[0]?.step === 0 ? webTrace : [null, ...webTrace]);
      if (lifts.length) report.wheelLift = compareLifts(lifts, webTrace);
      if (lifts.length) report.dirt = compareDirt(lifts, webTrace);
      assertNoErrors(rustErrors);
      await rust.close();
    }
    if (process.env.FURY_BIN) {
      const traceFile = path.join(out, 'rust-native-trace.json');
      const content = process.env.FURY_CONTENT ? ['--content', process.env.FURY_CONTENT] : [];
      execFileSync(process.env.FURY_BIN, [...content, '--venue', venue, '--quad', String(quad), '--script', scriptFile, '--trace', traceFile], { stdio: 'inherit' });
      const native = JSON.parse(await readFile(traceFile, 'utf8'));
      report.native = compareTraces(js, native);
      const final = native.at(-1), web = report.rustWebFinal;
      report.webVersusNative = { origin: Math.hypot(...web.origin.map((v, i) => v - final.origin[i])), step: [web.step, final.step] };
    }
    report.cpu = await cpuBaseline(venue, script, steps, scriptFile, quad);
    await writeFile(path.join(out, 'report.json'), JSON.stringify(report, null, 2));
    console.log(JSON.stringify(report, null, 2));
    console.log(`PASS: wrote ${venue} ride parity to ${out}`);
  } finally {
    await browser.close();
  }
}

// Race parity: one scripted player through the JavaScript race and the
// native race replay; compares phases, player and CPU positions per step.
//   node tools/compare_engine.mjs race [venue] [script.json] [out-dir] [laps] [cpu] [seed] [quad]
async function raceMode() {
  const venue = process.argv[3] ?? 'env01';
  const script = process.argv[4] ? JSON.parse(await readFile(process.argv[4], 'utf8'))
    : [{ frames: 1800, input: { keyboard_drive: { held: true, axis: 1 } } }];
  const out = path.resolve(process.argv[5] ?? 'private/reports/engine-parity/race');
  const race = { laps: Number(process.argv[6] ?? 1), cpu: Number(process.argv[7] ?? 3), seed: Number(process.argv[8] ?? 12345) };
  // The Garage selection (27-46: a motorcycle against the CPU quads).
  const quad = Number(process.argv[9] ?? 1);
  await mkdir(out, { recursive: true });
  const scriptFile = path.join(out, 'script.json');
  await writeFile(scriptFile, JSON.stringify(script));
  const normalized = input => input?.keyboard_drive ? { ...input, keyboard_drive: { generation: 0, ...input.keyboard_drive } } : input ?? {};
  const steps = script.flatMap(({ frames, input }) => Array.from({ length: frames }, () => normalized(input)));
  const browser = await launch();
  const errors = [];
  try {
    if (process.env.RACE_AUTOPILOT) {
      // Record the race path driver riding the player in the Rust browser
      // build (whose physics matches the JavaScript game's Wasm exactly),
      // and use its controls as the script: a player who finishes.
      const recorder = await newPage(browser, { viewport: { width: 640, height: 360 } }, errors);
      const frames = Number(process.env.RACE_AUTOPILOT);
      await recorder.addInitScript(script => localStorage.setItem('furyScript', script), JSON.stringify([{ frames, input: {} }]));
      await recorder.goto(url(`/engine.html?venue=${venue}&race=1&laps=${race.laps}&cpu=${race.cpu}&seed=${race.seed}&quad=${quad}&weather=off&traceAll=1&autopilot=1&script=storage`));
      await recorder.waitForFunction(() => document.querySelector('#trace'), null, { timeout: 300000 });
      const recorded = JSON.parse(await recorder.locator('#trace').textContent());
      script.length = 0;
      for (const record of recorded) {
        const [throttle, brake, steer, lean] = record.imposed ?? [0, 0, 0, 0];
        const input = { throttle, brake, steer, lean };
        const last = script.at(-1);
        if (last && JSON.stringify(last.input) === JSON.stringify(input)) last.frames++; else script.push({ frames: 1, input });
      }
      await recorder.close();
      await writeFile(scriptFile, JSON.stringify(script));
      steps.splice(0, steps.length, ...script.flatMap(({ frames, input }) => Array.from({ length: frames }, () => input)));
    }
    // RACE_VIEWPORT=390x844: the JavaScript capture's viewport (race HUD layouts).
    const [width, height] = (process.env.RACE_VIEWPORT ?? '1280x720').split('x').map(Number);
    const page = await scriptedGame(browser, venue, steps, { width, height }, errors, { race, quad });
    const js = await page.evaluate(() => window.rideTrace.frames);
    await page.locator('#game').screenshot({ path: path.join(out, `${venue}-race-javascript.png`) });
    await writeFile(path.join(out, 'javascript-trace.json'), JSON.stringify(js));
    assertNoErrors(errors);
    const traceFile = path.join(out, 'rust-native-trace.json');
    const content = process.env.FURY_CONTENT ? ['--content', process.env.FURY_CONTENT] : [];
    execFileSync(process.env.FURY_BIN, [...content, '--venue', venue, '--quad', String(quad), '--race', '--laps', String(race.laps), '--cpu', String(race.cpu),
      '--seed', String(race.seed), '--script', scriptFile, '--trace', traceFile], { stdio: 'inherit' });
    const rust = JSON.parse(await readFile(traceFile, 'utf8'));
    // The Rust engine in the browser, every step (?traceAll=1).
    const web = await newPage(browser, { viewport: { width: 640, height: 360 } }, errors);
    // Long scripts exceed URL limits: hand them over through localStorage.
    await web.addInitScript(script => localStorage.setItem('furyScript', script), JSON.stringify(script));
    await web.goto(url(`/engine.html?venue=${venue}&race=1&laps=${race.laps}&cpu=${race.cpu}&seed=${race.seed}&quad=${quad}&weather=off&traceAll=1&script=storage`));
    await web.waitForFunction(() => document.querySelector('#trace') || /error|fail/i.test(document.querySelector('#status')?.textContent), null, { timeout: 300000 });
    const webTrace = [{ step: 0 }, ...JSON.parse(await web.locator('#trace').textContent())];
    assertNoErrors(errors);
    // Traces print binary32 values at different precision: compare as binary32.
    const distance = (a, b) => Math.hypot(...a.map((v, i) => Math.fround(v) - Math.fround(b[i])));
    const phases = trace => { const list = []; for (const f of trace) { const p = f.race?.phase ?? null; if (list.at(-1)?.[0] !== p) list.push([p, f.step]); } return list; };
    const versus = other => {
      let first = null, worstPlayer = 0, worstCpu = 0;
      const count = Math.min(js.length, other.length - 1);
      for (let i = 0; i < count; i++) {
        const a = js[i], b = other[i + 1];
        const player = distance(a.origin, b.origin);
        const cpu = Math.max(0, ...(a.race?.opponents ?? []).map((o, k) => distance(o.origin, b.race.opponents[k].origin)));
        worstPlayer = Math.max(worstPlayer, player); worstCpu = Math.max(worstCpu, cpu);
        if (first === null && (player > 0 || cpu > 0)) first = { step: a.step, player, cpu };
      }
      return { steps: count, firstDifference: first, worstPlayerFt: worstPlayer, worstCpuFt: worstCpu };
    };
    const count = Math.min(js.length, rust.length - 1);
    const last = (trace, i) => trace[i].race;
    // Race presentation (fury --trace writes TRACE.events.json): lamps, crowd
    // cues, the drop gate and the stadium flashes lit after each step.
    const eventsFile = traceFile.replace(/\.json$/, '.events.json');
    const events = await readFile(eventsFile, 'utf8').then(JSON.parse).catch(() => []);
    const presentation = { steps: Math.min(js.length, events.length), lights: 0, crowd: 0, crowdCues: 0, flashes: 0, flashSteps: 0, gateWorst: 0, firstMismatch: null };
    for (let i = 0; i < presentation.steps; i++) {
      const a = js[i].race ?? {}, b = events[i], step = js[i].step;
      const mismatch = what => { presentation[what]++; presentation.firstMismatch ??= { step, what }; };
      if ((a.lights ?? null) !== (b.lights ?? null)) mismatch('lights');
      const canonical = v => JSON.stringify(v, (_, x) => x && typeof x === 'object' && !Array.isArray(x) ? Object.fromEntries(Object.entries(x).sort()) : x);
      if (canonical(a.crowd ?? null) !== canonical(b.crowd ?? null)) mismatch('crowd');
      if (a.crowd) presentation.crowdCues++;
      if (JSON.stringify(a.flashes ?? null) !== JSON.stringify(b.flashes ?? null)) mismatch('flashes');
      if (a.flashes?.length) presentation.flashSteps++;
      if (a.gate || b.gate) presentation.gateWorst = Math.max(presentation.gateWorst, ...(a.gate ?? []).map((v, k) => Math.abs(Math.fround(v) - Math.fround(b.gate?.[k] ?? NaN))));
    }
    presentation.fireworkCommands = events.reduce((n, e) => n + (e.fireworks?.length ?? 0), 0);
    presentation.sounds = events.flatMap(e => (e.sounds ?? []).map(name => [e.step, name]));
    presentation.music = events.filter(e => e.music).map(e => [e.step, e.music]);
    const report = { venue, race, steps: count, web: versus(webTrace), native: versus(rust), presentation,
      phases: { javascript: phases(js), rustWeb: phases(webTrace.slice(1)), rustNative: phases(rust.slice(1)) },
      final: { javascript: last(js, count - 1), rust: last(rust, count) } };
    await writeFile(path.join(out, 'report.json'), JSON.stringify(report, null, 2));
    console.log(JSON.stringify({ ...report, final: { javascript: report.final.javascript?.standings, rust: report.final.rust?.standings } }, null, 2));
    console.log(`PASS: wrote ${venue} race parity to ${out}`);
  } finally {
    await browser.close();
  }
}

if (process.argv[2] === 'race') {
  await raceMode();
  process.exit(0);
}

if (process.argv[2] === 'ride') {
  await rideMode();
  process.exit(0);
}

const positional = [], views = [];
let graphics = 'original', actors = false, race = false;
for (let i = 2; i < process.argv.length; i++) {
  const arg = process.argv[i];
  if (arg === '--view') views.push(process.argv[++i]);
  else if (arg === '--graphics') graphics = process.argv[++i];
  else if (arg === '--actors') actors = true;
  else if (arg === '--race') race = true;
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
  // --race: the National/Supercross course without CPU riders (race weather, Snow).
  await page.goto(url(race ? `/play.html?play=1&map=${venue}&activity=race&cpu=0` : `/play.html?drive=1&environment=${venue}`));
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
    await rust.goto(url(`/engine.html?venue=${venue}&camera=${view}&weather=${state.weather ? 'on' : 'off'}&steps=${state.steps}&graphics=${graphics}&ui=0${actors ? '' : '&free=1'}${race ? '&race=1&cpu=0' : ''}`));
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
      if (!actors) args.unshift('--free');
      if (race) args.unshift('--race', '--cpu', '0');
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
