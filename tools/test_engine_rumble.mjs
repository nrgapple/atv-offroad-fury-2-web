// Controller rumble in the Rust engine's browser build (Rumble and
// ImpactEvents: web/graphics.js, web/render-support.js). An autodriven ENV03
// run into a Gorn wreck pulses a recording vibrationActuator stand-in on a
// fake standard pad ('dual-rumble', strong 0.2-1, 70-300 ms); with
// Settings → Vibration Off nothing is sent. Host browser evidence; the
// impact and pulse rules are checked against the JavaScript values in
// atv-game's unit test. Real controllers are not exercised here.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, url } from './browser/harness.mjs';

const script = [{ frames: 60, input: {} }, { frames: 673, input: { steer: -0.5, lean: 0, keyboard_drive: { held: true, axis: 1, generation: 0 } } }];
const browser = await launch();
try {
  for (const vibration of ['on', 'off']) {
    const errors = [];
    const page = await newPage(browser, { viewport: { width: 640, height: 360 } }, errors);
    await page.addInitScript(vibration => {
      localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
      localStorage.setItem('atv-vibration', vibration);
      const effects = window.rumbleLog = [];
      const actuator = { playEffect: async (kind, options) => { effects.push({ kind, ...options }); return 'complete'; } };
      // A pad with no input: gilrs sees a standard pad, the rumble finds its actuator.
      const real = navigator.getGamepads?.bind(navigator);
      const pad = { id: 'Rumble stand-in (STANDARD GAMEPAD)', index: 0, connected: true, mapping: 'standard', timestamp: 1,
        axes: [0, 0, 0, 0], buttons: Array.from({ length: 17 }, () => ({ pressed: false, touched: false, value: 0 })), vibrationActuator: actuator };
      navigator.getGamepads = () => { const list = real ? [...real()] : []; list[0] = pad; return list; };
    }, vibration);
    await page.goto(url(`/engine.html?venue=env03&crash=gorn&weather=off&uiDiagnostics=1&autodrive=${encodeURIComponent(JSON.stringify(script))}`));
    // Focus the canvas once the ride is up (impacts count only while focused).
    await page.waitForSelector('#ui-state', { state: 'attached', timeout: 120000 });
    await page.mouse.click(320, 180);
    await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent ?? 'null')?.step >= 733
      || /error|fail/i.test(document.querySelector('#status')?.textContent), null, { timeout: 240000 });
    const state = await page.evaluate(() => JSON.parse(document.querySelector('#ui-state').textContent));
    const effects = await page.evaluate(() => window.rumbleLog);
    const v = state.settings.vibration;
    assert.equal(v.enabled, vibration === 'on');
    assert.ok(v.impacts > 0, JSON.stringify(v));
    if (vibration === 'on') {
      assert.ok(effects.length > 0 && effects.length === v.pulses, JSON.stringify({ v, effects }));
      for (const e of effects) {
        assert.equal(e.kind, 'dual-rumble');
        assert.ok(e.strongMagnitude >= 0.2 && e.strongMagnitude <= 1 && e.duration >= 70 && e.duration <= 300, JSON.stringify(e));
        assert.ok(Math.abs(e.weakMagnitude - Math.min(1, 0.25 + e.strongMagnitude * 0.75)) < 1e-9);
      }
    } else assert.equal(effects.length, 0);
    assertNoErrors(errors);
    console.log(`  vibration ${vibration}: ${v.impacts} impacts, ${effects.length} pulses ${JSON.stringify(effects.map(e => [+e.strongMagnitude.toFixed(2), e.duration]))}`);
    await page.close();
  }
  console.log('PASS: Rust controller rumble');
} finally {
  await browser.close();
}
