// Host-only Arlington Free Ride gate with actual WASM, content and the Rust relay.
// Two real browsers tap the main menu item and meet in shared room FREERD01;
// synthetic riders (the Rust codec over Node WebSockets) fill it to 20, the
// name filter replaces an offensive name, the 21st rider opens FREERD02, and
// the full room's frame rate, relay round trip, engine voices and name tags
// are sampled. Leave returns to the main menu.
//   PLAYWRIGHT_MODULE=/path/to/playwright ATV_TEST_URL=http://127.0.0.1:4187 \
//   node tools/test_freeride.mjs
// ATV_FREERIDE_BOTS (default 18) sets the synthetic riders; ATV_FREERIDE_HZ
// (default 20, the near/fast cadence) their send rate; ATV_FREERIDE_REPORT
// writes the JSON report.
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { createInterface } from 'node:readline';
import { homedir } from 'node:os';
import path from 'node:path';
import { assertNoErrors, base, button, launch, watchErrors, writeJson } from './browser/harness.mjs';

const BOTS = Number(process.env.ATV_FREERIDE_BOTS ?? 18);
const HZ = Number(process.env.ATV_FREERIDE_HZ ?? 20);
const report = { bots: BOTS, hz: HZ, samples: [] };
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));

// One persistent Rust codec process encodes every synthetic packet.
const metadata = spawnSync(
  path.join(homedir(), '.cargo/bin/cargo'),
  ['metadata', '--no-deps', '--format-version', '1'],
  { encoding: 'utf8', cwd: new URL('..', import.meta.url) }
);
const codecPath =
  process.env.ATV_CODEC ?? path.join(JSON.parse(metadata.stdout).target_directory, 'debug/examples/codec');
const codec = spawn(codecPath, ['stream'], { stdio: ['pipe', 'pipe', 'inherit'] });
const pending = [];
createInterface({ input: codec.stdout }).on('line', line => pending.shift()(Buffer.from(line, 'hex')));
const encode = packet =>
  new Promise(resolve => {
    pending.push(resolve);
    codec.stdin.write(JSON.stringify(packet) + '\n');
  });

async function claim() {
  const response = await fetch(base + '/multiplayer/freeride', { method: 'POST', headers: { Origin: base } });
  assert.equal(response.status, 200, await response.clone().text());
  return response.json();
}
const transform = position => ({ position, rotation: [1, 0, 0, 0], scale: [1, 1, 1], shear: [0, 0, 0] });
async function bot(index, centre, name) {
  const { code, key } = await claim();
  const socket = new WebSocket(`${base.replace('http', 'ws')}/multiplayer/rooms/${code}?key=${key}`, {
    headers: { Origin: base }
  });
  socket.binaryType = 'arraybuffer';
  await new Promise((resolve, reject) => {
    socket.onopen = resolve;
    socket.onerror = () => reject(new Error(`bot ${index} socket failed`));
  });
  const quad = (index % 26) + 1;
  socket.send(
    await encode({
      Join: {
        code: [...Buffer.from(code)],
        player: { id: 0, name, atv_model: quad, rider_model: index % 2, appearance: [0, 0, 0, 0, 0, 0, 0, 0] }
      }
    })
  );
  socket.send(await encode({ Ready: { map: 8 } }));
  const started = Date.now(),
    radius = 30 + (index % 6) * 12,
    speed = 25 / radius,
    phase = (index / BOTS) * Math.PI * 2;
  let sequence = 1;
  const timer = setInterval(async () => {
    const t = (Date.now() - started) / 1000,
      angle = phase + t * speed;
    const position = [centre[0] + Math.cos(angle) * radius, centre[1] + 1, centre[2] + Math.sin(angle) * radius];
    const velocity = [-Math.sin(angle) * 25, 0, Math.cos(angle) * 25];
    const yaw = -angle / 2;
    const bytes = await encode({
      Snapshot: {
        animation: {
          steering: 0.3,
          wheels: [t * 10, t * 10, t * 10, t * 10],
          compression: [0, 0, 0, 0],
          clips: [4, 0, 0, 0],
          count: 1,
          phase: 0.5,
          first_weight: 0,
          pair_weight: 0,
          look_phase: -1,
          paused: false
        },
        peer: 0,
        sequence: sequence++,
        time_ms: Date.now() - started,
        atv_model: quad,
        rider_model: index % 2,
        atv: { ...transform(position), rotation: [Math.cos(yaw), 0, Math.sin(yaw), 0] },
        velocity,
        angular_velocity: [0, speed, 0],
        atv_nodes: [],
        rider_nodes: [],
        rider_state: 'Riding',
        rider: transform(position),
        rider_velocity: velocity,
        rpm: 6000,
        throttle: 0.7,
        speed: 25,
        crash_serial: 0,
        train_time: null,
        train_running: false,
        platform: null
      }
    });
    if (socket.readyState === WebSocket.OPEN) socket.send(bytes);
  }, 1000 / HZ);
  return { code, socket, stop: () => (clearInterval(timer), socket.close(1000, 'Left lobby')) };
}

