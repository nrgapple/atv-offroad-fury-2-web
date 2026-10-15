// Owner-content gate for actual touch turn direction, grip/peg attachment and
// pose continuity. No solver poses, velocities or contact states are injected.
import assert from 'node:assert/strict';
import { writeFile } from 'node:fs/promises';
import { captureSimulation } from './capture_simulation.mjs';
import { captureCrashIsolation, verifyCrashIsolation } from './crash_isolation.mjs';
import { assertNoErrors, base, gameState, launch, newPage, waitForGame, waitForSteps } from './browser/harness.mjs';
const browser = await launch();
try {
  const errors = [],
    page = await newPage(browser, { viewport: { width: 390, height: 844 }, hasTouch: true }, errors);
  await captureCrashIsolation(page);
  await captureSimulation(page, 'controlFrames');
  await page.addInitScript(() => {
    window.bikePresentationProbe = { draws: 0, blends: 0, maxOriginError: 0, contacts: 0, maxGrip: 0, maxFoot: 0 };
    const requests = new WeakMap(),
      post = Worker.prototype.postMessage;
    let model,
      quad,
      body = false,
      bodyMatrix;
    Worker.prototype.postMessage = function (message, ...args) {
      if (!requests.has(this)) {
        const ids = new Set();
        requests.set(this, ids);
        this.addEventListener('message', ({ data }) => {
          if (ids.delete(data.id) && data.bytes && !data.error)
            model = JSON.parse(new TextDecoder().decode(data.bytes));
        });
      }
      if (message.op === 100) requests.get(this).add(message.id);
      return post.call(this, message, ...args);
    };
    const proto = WebGL2RenderingContext.prototype,
      locations = new WeakMap(),
      get = proto.getUniformLocation,
      mat = proto.uniformMatrix4fv;
    proto.getUniformLocation = function (...args) {
      const loc = get.apply(this, args);
      if (loc) locations.set(loc, args[1]);
      return loc;
    };
    proto.uniformMatrix4fv = function (loc, transpose, value) {
      const status = window.atvTerrainStatus,
        p = status?.motorcyclePresentation;
      if (locations.get(loc) === 'model') body = false;
      if (locations.get(loc) === 'model' && p?.interpolated && status.frame?.wreck.phase === 'Riding') {
        // The body model uniform must use the same presentation origin, not the
        // latest worker pose. Ignore identity/scenery/CPU uniforms.
        const error = Math.hypot(...p.origin.map((v, i) => v - value[12 + i]));
        if (error < 0.001) {
          body = true;
          bodyMatrix = Array.from(value);
          window.bikePresentationProbe.draws++;
          window.bikePresentationProbe.blends++;
          window.bikePresentationProbe.maxOriginError = Math.max(window.bikePresentationProbe.maxOriginError, error);
        }
      }
      if (body && model && locations.get(loc) === 'quadBones[0]') {
        if (value.length === model.mesh.nodes.length * 16) quad = Array.from(value);
        if (quad && value.length === model.riderMesh.nodes.length * 16) {
          const error = (r, b) => {
            const ri = model.riderMesh.nodes.findIndex(n => n.name.toLowerCase() === r.toLowerCase()),
              bi = model.mesh.nodes.findIndex(n => n.name.toLowerCase() === b.toLowerCase());
            return Math.hypot(...[0, 1, 2].map(i => value[ri * 16 + 12 + i] - quad[bi * 16 + 12 + i]));
          };
          const probe = window.bikePresentationProbe;
          probe.contacts++;
          probe.maxGrip = Math.max(
            probe.maxGrip,
            error('Dummy_hand_L', 'Dummy_Hand_L'),
            error('Dummy_hand_R', 'Dummy_Hand_R')
          );
          probe.maxFoot = Math.max(
            probe.maxFoot,
            error('Dummy_foot_L', 'Dummy_Foot_L'),
            error('Dummy_foot_R', 'Dummy_Foot_R')
          );
          const head = model.riderMesh.nodes.findIndex(n => n.name === 'Head'),
            pelvis = model.riderMesh.nodes.findIndex(n => n.name === 'Pelvis');
          const local = [0, 1, 2].map(i => value[head * 16 + 12 + i] - value[pelvis * 16 + 12 + i]);
          const world = [0, 1, 2].map(
            i => bodyMatrix[i] * local[0] + bodyMatrix[4 + i] * local[1] + bodyMatrix[8 + i] * local[2]
          );
          const yaw = status.camera[3],
            right = [Math.cos(yaw), 0, -Math.sin(yaw)];
          probe.bank = {
            body: bodyMatrix[4] * right[0] + bodyMatrix[6] * right[2],
            rider: world[0] * right[0] + world[2] * right[2]
          };
        }
      }
      return mat.call(this, loc, transpose, value);
    };
  });
  const button = name => page.getByRole('button', { name, exact: true }),
    state = () => gameState(page);
  const cdp = await page.context().newCDPSession(page);
  await page.goto(`${base}/play.html`);
  await button('Press START').click({ timeout: 120000 });
  const id = Number(process.env.ATV_CONTROL_MODEL || 29);
  await button('Single Player').click();
  await button('Quad and Rider').click();
  for (let i = 0; i < Math.min(id - 1, 47 - id); i++)
    await button(id - 1 <= 47 - id ? 'Next quad' : 'Previous quad').click();
  await page.waitForFunction(
    id => [...document.querySelectorAll('.model-view canvas')].every(c => c.dataset.selection === `${id}:0`),
    id
  );
  await button('Done').click();
  await button('Freestyle').click();
  await button('Ride Caribou Peak').click();
  await waitForGame(page, {
    until: () => document.querySelector('#frontend').hidden && window.atvTerrainStatus.simulation
  });
  const report = { id, turns: [], errors };
  for (const direction of [-1, 1]) {
    const reset = (await state()).frame.reset_count;
    await button('Pause').click();
    await button('Reset ATV').click();
    await page.waitForFunction(n => window.atvTerrainStatus.frame.reset_count > n, reset);
    await cdp.send('Input.dispatchTouchEvent', {
      type: 'touchStart',
      touchPoints: [{ id: 1, x: 180, y: 620, radiusX: 6, radiusY: 6, force: 1 }]
    });
    await page.waitForFunction(() => window.atvTerrainStatus.frame.speed > 12, null, { timeout: 15000 });
    const before = await state();
    const last = await page.evaluate(() => window.controlFrames.at(-1));
    const look = last.camera.target.map((v, i) => v - last.camera.position[i]);
    const right = [look[2], 0, -look[0]],
      len = Math.hypot(...right);
    right.forEach((v, i) => (right[i] = v / len));
    const fullLock = process.env.ATV_CONTROL_FULL_LOCK === '1';
    await cdp.send('Input.dispatchTouchEvent', {
      type: 'touchMove',
      touchPoints: [{ id: 1, x: 180 + direction * (fullLock ? 100 : 50), y: 620, radiusX: 6, radiusY: 6, force: 1 }]
    });
    await page.waitForFunction(() => Math.abs(window.atvTerrainStatus.input.steer) > 0.4);
    await waitForSteps(page, 27);
    const after = await state(),
      delta = after.frame.origin.map((v, i) => v - before.frame.origin[i]);
    if (fullLock) assert(Math.abs(after.input.steer) > 0.95, 'Extended reach gate must actually request full steering');
    const [w, x, y, z] = after.frame.orientation,
      forward = [2 * (x * z + w * y), 2 * (y * z - w * x), 1 - 2 * (x * x + y * y)];
    const side = delta.reduce((s, v, i) => s + v * right[i], 0),
      heading = forward.reduce((s, v, i) => s + v * right[i], 0);
    assert(side * direction > 0.15, `Touch ${direction}: on-screen side ${side}`);
    assert(heading * direction > 0.05, `Touch ${direction}: on-screen heading ${heading}`);
    const bank = await page.evaluate(() => window.bikePresentationProbe.bank);
    assert(
      bank && bank.body * direction > 0.025,
      `Rendered bike banks away from ${direction}: ${JSON.stringify(bank)}`
    );
    assert(bank.rider * direction > 0.025, `Rendered rider banks away from ${direction}: ${JSON.stringify(bank)}`);
    report.turns.push({ direction, side, heading, bank, steer: after.input.steer });
    if (process.env.ATV_CONTROL_SCREENSHOTS)
      await page.screenshot({ path: `${process.env.ATV_CONTROL_SCREENSHOTS}-${direction < 0 ? 'left' : 'right'}.png` });
    if (fullLock) await waitForSteps(page, 96); // Observe low-speed, full-lock reach after the initial turn.
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchCancel', touchPoints: [] });
    await page.waitForFunction(() => !window.atvTerrainStatus.input.touch_drive.held);
    await waitForSteps(page, 15);
  }
  const frames = await page.evaluate(() => window.controlFrames.filter(s => s.frame.vehicle_kind === 'motorcycle'));
  const normal = frames.filter(s => s.frame.rider_grip_errors && s.frame.wreck.phase === 'Riding');
  report.maxGrip = Math.max(...normal.flatMap(s => s.frame.rider_grip_errors));
  report.maxFoot = Math.max(...normal.flatMap(s => s.frame.rider_foot_errors));
  assert(report.maxGrip < 0.08, `Hands leave source grips: ${report.maxGrip}`);
  assert(report.maxFoot < 0.08, `Feet leave source pegs: ${report.maxFoot}`);
  assert(
    normal.every(s => Math.abs(s.frame.front_wheel_local_position[0]) < 0.001),
    'Steering must preserve front wheel/fork pivot line'
  );
  const consecutive = normal.slice(1).filter((s, i) => s.frame.reset_count === normal[i].frame.reset_count);
  report.maxPelvisStep = Math.max(
    ...consecutive.map(s => {
      const i = normal.indexOf(s),
        a = normal[i - 1].frame.rider_node_matrices[0],
        b = s.frame.rider_node_matrices[0];
      return Math.hypot(...b.slice(12, 15).map((v, j) => v - a[12 + j]));
    })
  );
  assert(report.maxPelvisStep < 0.2, `Normal pose snaps: ${report.maxPelvisStep}`);
  report.frames = normal.length;
  report.isolation = await verifyCrashIsolation(page);
  assertNoErrors(errors);
  assert.equal((await state()).glError, 0);
  report.passed = true;
  report.presentation = await page.evaluate(() => window.bikePresentationProbe);
  assert(report.presentation.blends > 10, 'Actual GPU draws must use intermediate motorcycle poses');
  assert(report.presentation.contacts > 10, 'Actual GPU draws must compare rider and bike markers');
  assert(report.presentation.maxGrip < 0.08, `Rendered hands disconnect: ${report.presentation.maxGrip}`);
  assert(report.presentation.maxFoot < 0.08, `Rendered boots disconnect: ${report.presentation.maxFoot}`);
  await writeFile(
    process.env.ATV_CONTROL_REPORT || 'work/verification/motorcycle-controls.json',
    JSON.stringify(report, null, 2)
  );
  console.log(JSON.stringify(report));
} finally {
  await browser.close();
}
