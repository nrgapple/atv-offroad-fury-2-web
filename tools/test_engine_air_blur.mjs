// Alpha Air dark blur in the Rust engine's browser build (AirBlurRenderer in
// web/hype-gl.js): holding throttle over the ENV05 hills, big air raises the
// director's blur and the screen pass draws it; with the toggle off the
// amount stays 0. Host browser evidence (live keyboard ride); a screenshot
// at the strongest blur lands in private/reports when ATV_SHOT is set.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, url } from './browser/harness.mjs';

const browser = await launch();
try {
  for (const on of [true, false]) {
    const errors = [];
    const page = await newPage(browser, { viewport: { width: 960, height: 540 } }, errors);
    await page.addInitScript(on => {
      localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
      localStorage.setItem('atv-alpha', JSON.stringify(on ? ['air-blur'] : []));
      localStorage.setItem('atv-graphics', 'original');
    }, on);
    await page.goto(url('/play.html?venue=env05&weather=off&uiDiagnostics=1'));
    await page.waitForSelector('#ui-state', { state: 'attached', timeout: 120000 });
    await page.mouse.click(480, 300);
    await page.keyboard.down('ArrowUp');
    const read = () => page.evaluate(() => JSON.parse(document.querySelector('#ui-state').textContent).settings);
    let peak = 0, director = 0;
    for (let i = 0; i < 80 && !(on && peak > 0.3); i++) {
      await page.waitForTimeout(250);
      const s = await read();
      director = Math.max(director, s.hypeBlur);
      if (s.airBlur > peak) { peak = s.airBlur; if (process.env.ATV_SHOT) await page.screenshot({ path: process.env.ATV_SHOT }); }
    }
    await page.keyboard.up('ArrowUp');
    const s = await read();
    assert.equal(s.alphaAirBlur, on);
    if (on) assert.ok(peak > 0.3, `peak ${peak}`);
    else { assert.equal(peak, 0); assert.ok(director > 0, 'the director still measured big air'); }
    assertNoErrors(errors);
    console.log(`  air blur ${on ? 'on' : 'off'}: peak drawn ${peak.toFixed(2)}, director ${director.toFixed(2)}`);
    await page.close();
  }
  console.log('PASS: Rust Alpha air dark blur');
} finally {
  await browser.close();
}
