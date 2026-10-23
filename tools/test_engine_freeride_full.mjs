// Host-only measurement: a full Arlington Free Ride room in the Rust engine.
// One Rust browser rides Free Ride from its main menu; synthetic riders (the
// Rust codec over Node WebSockets, test_freeride.mjs's bots) circle it at the
// near/fast 20 Hz cadence until the room holds 20. Over a 30 s window (ATV_FREERIDE_SECONDS) the
// Rust page's frame rate and CPU, drawn riders, name tags, engine voices,
// relay round trip and received/sent bandwidth are sampled.
//   ATV_CODEC=target/.../debug/examples/codec PLAYWRIGHT_MODULE=... \
//   ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_freeride_full.mjs
// ATV_FREERIDE_BOTS (default 19), ATV_FREERIDE_HZ (default 20),
// ATV_FREERIDE_REPORT writes the JSON report, ATV_MP_SCREENSHOT a capture.
// Local relay only: never point this at the production site.
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { createInterface } from 'node:readline';
import { homedir } from 'node:os';
import path from 'node:path';
import { assertNoErrors, base, launch, newPage, nextFrames, url, writeJson } from './browser/harness.mjs';

assert.ok(/127\.0\.0\.1|localhost/.test(base), 'load runs only against a local relay');
const BOTS = Number(process.env.ATV_FREERIDE_BOTS ?? 19);
const HZ = Number(process.env.ATV_FREERIDE_HZ ?? 20);
const SECONDS = Number(process.env.ATV_FREERIDE_SECONDS ?? 30);
const PING = Number(process.env.ATV_FREERIDE_PING ?? 1000);
const report = { bots: BOTS, hz: HZ, samples: [] };
const relayRtt = [];
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
  // Bot 1 also pings each second: the relay's own round trip under this
  // load, to compare with the Rust client's (Pong: kind 7, nonce at byte 8).
  let probe = null;
  if (index === 0) {
    const sent = new Map();
    let nonce = 1;
    socket.onmessage = event => {
      const bytes = new Uint8Array(event.data);
      if (bytes[5] !== 7 || bytes.length < 12) return;
      const id = new DataView(event.data).getUint32(8, true);
      if (sent.has(id)) relayRtt.push(performance.now() - sent.get(id));
    };
    probe = setInterval(async () => {
      const id = nonce++;
      const bytes = await encode({ Ping: { nonce: id } });
      sent.set(id, performance.now());
      if (socket.readyState === WebSocket.OPEN) socket.send(bytes);
    }, 1000);
  }
  return {
    code,
    socket,
    stop: () => (clearInterval(timer), clearInterval(probe), socket.close(1000, 'Left lobby'))
  };
}

