// Host-only drop-in multiplayer gate with actual WASM, content and the Rust relay.
// Covers hosting straight into a ride, joining by invite at any time, meeting
// up beside a rider, remote motion smoothness, in-place ATV changes, host track
// changes, reload slot recovery, host handoff and reopening an empty room.
//   PLAYWRIGHT_MODULE=/path/to/playwright ATV_TEST_URL=http://127.0.0.1:4187 \
//   node tools/test_multiplayer_dropin.mjs
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
const { chromium } = createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || 'playwright');
const base = (process.env.ATV_TEST_URL || 'http://127.0.0.1:4187').replace(/\/$/, '');
const shots = process.env.ATV_MP_SCREENSHOT;
const browser = await chromium.launch({ headless: true, args: process.env.ATV_BROWSER_GPU === 'metal' ? ['--enable-gpu', '--use-angle=metal'] : [] });
const errors = [], report = {};
async function rider(viewport = { width: 1280, height: 896 }) {
  const context = await browser.newContext({ viewport, hasTouch: true });
  await context.grantPermissions(['clipboard-read', 'clipboard-write'], { origin: base });
  const page = await context.newPage();
  page.on('pageerror', error => errors.push(error.message));
  await page.addInitScript(() => { Object.defineProperty(document, 'hasFocus', { value: () => true }); });
  return page;
}
const button = (page, name) => page.getByRole('button', { name, exact: true });
const state = page => page.evaluate(() => window.atvTerrainStatus);
const distance = (a, b) => Math.hypot(...a.map((v, i) => v - b[i]));
const riding = (page, map, remotes = 1, timeout = 120000) => page.waitForFunction(([map, remotes]) => {
  const s = window.atvTerrainStatus, m = s?.multiplayer;
  if (s?.multiplayerError) return true;
  return s?.simulation && m?.state?.environment === map && m.state.lobby?.started && m.remote.filter(r => !r.stale).length === remotes && (remotes === 0 || s.multiplayerDraws > 0);
}, [map, remotes], { timeout });
const clean = async page => { const s = await state(page); assert.equal(s.multiplayerError, undefined, s.multiplayerError); assert.equal(s.glError ?? 0, 0); return s; };
try {
  // 1. Host from the menus: Multiplayer → Host Game → Select Track → Ride.
  const a = await rider();
  await a.goto(base + '/play.html');
  await button(a, 'Press START').click({ timeout: 90000 });
  await button(a, 'Multiplayer').click();
  await button(a, 'Host Game').click();
  await a.getByLabel('Rider name').fill('Host');
  await button(a, 'Select Track').click();
  await a.locator('button.ms-ride').click({ timeout: 60000 });
  await a.waitForFunction(() => window.atvTerrainStatus?.simulation && window.atvTerrainStatus.multiplayer?.state?.lobby?.started || window.atvTerrainStatus?.multiplayerError, null, { timeout: 120000 });
  let s = await clean(a);
  const code = s.multiplayer.state.code, map = s.multiplayer.state.environment;
  assert.match(code, /^[A-HJ-NP-Z2-9]{8}$/); assert.match(s.multiplayer.state.key, /^[0-9a-f]{32}$/);
  assert.equal(s.multiplayer.state.peer, 0); assert.equal(s.multiplayer.state.lobby.host, 0);
  report.hosted = { code, map, players: s.multiplayer.state.lobby.players.length };

  // 2. A friend opens the invite link while the host is already riding.
  const b = await rider();
  await b.goto(`${base}/play.html?room=${code}&quad=14&rider=1&player=Guest`);
  await riding(b, map); await riding(a, map);
  s = await clean(b);
  assert.equal(s.multiplayer.state.peer, 1);
  // Arrivals are placed beside a rider who is already there.
  await b.waitForFunction(() => window.atvTerrainStatus.frame?.origin, null, { timeout: 30000 });
  await b.waitForTimeout(4000);
  const [ha, hb] = [await state(a), await state(b)];
  report.meetDistanceFt = distance(ha.frame.origin, hb.frame.origin);
  assert.ok(report.meetDistanceFt < 40, `guest should meet beside the host, was ${report.meetDistanceFt.toFixed(1)} ft`);
  if (shots) { await a.screenshot({ path: shots.replace('.png', '-meet-host.png') }); await b.screenshot({ path: shots.replace('.png', '-meet-guest.png') }); }

  // 3. Remote motion: the host drives; the guest samples presented positions every frame.
  await b.evaluate(() => { window.trace = []; const step = () => { const p = window.atvTerrainStatus.multiplayer?.remote?.[0]; if (p) window.trace.push([performance.now(), ...p.position]); if (window.trace.length < 600) requestAnimationFrame(step); }; requestAnimationFrame(step); });
  await a.bringToFront(); await a.evaluate(() => window.dispatchEvent(new Event('focus')));
  await a.keyboard.down('KeyW'); await a.waitForTimeout(4000); await a.keyboard.up('KeyW');
  const trace = await b.evaluate(() => window.trace);
  // Presentation updates asynchronously; compare speeds between updates only.
  const updates = trace.filter((p, i) => i === 0 || distance(p.slice(1), trace[i - 1].slice(1)) > 1e-4);
  const steps = updates.slice(1).map((p, i) => [p[0] - updates[i][0], distance(p.slice(1), updates[i].slice(1))]);
  const speeds = steps.filter(([dt]) => dt > 4).map(([dt, d]) => d / dt * 1000);
  const accel = speeds.slice(1).map((v, i) => Math.abs(v - speeds[i]));
  const moved = distance(trace.at(-1).slice(1), trace[0].slice(1));
  accel.sort((x, y) => x - y);
  report.remoteMotion = { frames: trace.length, updates: updates.length, movedFt: moved, p95SpeedChangeFtPerS: accel[Math.floor(accel.length * .95)] ?? 0, maxSpeedChangeFtPerS: accel.at(-1) ?? 0 };
  assert.ok(moved > 10, 'remote host motion should reach the guest');

  // 4. The guest changes ATV mid-session from Pause → Change ATV and Rider.
  await b.bringToFront(); await b.evaluate(() => window.dispatchEvent(new Event('focus')));
  await b.locator('#menu').click();
  await button(b, 'Change ATV and Rider').click();
  await button(b, 'Done').waitFor({ timeout: 30000 });
  await b.waitForFunction(() => !document.querySelector('#menu-buttons [aria-busy=true]'));
  await b.locator('[data-action=quadnext]').click();
  await b.waitForFunction(() => document.querySelector('.ms-ride-name')?.textContent?.includes('Duncan'), null, { timeout: 10000 }).catch(() => {});
  await button(b, 'Done').click();
  await a.waitForFunction(() => window.atvTerrainStatus.multiplayer?.state?.lobby?.players.find(p => p.id === 1)?.atv_model === 15, null, { timeout: 60000 });
  await riding(b, map); await riding(a, map);
  await a.waitForFunction(() => window.atvTerrainStatus.multiplayer?.remote?.find(r => r.peer === 1 && !r.stale)?.atv_model === 15, null, { timeout: 60000 });
  await clean(a);
  report.vehicleChange = { guestQuad: 15 };

  // 5. The host moves everyone to another track from Pause → Select Track.
  await a.bringToFront(); await a.evaluate(() => window.dispatchEvent(new Event('focus')));
  await a.locator('#menu').click();
  await button(a, 'Select Track').click();
  await a.locator('button.ms-ride').waitFor({ timeout: 60000 });
  await a.locator('[data-action=right]').click();
  const next = await a.evaluate(() => window.atvFrontendTrack?.() ?? null);
  await a.locator('button.ms-ride').click();
  await a.waitForFunction(old => window.atvTerrainStatus.multiplayer?.state?.environment && window.atvTerrainStatus.multiplayer.state.environment !== old, map, { timeout: 30000 });
  const map2 = (await state(a)).multiplayer.state.environment;
  await riding(a, map2); await riding(b, map2);
  await clean(b);
  report.trackChange = { from: map, to: map2, expected: next };

  // 6. Reloading the guest tab reclaims the same slot with its kept key.
  await b.reload();
  await riding(b, map2); await riding(a, map2);
  s = await clean(b);
  assert.equal(s.multiplayer.state.peer, 1, 'reload keeps the rider slot and color');
  report.reloadSlot = s.multiplayer.state.peer;

  // 7. Host leaves: the guest keeps riding and becomes the host.
  await a.close();
  await b.waitForFunction(() => { const m = window.atvTerrainStatus.multiplayer; return m?.state?.lobby?.host === m.state.peer && m.state.lobby.players.length === 1; }, null, { timeout: 60000 });
  s = await clean(b);
  assert.ok(s.simulation, 'guest ride continues after the host leaves');
  report.handoff = { host: s.multiplayer.state.lobby.host };
  await b.locator('#menu').click();
  await button(b, 'Select Track').waitFor({ timeout: 15000 });
  await b.keyboard.press('Escape');

  // 8. A late friend can still use the original link.
  const c = await rider({ width: 390, height: 844 });
  await c.goto(`${base}/play.html?room=${code}&player=Late`);
  await riding(c, map2); await riding(b, map2);
  await clean(c);
  if (shots) await c.screenshot({ path: shots.replace('.png', '-late-portrait.png') });
  report.lateJoin = true;

  // 9. After everyone leaves and the room expires, the same link opens it again.
  if (process.env.ATV_MP_REOPEN !== '0') {
    await b.close(); await c.close();
    await new Promise(resolve => setTimeout(resolve, 100000));
    const d = await rider();
    await d.goto(`${base}/play.html?room=${code}&map=env05&player=Again`);
    await riding(d, 'env05', 0);
    s = await clean(d);
    assert.equal(s.multiplayer.state.code, code); assert.equal(s.multiplayer.state.lobby.host, s.multiplayer.state.peer);
    report.reopened = { code, map: s.multiplayer.state.environment };
  }
  assert.deepEqual(errors, []);
  console.log(JSON.stringify({ result: 'pass', ...report }, null, 2));
} catch (error) {
  console.log(JSON.stringify({ result: 'fail', error: error.message, errors, ...report }, null, 2));
  process.exitCode = 1;
} finally {
  await browser.close();
}
