// Original-content integration; no injected crashes or modified physics.
import assert from 'node:assert/strict';
import { assertNoErrors, base, launch, newPage, waitForScreen } from './browser/harness.mjs';
const browser = await launch();
try {
  const errors = [],
    page = await newPage(browser, { viewport: { width: 390, height: 844 }, hasTouch: true }, errors);
  await page.addInitScript(() => localStorage.setItem('atv-crash-mode', 'crazy')); // Crazy is opt-in; new players default to Original.
  // Observe every actual worker batch frame, including frames between renders.
  // This probe only reads transport responses; it changes no game input/state.
  await page.addInitScript(() => {
    const NativeWorker = window.Worker;
    window.cpuBehavior = Array.from({ length: 4 }, () => ({
      samples: 0,
      peers: 0,
      plants: 0,
      obstacles: 0,
      wrecks: 0,
      neutralWreck: 0,
      detached: 0,
      contacts: 0,
      recoveries: 0,
      resumed: 0,
      tricks: 0,
      finite: true,
      resetDistance: true,
      resetLaps: true,
      kinds: [],
      clips: []
    }));
    window.Worker = class extends NativeWorker {
      constructor(...args) {
        super(...args);
        const ids = new Set();
        this.cpuIds = ids;
        this.addEventListener('message', ({ data }) => {
          if (!ids.delete(data.id) || data.error) return;
          const batch = JSON.parse(new TextDecoder().decode(data.bytes));
          for (const { race } of batch) {
            if (race?.phase !== 'racing') continue;
            race.opponents.forEach((cpu, i) => {
              const s = window.cpuBehavior[i],
                p = cpu.physical,
                d = p?.decisions;
              if (!s || !d) return;
              s.samples++;
              s.peers += d.avoidance.peers > 0;
              s.plants += d.avoidance.vegetation ?? 0;
              s.obstacles += d.avoidance.obstacle_hits;
              s.finite &&=
                Number.isFinite(p.speed) &&
                p.orientation.every(Number.isFinite) &&
                p.quad_matrices.flat().every(Number.isFinite) &&
                p.rider_matrices.flat().every(Number.isFinite);
              const wreck = p.wreck.phase !== 'Riding';
              if (wreck && !s.wasWreck) {
                s.wrecks++;
                s.kinds.push(p.wreck.kind);
              }
              if (wreck && !d.active) {
                s.neutralWreck++;
                s.finite &&= d.throttle === 0 && d.brake === 0 && d.steer === 0 && d.lean === 0;
              }
              if (p.rider_wreck) {
                s.detached++;
                s.contacts += p.rider_wreck.contact_count;
                s.finite &&= p.rider_world_matrix?.length === 16 && p.rider_world_matrix.every(Number.isFinite);
              }
              if (s.lastReset !== undefined && p.reset_count > s.lastReset) {
                s.recoveries++;
                s.resetDistance &&= cpu.distance - s.lastDistance < 0.001;
                s.resetLaps &&= race.standings.find(r => r.name === cpu.name).laps === s.lastLaps;
              }
              if (s.recoveries && !wreck && d.active && p.speed > 10) s.resumed++;
              if (p.stunt?.started) {
                s.tricks++;
                s.clips.push(p.stunt.name);
              }
              s.lastReset = p.reset_count;
              s.lastDistance = cpu.distance;
              s.lastLaps = race.standings.find(r => r.name === cpu.name).laps;
              s.wasWreck = wreck;
            });
          }
        });
      }
      postMessage(message, ...args) {
        if (message?.op === 50) this.cpuIds.add(message.id);
        return super.postMessage(message, ...args);
      }
    };
  });
  const screen = n => waitForScreen(page, n);
  await page.goto(base);
  await screen('title');
  for (const [label, next] of [
    ['Press START', 'main'],
    ['Single Player', 'single'],
    ['Race', 'race']
  ]) {
    await page.getByRole('button', { name: label, exact: true }).click();
    await screen(next);
  }
  await page.getByRole('button', { name: 'CPU riders: 3', exact: true }).click();
  await page.getByRole('button', { name: 'Start Race', exact: true }).click();
  const seconds = Number(process.env.ATV_CPU_SECONDS || 65);
  await page.waitForFunction(t => window.atvTerrainStatus.frame?.race?.elapsed >= t, seconds, { timeout: 240000 });
  const stats = await page.evaluate(() => window.cpuBehavior),
    frame = await page.evaluate(() => window.atvTerrainStatus.frame);
  console.log(
    JSON.stringify({
      gate: 'natural-cpu-behavior',
      elapsed: frame.race.elapsed,
      stats,
      cpus: frame.race.opponents.map(o => ({
        name: o.name,
        distance: o.distance,
        reset: o.physical.reset_count,
        phase: o.physical.wreck.phase
      }))
    })
  );
  assert(stats.every(s => s.samples > 100 && s.peers > 0 && s.finite && s.resetDistance && s.resetLaps));
  assert(
    stats.some(s => s.tricks > 0),
    'Original jump flags/airtime select live original stunt clips'
  );
  assert(
    stats.some(s => s.obstacles > 0),
    'Original scenery rays encounter real course geometry'
  );
  assert(
    stats.some(
      s => s.wrecks > 0 && s.neutralWreck > 0 && s.detached > 0 && s.contacts > 0 && s.recoveries > 0 && s.resumed > 0
    ),
    'Natural wreck, detached rider contacts, recovery and resumed driving'
  );
  for (const size of [
    { width: 844, height: 390 },
    { width: 390, height: 844 }
  ]) {
    await page.setViewportSize(size);
    await page.getByRole('button', { name: 'Pause', exact: true }).tap();
    await screen('pause');
    // Time-based on purpose: let any in-flight worker batch land, then prove the paused frame does not change.
    await page.waitForTimeout(100);
    const before = await page.evaluate(() => window.atvTerrainStatus.frame);
    await page.waitForTimeout(300);
    assert.deepEqual(await page.evaluate(() => window.atvTerrainStatus.frame), before);
    await page.getByRole('button', { name: 'Resume', exact: true }).tap();
  }
  assertNoErrors(errors);
} finally {
  await browser.close();
}
