// Alpha Landing burst and Heavy mud in the Rust engine's browser build
// (web/hype-gl.js BurstEmitter, audio.js landingThump, graphics.js
// DirtBuildup.setHeavy/splat). Holding throttle over the ENV05 hills on the
// Original preset: with both on, hard landings throw dust and clods, thump,
// and the quad soils; with both off nothing is emitted and the quad stays
// clean. Host browser evidence (live keyboard ride, so counts vary by run);
// the particle and thump maths are unit-tested against the JavaScript.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, url } from './browser/harness.mjs';

const browser = await launch();
try {
  for (const alpha of [['landing-burst', 'heavy-mud'], []]) {
    const errors = [];
    const page = await newPage(browser, { viewport: { width: 960, height: 540 } }, errors);
    await page.addInitScript(alpha => {
      localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
      localStorage.setItem('atv-alpha', JSON.stringify(alpha));
      localStorage.setItem('atv-graphics', 'original');
    }, alpha);
    await page.goto(url('/engine.html?venue=env05&weather=off&uiDiagnostics=1'));
    await page.waitForSelector('#ui-state', { state: 'attached', timeout: 120000 });
    await page.mouse.click(480, 300);
    await page.keyboard.down('ArrowUp');
    const read = () => page.evaluate(() => JSON.parse(document.querySelector('#ui-state').textContent).settings);
    let s = await read();
    for (let i = 0; i < 60 && !(alpha.length && s.burstLive > 0 && s.thumps > 0); i++) { await page.waitForTimeout(250); s = await read(); }
    await page.keyboard.up('ArrowUp');
    assert.equal(s.alphaLandingBurst, alpha.length > 0);
    assert.equal(s.heavyMud, alpha.length > 0);
    if (alpha.length) {
      assert.ok(s.burstLive > 0 && s.thumps > 0, JSON.stringify(s));
      assert.ok(s.dirtLevel > 0, JSON.stringify(s));
    } else {
      assert.equal(s.burstLive, 0);
      assert.equal(s.thumps, 0);
      assert.equal(s.dirtLevel, 0);
    }
    assertNoErrors(errors);
    console.log(`  alpha ${JSON.stringify(alpha)}: ${s.burstLive} particles, ${s.thumps} thumps, dirt ${s.dirtLevel.toFixed(1)}`);
    await page.close();
  }
  console.log('PASS: Rust Alpha landing burst and heavy mud');
} finally {
  await browser.close();
}