const browser = await launch();
const errors = [];
async function rider(viewport) {
  const context = await browser.newContext({ viewport, hasTouch: true });
  const page = await context.newPage();
  watchErrors(page, errors);
  await page.addInitScript(() => {
    Object.defineProperty(document, 'hasFocus', { value: () => true });
    // Record every menu screen shown, to prove Free Ride skips the lobby page.
    window.screensSeen = [];
    new MutationObserver(() => {
      const screen = document.querySelector('[data-screen]')?.dataset.screen;
      if (screen && window.screensSeen.at(-1) !== screen) window.screensSeen.push(screen);
    }).observe(document, { subtree: true, childList: true, attributes: true, attributeFilter: ['data-screen'] });
  });
  return page;
}
const state = page => page.evaluate(() => window.atvTerrainStatus);
async function freeRide(page) {
  await page.goto(base + '/play.html');
  await button(page, 'Press START').click({ timeout: 90000 });
  // The main menu offers Free Ride beside Single Player and Multiplayer, with
  // a NEW tag and the number of people riding it.
  const row = button(page, 'Arlington Free Ride');
  await row.locator('.ms-new').waitFor();
  await row.locator('.ms-live:not([hidden])').waitFor({ timeout: 20000 });
  const riding = Number((await row.locator('.ms-live').textContent()).match(/^(\d+) riding$/)[1]);
  await row.click();
  await page.waitForFunction(
    () => {
      const s = window.atvTerrainStatus;
      return (
        s?.multiplayerError ||
        (s?.simulation && s.multiplayer?.state?.lobby?.public && !document.body.classList.contains('menu-open'))
      );
    },
    null,
    { timeout: 120000 }
  );
  const s = await state(page);
  assert.equal(s.multiplayerError, undefined, s.multiplayerError);
  assert.equal(s.multiplayer.state.environment, 'env08');
  // The tap goes straight to the loading screen: no lobby page flashes first.
  const seen = await page.evaluate(() => window.screensSeen);
  const after = seen.slice(seen.lastIndexOf('main') + 1);
  assert.deepEqual(after, ['loading'], `screens after the tap: ${after.join(' → ')}`);
  s.ridingBefore = riding;
  return s;
}
const remotes = (page, count, timeout = 60000) =>
  page.waitForFunction(
    count => (window.atvTerrainStatus.multiplayer?.remote ?? []).filter(r => !r.stale).length >= count,
    count,
    { timeout }
  );
