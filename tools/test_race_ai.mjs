// Source-content host/public integration; excluded from private-free CI.
import assert from 'node:assert/strict';
import { assertNoErrors, base, button, launch, newPage } from './browser/harness.mjs';
const browser = await launch();
const errors = [];
try {
  const page = await newPage(browser, { viewport: { width: 844, height: 390 }, hasTouch: true }, errors);
  // Enable existing read-only render counters for the GPU delivery assertion.
  await page.addInitScript(() => localStorage.setItem('atv.diagnostics.v1', 'performance'));
  const screen = async n => {
    await page.waitForFunction(n => document.querySelector('#frontend').dataset.page === n, n, { timeout: 90000 });
  };
  await page.goto(base);
  await screen('title');
  await button(page, 'Press START').click();
  await screen('main');
  await button(page, 'Single Player').click();
  await screen('single');
  await button(page, 'Race').click();
  await screen('race');
  // The original Normal/Expert choice; Normal keeps the leader speed policy.
  await button(page, 'Difficulty: Normal').waitFor();
  await page.getByRole('button', { name: 'Next Difficulty' }).click();
  await button(page, 'Difficulty: Expert').waitFor();
  await page.getByRole('button', { name: 'Next Difficulty' }).click();
  await button(page, 'Difficulty: Normal').waitFor();
  await button(page, 'CPU riders: 3').click();
  await button(page, 'CPU riders: 4').waitFor();
  await button(page, 'Start Race').click();
  await page.waitForFunction(() => window.atvTerrainStatus.frame?.race?.phase === 'racing', null, { timeout: 120000 });
  // Observe each published simulation frame, rather than requiring a brief
  // braking/coasting decision to coincide with a five-second telemetry sample.
  await page.evaluate(() => {
    window.cpuDrivingChecks = Array.from({ length: 4 }, () => ({
      samples: 0,
      brake: 0,
      coast: 0,
      limited: 0,
      preloads: 0,
      edges: 0,
      maxError: 0
    }));
    let last = -1;
    function observe() {
      const race = window.atvTerrainStatus.frame?.race;
      if (race?.phase === 'racing' && race.elapsed !== last) {
        last = race.elapsed;
        race.opponents.forEach((cpu, i) => {
          const d = cpu.physical?.decisions;
          if (!d) return;
          const s = window.cpuDrivingChecks[i];
          s.samples++;
          s.brake += d.brake === 1;
          s.coast += d.throttle === 0;
          s.limited += d.ahead_distance > 150 && d.desired_speed <= 75.01;
          s.preloads += d.preload_flick !== 0;
          s.edges += d.avoidance?.track_edge === true;
          s.maxError = Math.max(s.maxError, d.route_error);
        });
      }
      requestAnimationFrame(observe);
    }
    requestAnimationFrame(observe);
  });
  const rows = [];
  for (let seconds = 5; seconds <= 45; seconds += 5) {
    await page.waitForFunction(t => window.atvTerrainStatus.frame.race.elapsed >= t, seconds, { timeout: 120000 });
    rows.push(
      await page.evaluate(() => ({
        elapsed: window.atvTerrainStatus.frame.race.elapsed,
        draws: window.atvTerrainStatus.cpuDraws,
        cpus: window.atvTerrainStatus.frame.race.opponents.map(o => ({
          origin: o.origin,
          distance: o.distance,
          physical: o.physical
        }))
      }))
    );
  }
  console.log(
    JSON.stringify({
      telemetry: rows.map(r => ({
        elapsed: r.elapsed,
        cpus: r.cpus.map(o => ({
          origin: o.origin,
          distance: o.distance,
          speed: o.physical.speed,
          grounded: o.physical.grounded,
          orientation: o.physical.orientation,
          decision: o.physical.decisions
        }))
      }))
    })
  );
  const decisions = await page.evaluate(() => window.cpuDrivingChecks);
  assert.equal(rows.at(-1).cpus.length, 4);
  for (let i = 0; i < 4; i++) {
    const cpu = rows.map(r => r.cpus[i]);
    assert(
      cpu.every(o => o.physical && o.physical.quad_matrices.length === 27 && o.physical.rider_matrices.length === 18)
    );
    assert(cpu.at(-1).distance > 400, 'Physical driver must keep moving through several turns');
    assert(
      cpu.some(o => Math.abs(o.physical.decisions.steer) > 0.2),
      'Driver turns towards the course'
    );
    assert(decisions[i].coast > 0, 'Recovered speed policy coasts or brakes during the drive');
    assert(decisions[i].limited > 0, 'Original leader speed policy limits a CPU ahead of an idle player');
    const y = cpu.map(o => o.origin[1]);
    assert(Math.max(...y) - Math.min(...y) > 3, 'CPU follows changing terrain heights through suspension physics');
    assert(cpu.every(o => Number.isFinite(o.physical.speed) && o.physical.orientation.every(Number.isFinite)));
  }
  assert(
    rows.some(r => r.draws > 0),
    'Physical CPU matrices reach actual GPU draws'
  );
  assert(
    decisions.some(d => d.preloads > 0),
    'The racer jump hook releases a preload on a flagged Caribou jump'
  );
  assert(
    decisions.some(d => d.edges > 0),
    'The track-edge tail turns a CPU whose 20ft probe leaves the track'
  );
  await button(page, 'Pause').click();
  await screen('pause');
  // Time-based on purpose: proves the paused race frame does not change.
  const before = await page.evaluate(() => window.atvTerrainStatus.frame);
  await page.waitForTimeout(500);
  assert.deepEqual(await page.evaluate(() => window.atvTerrainStatus.frame), before);
  // Expert (play link): race creation leaves out the leader speed policy, so
  // CPUs well ahead of the idle player keep their TDF target speed.
  const expert = await newPage(browser, { viewport: { width: 844, height: 390 }, hasTouch: true }, errors);
  await expert.goto(`${base}/play.html?activity=race&map=env01&cpu=4&difficulty=expert`);
  await expert.waitForFunction(() => window.atvTerrainStatus.frame?.race?.elapsed >= 15, null, { timeout: 180000 });
  const expertSpeeds = await expert.evaluate(
    () =>
      new Promise(resolve => {
        const seen = { ahead: 0, full: 0 };
        let last = -1;
        const observe = () => {
          const race = window.atvTerrainStatus.frame?.race;
          if (race.elapsed !== last) {
            last = race.elapsed;
            for (const d of race.opponents.map(o => o.physical?.decisions)) {
              if (!d?.active || d.ahead_distance <= 150) continue;
              seen.ahead++;
              seen.full += d.desired_speed >= 149;
            }
          }
          if (race.elapsed >= 25) resolve(seen);
          else requestAnimationFrame(observe);
        };
        requestAnimationFrame(observe);
      })
  );
  assert(expertSpeeds.ahead > 0, 'Expert CPUs pull away from the idle player');
  // Normal caps a far-ahead CPU at about half the 150ft/s sentinel.
  assert(expertSpeeds.full > 0, `Expert keeps the full TDF target: ${JSON.stringify(expertSpeeds)}`);
  await expert.close();
  assertNoErrors(errors);
  console.log(
    JSON.stringify({
      gate: 'source-cpu-driving',
      decisions,
      elapsed: rows.at(-1).elapsed,
      samples: rows.map(r => ({
        elapsed: r.elapsed,
        draws: r.draws,
        cpus: r.cpus.map(o => ({
          origin: o.origin,
          distance: o.distance,
          speed: o.physical.speed,
          steer: o.physical.decisions.steer,
          brake: o.physical.decisions.brake,
          throttle: o.physical.decisions.throttle,
          error: o.physical.decisions.route_error
        }))
      })),
      errors
    })
  );
} finally {
  await browser.close();
}