const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const engine = p => p.evaluate(() => JSON.parse(window.furyMultiplayer ?? 'null'));
async function key(page, code) {
  await page.keyboard.down(code);
  await nextFrames(page, 2);
  await page.keyboard.up(code);
  await nextFrames(page, 3);
}
const browser = await launch();
const errors = [];
const bots = [];
try {
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await page.addInitScript(() => {
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    Object.defineProperty(document, 'hasFocus', { value: () => true });
  });
  // Pings each second (the room default is 15 s) give a steady round trip.
  await page.goto(url(`/play.html?uiDiagnostics=1${PING ? `&pingMs=${PING}` : ''}`));
  await page.waitForFunction(
    () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === 'title',
    null,
    { timeout: 120000 }
  );
  await page.mouse.click(8, 8);
  await key(page, 'Enter');
  await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state').textContent).front?.page === 'main');
  const choices = (await read(page)).front.choices;
  for (let i = 0; i < choices.indexOf('freeride'); i++) await key(page, 'ArrowDown');
  await key(page, 'Enter');
  await page.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.environment === 'env08', null, {
    timeout: 180000
  });
  // Let the ride's own start (sound bank decode, map raster) finish first,
  // so the arrival window measures arrivals.
  await sleep(8000);
  const me = await engine(page);
  report.room = me.code;
  // Fill the room around the Rust rider.
  // Riders arrive together, as a busy room brings them; the worst frame
  // while their models and engine sounds load is sampled too.
  const arrival = [];
  // ATV_FREERIDE_PROFILE=out.cpuprofile records the page's CPU while the
  // riders arrive (Chrome DevTools format; build without
  // --remove-name-section for wasm function names).
  const cdp = process.env.ATV_FREERIDE_PROFILE ? await page.context().newCDPSession(page) : null;
  if (cdp) {
    await cdp.send('Profiler.enable');
    await cdp.send('Profiler.setSamplingInterval', { interval: 250 });
    await cdp.send('Profiler.start');
  }
  const filling = Date.now();
  const watch = setInterval(async () => {
    const status = await page.evaluate(() => document.querySelector('#status')?.textContent ?? '').catch(() => '');
    const worst = Number(status.match(/([\d.]+) ms worst/)?.[1]);
    const load = Number(status.match(/([\d.]+) ms load/)?.[1]);
    const upload = Number(status.match(/([\d.]+) ms upload/)?.[1]);
    const cpu = Number(status.match(/([\d.]+) ms CPU/)?.[1]);
    if (Number.isFinite(worst)) arrival.push({ worst, load, upload, cpu });
  }, 1000);
  // Joins are paced: simultaneous claims have stalled the local workerd.
  for (let i = 0; i < BOTS; i++) {
    bots.push(await bot(i, me.origin, `Bot ${i + 1}`));
    await sleep(250);
  }
  await page.waitForFunction(n => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= n, Math.min(BOTS, 19), {
    timeout: 180000
  });
  report.allDrawnSeconds = (Date.now() - filling) / 1000;
  // Engine voices start once riders are drawn; keep watching for them.
  await sleep(5000);
  clearInterval(watch);
  if (cdp) {
    const { profile } = await cdp.send('Profiler.stop');
    await writeJson(process.env.ATV_FREERIDE_PROFILE, profile);
  }
  report.arrival = arrival;
  report.arrivalWorstFrameMs = Math.max(...arrival.map(a => a.worst));
  const first = await engine(page);
  const t0 = Date.now();
  for (let i = 0; i < SECONDS; i++) {
    await sleep(1000);
    const [m, status] = await Promise.all([
      engine(page),
      page.evaluate(() => document.querySelector('#status')?.textContent ?? '')
    ]);
    const fps = Number(status.match(/(\d+) fps/)?.[1]);
    const frame = Number(status.match(/([\d.]+) ms frame/)?.[1]);
    const cpu = Number(status.match(/([\d.]+) ms CPU/)?.[1]);
    const worst = Number(status.match(/([\d.]+) ms worst/)?.[1]);
    const room = Number(status.match(/([\d.]+) ms room/)?.[1]);
    report.samples.push({
      fps,
      frame,
      cpu,
      worst,
      room,
      drawn: m.drawn,
      tags: m.tags,
      voices: m.remoteSound?.voices
    });
  }
  const last = await engine(page);
  const seconds = (Date.now() - t0) / 1000;
  const stat = key => {
    const v = report.samples
      .map(s => s[key])
      .filter(Number.isFinite)
      .sort((a, b) => a - b);
    return { min: v[0], median: v[Math.floor(v.length / 2)], max: v.at(-1) };
  };
  report.summary = {
    players: last.players,
    drawn: stat('drawn'),
    tags: stat('tags'),
    fps: stat('fps'),
    frameMs: stat('frame'),
    cpuMs: stat('cpu'),
    worstFrameMs: stat('worst'),
    roomMs: stat('room'),
    rttMs: (() => {
      const v = last.rttSamples.slice(-SECONDS).sort((a, b) => a - b);
      return {
        samples: v.length,
        min: v[0],
        median: v[Math.floor(v.length / 2)],
        p95: v[Math.floor(v.length * 0.95)],
        max: v.at(-1)
      };
    })(),
    botRttMs: (() => {
      const v = relayRtt.slice(-SECONDS).sort((a, b) => a - b);
      return { samples: v.length, median: v[Math.floor(v.length / 2)], p95: v[Math.floor(v.length * 0.95)] };
    })(),
    receivedKBps: (last.network.receivedBytes - first.network.receivedBytes) / seconds / 1000,
    sentBytesPerSecond: (last.network.sentBytes - first.network.sentBytes) / seconds,
    closed: last.closed
  };
  assert.equal(last.closed, null);
  assert.ok(report.summary.drawn.min >= Math.min(BOTS, 19) - 1, JSON.stringify(report.summary));
  if (process.env.ATV_MP_SCREENSHOT) await page.screenshot({ path: process.env.ATV_MP_SCREENSHOT });
  if (process.env.ATV_FREERIDE_REPORT) writeJson(process.env.ATV_FREERIDE_REPORT, report);
  assertNoErrors(errors);
  console.log(
    JSON.stringify(
      { allDrawnSeconds: report.allDrawnSeconds, arrivalWorstFrameMs: report.arrivalWorstFrameMs, ...report.summary },
      null,
      2
    )
  );
} finally {
  for (const b of bots) b.stop();
  codec.stdin.end();
  await browser.close();
}