const bots = [];
try {
  const a = await rider({ width: 1280, height: 896 });
  let s = await freeRide(a);
  assert.equal(s.multiplayer.state.code, 'FREERD01');
  // Riders who kept the default name are numbered so strangers can tell them apart.
  const self = s.multiplayer.state.lobby.players.find(player => player.id === s.multiplayer.state.peer);
  assert.match(self.name, /^Rider \d{4}$/);
  // A fresh relay (or 90 s after a previous run's held slots) starts empty.
  assert.equal(s.multiplayer.state.lobby.players.length, 1, 'FREERD01 should start empty');
  report.first = { code: s.multiplayer.state.code, peer: s.multiplayer.state.peer, ridingBefore: s.ridingBefore };
  assert.equal(s.ridingBefore, 0, 'nobody rides before the first browser');
  const b = await rider({ width: 390, height: 844 });
  s = await freeRide(b);
  assert.equal(s.multiplayer.state.code, 'FREERD01');
  await remotes(a, 1);
  await remotes(b, 1);
  report.meet = 'two browsers met in FREERD01';

  // Fill the room. One synthetic rider picks a name the filter must replace.
  const centre = (await state(a)).frame.origin;
  for (let i = 0; i < BOTS; i++) {
    bots.push(await bot(i, centre, i === 0 ? 'fuck you' : `Bot ${i + 1}`));
    if (process.env.ATV_FREERIDE_VERBOSE) console.log('bot', i, 'joined', new Date().toISOString());
  }
  assert.ok(bots.every(entry => entry.code === 'FREERD01'));
  const full = 2 + BOTS;
  await remotes(a, full - 1, 120000);
  s = await state(a);
  const names = s.multiplayer.state.lobby.players.map(player => player.name);
  assert.equal(names.length, full);
  assert.ok(!names.some(name => /fuck/i.test(name)), names.join(', '));
  // The filtered rider is shown as its slot number, not the name it sent.
  assert.ok(
    names.some(name => /^Rider \d{1,2}$/.test(name)),
    names.join(', ')
  );
  report.filteredName = true;

  // A rider on the main menu sees the live count of everyone riding.
  const watcher = await rider({ width: 390, height: 844 });
  await watcher.goto(base + '/play.html');
  await button(watcher, 'Press START').click({ timeout: 90000 });
  await watcher.waitForFunction(full => document.querySelector('.ms-live')?.textContent === `${full} riding`, full, {
    timeout: 45000
  });
  report.menuCount = `${full} riding`;
  if (process.env.ATV_FREERIDE_SCREENSHOT)
    await watcher.screenshot({ path: process.env.ATV_FREERIDE_SCREENSHOT.replace('.png', '-menu-count.png') });

  // The 21st rider opens the next shared room.
  if (full >= 20) {
    const overflow = await claim();
    assert.equal(overflow.code, 'FREERD02');
    report.overflow = overflow.code;
  }

  // Full-room sample: rolling diagnostics refresh over ten seconds.
  // Time-based on purpose: wall-clock samples of the rolling performance diagnostics.
  for (const page of [a, b]) {
    await page.bringToFront();
    await page.keyboard.press('F3'); // Performance overlay for the rolling FPS diagnostics.
  }
  await a.bringToFront();
  for (let tick = 0; tick < 10; tick++) {
    await a.waitForTimeout(1000);
    for (const [label, page] of [
      ['desktop', a],
      ['portrait', b]
    ]) {
      const v = await state(page);
      report.samples.push({
        label,
        remotes: v.multiplayer.remote.filter(r => !r.stale).length,
        fps: v.performance?.fps,
        p95: v.performance?.p95Ms,
        cpu: v.performance?.renderCpuMs,
        rtt: v.multiplayer.state.rttMs,
        voices: v.audio?.multiplayer?.voices,
        draws: v.multiplayerDraws,
        tags: await page.evaluate(() => document.querySelectorAll('#player-indicators .player-indicator').length),
        edgeTags: await page.evaluate(
          () => document.querySelectorAll('#player-indicators .player-indicator[data-edge=true]').length
        ),
        dropped: v.multiplayer.droppedSnapshots,
        models: v.multiplayer.models.length,
        error: v.multiplayerError
      });
    }
  }
  console.log('SAMPLES', JSON.stringify(report.samples.slice(-2)));
  const shots = process.env.ATV_FREERIDE_SCREENSHOT;
  if (shots) {
    await a.screenshot({ path: shots.replace('.png', '-desktop.png') });
    await b.bringToFront();
    await b.screenshot({ path: shots.replace('.png', '-portrait.png') });
    await a.bringToFront();
  }
  const last = report.samples.slice(-2);
  for (const sample of last) assert.equal(sample.remotes, full - 1, `${sample.label} sees every rider`);
  for (const sample of report.samples) {
    assert.equal(sample.error, undefined, sample.error);
    assert.ok(sample.voices == null || sample.voices <= 6, `engine voices ${sample.voices}`);
    assert.ok(sample.tags <= 8, `name tags ${sample.tags}`);
    assert.ok(sample.edgeTags <= 3, `off-screen arrows ${sample.edgeTags}`);
  }

  // Pause shows only shared-room rows; Leave returns to the main menu.
  await button(a, 'Pause').click();
  assert.equal(await button(a, 'Copy invite link').count(), 0);
  assert.equal(await button(a, 'Select Track').count(), 0);
  if (shots) await a.screenshot({ path: shots.replace('.png', '-pause.png') });
  await button(a, 'Players').click();
  await a
    .getByRole('button', { name: /^Spawn beside / })
    .first()
    .waitFor({ timeout: 10000 });
  if (shots) await a.screenshot({ path: shots.replace('.png', '-players.png') });
  const visible = (await state(a)).multiplayer.remote.filter(r => !r.stale).length;
  assert.ok((await a.getByRole('button', { name: /^Spawn beside / }).count()) >= Math.min(visible, full - 2));
  await button(a, 'Back').click();
  await button(a, 'Leave Game').click();
  await button(a, 'Arlington Free Ride').waitFor({ timeout: 20000 });
  if (shots) await a.screenshot({ path: shots.replace('.png', '-menu.png') });
  report.leave = 'returned to the main menu';
  assertNoErrors(errors);
  console.log(JSON.stringify(report, null, 2));
  if (process.env.ATV_FREERIDE_REPORT) await writeJson(process.env.ATV_FREERIDE_REPORT, report);
} finally {
  for (const entry of bots) entry.stop();
  codec.stdin.end();
  await browser.close();
}
