// Alpha Arcade flips on touch, Rust engine against the JavaScript game (the
// pinned pre-switch reference at ATV_JS_URL: python3 tools/js_reference.py
// serve; manual group). On env01's Caribou ramp each engine jumps from
// the one-handed thumb (hold into the ramp, lift for the preload jump), then
// in the air: (1) a new touch pulled down leans back into a backflip, and
// (2) a sideways drag past full steering lock spins a 360 (touch.js
// arcadeSpinAxis: steering keeps the original air lean until then). As a
// control, (3) the keyboard jumps and holds lean back. The commanded rotation
// rates (atv-core arcade_flips) must match the JavaScript game's.
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=<Rust site> ATV_JS_URL=<JS site> node tools/test_engine_arcade_touch.mjs
import assert from 'node:assert/strict';
import { assertNoErrors, base, jsBase, launch, newPage } from './browser/harness.mjs';

if (!process.env.ATV_JS_URL) throw new Error('set ATV_JS_URL to the JavaScript reference (python3 tools/js_reference.py serve)');
const report = {};
const errors = [];

// One engine's page and how to read its simulation frame.
const ENGINES = {
  rust: {
    url: `${base}/play.html?venue=env01&weather=off&uiDiagnostics=1`,
    read: () => {
      const s = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}');
      return {
        ready: s.active === true,
        speed: s.speed ?? 0,
        armed: Boolean(s.preloadArmed),
        grounded: (s.grounded ?? []).some(Boolean),
        arcade: s.flip?.arcade ?? null
      };
    }
  },
  js: {
    url: `${jsBase}/play.html?drive=1&environment=env01`,
    read: () => {
      const s = window.atvTerrainStatus;
      const f = s?.frame;
      return {
        ready: Boolean(s?.simulation && f),
        speed: f?.speed ?? 0,
        armed: Boolean(f?.preload_armed),
        grounded: (f?.grounded ?? []).some(Boolean),
        arcade: f?.flip?.arcade ?? null
      };
    }
  }
};

async function open(browser, name, touch) {
  const options = touch
    ? { viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true }
    : { viewport: { width: 1280, height: 720 } };
  const page = await newPage(browser, options, errors);
  await page.addInitScript(() => {
    Object.defineProperty(document, 'hasFocus', { value: () => true });
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.setItem('atv-alpha', JSON.stringify(['arcade-flips']));
  });
  await page.goto(ENGINES[name].url);
  const read = () => page.evaluate(ENGINES[name].read);
  // Polled from Node: the page's CSP has no unsafe-eval for predicates.
  const until = async (predicate, timeout = 60000) => {
    const deadline = Date.now() + timeout;
    for (;;) {
      const f = await read().catch(() => null);
      if (f && predicate(f)) return f;
      if (Date.now() > deadline) throw new Error(`${name}: timed out waiting for ${predicate}`);
      await page.waitForTimeout(16); // Time-based on purpose: the poll interval.
    }
  };
  await until(f => f.ready, 180000);
  await page.bringToFront();
  return { page, read, until };
}

// Rotation for 1.2 s after the air gesture (or until touchdown): the peak
// commanded nose-up and spin rates, and whether the controller engaged.
async function rotation(read, page) {
  const samples = [];
  const started = Date.now();
  while (Date.now() - started < 1200) {
    const f = await read();
    if (f.arcade) samples.push(f.arcade);
    if (f.grounded && samples.length > 3) break;
    await page.waitForTimeout(16); // Time-based on purpose: sample the live rotation.
  }
  return {
    engaged: samples.some(a => a.engaged),
    pitchRate: Math.max(0, ...samples.map(a => a.rate[0])),
    yawRate: Math.max(0, ...samples.map(a => Math.abs(a.rate[1]))),
    pitch: Math.max(0, ...samples.map(a => a.pitch))
  };
}

async function touchJump(browser, name, gesture) {
  const { page, read, until } = await open(browser, name, true);
  const cdp = await page.context().newCDPSession(page);
  const touch = (type, points = []) =>
    cdp.send('Input.dispatchTouchEvent', {
      type,
      touchPoints: points.map(point => ({ radiusX: 6, radiusY: 6, force: 1, ...point }))
    });
  // Hold the one-handed thumb into the ramp; lift for the preload jump.
  const thumb = { id: 1, x: 200, y: 600 };
  await touch('touchStart', [thumb]);
  await until(f => f.armed, 45000);
  await page.waitForTimeout(330); // About 20 steps of preload, as test_arcade_flips.mjs.
  await touch('touchEnd', [thumb]);
  await until(f => !f.grounded, 5000);
  const air = gesture === 'back' ? { id: 2, x: 200, y: 520 } : { id: 2, x: 110, y: 520 };
  await touch('touchStart', [air]);
  // Back: 80 px down (lean 1). Spin: 170 px right, past full lock.
  for (let i = 1; i <= 17; i++) {
    const to = gesture === 'back' ? { ...air, y: air.y + Math.min(80, i * 10) } : { ...air, x: air.x + i * 10 };
    await touch('touchMove', [to]);
  }
  const result = await rotation(read, page);
  await touch('touchEnd', [air]);
  await page.close();
  return result;
}

async function keyboardJump(browser, name) {
  const { page, read, until } = await open(browser, name, false);
  await page.locator('canvas').first().click({ position: { x: 640, y: 360 } });
  await page.keyboard.down('KeyW');
  await until(f => f.speed > 30, 30000);
  await page.keyboard.down('Space');
  await until(f => f.armed, 10000);
  await page.waitForTimeout(330);
  await page.keyboard.up('Space');
  await until(f => !f.grounded, 5000);
  await page.keyboard.up('KeyW');
  await page.keyboard.down('KeyE');
  const result = await rotation(read, page);
  await page.keyboard.up('KeyE');
  await page.close();
  return result;
}

const browser = await launch();
try {
  for (const name of ['js', 'rust']) {
    report[name] = {
      touchBack: await touchJump(browser, name, 'back'),
      touchSpin: await touchJump(browser, name, 'spin'),
      keyboardBack: await keyboardJump(browser, name)
    };
  }
  console.log(JSON.stringify(report));
  const near = (a, b) => Math.abs(a - b) <= 0.15 * Math.max(a, b);
  for (const [check, rate] of [
    ['touchBack', 'pitchRate'],
    ['touchSpin', 'yawRate'],
    ['keyboardBack', 'pitchRate']
  ]) {
    const [js, rust] = [report.js[check], report.rust[check]];
    assert.ok(js.engaged && js[rate] > 3, `the JavaScript game rotates on ${check}: ${JSON.stringify(js)}`);
    assert.ok(rust.engaged, `Rust engages Arcade flips on ${check}: ${JSON.stringify(rust)}`);
    assert.ok(near(js[rate], rust[rate]), `${check} ${rate}: Rust ${rust[rate].toFixed(2)} vs JavaScript ${js[rate].toFixed(2)}`);
  }
  assertNoErrors(errors);
  console.log(JSON.stringify(report, null, 2));
} finally {
  await browser.close();
}
