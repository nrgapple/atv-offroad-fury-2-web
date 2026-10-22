// Alpha Speed & air muffle in the Rust engine's browser build
// (GameAudio.setHype in web/audio.js, the music.js stream graph): holding
// throttle over the ENV05 hills, speed and air raise the director's muffle,
// the mixer's music lowpass/level and wind follow it, and the streamed
// soundtrack moves into a Web Audio lowpass graph and keeps playing. Off,
// nothing is sent and the soundtrack stays on its element. Host browser
// evidence; the filter and wind are unit-tested in atv-game.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, url } from './browser/harness.mjs';

const browser = await launch();
try {
  for (const on of [true, false]) {
    const errors = [];
    const page = await newPage(browser, { viewport: { width: 960, height: 540 } }, errors);
    await page.addInitScript(on => {
      localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
      localStorage.setItem('atv-alpha', JSON.stringify(on ? ['music-muffle'] : []));
      localStorage.setItem('atv-graphics', 'original');
    }, on);
    await page.goto(url('/engine.html?venue=env05&weather=off&uiDiagnostics=1'));
    await page.waitForSelector('#ui-state', { state: 'attached', timeout: 120000 });
    await page.mouse.click(480, 300);
    await page.keyboard.down('ArrowUp');
    const read = () => page.evaluate(() => JSON.parse(document.querySelector('#ui-state').textContent).settings);
    let peak = 0, director = 0, s = await read();
    for (let i = 0; i < 60 && !(on && peak > 0.3 && s.musicStream?.graph); i++) {
      await page.waitForTimeout(250);
      s = await read();
      director = Math.max(director, s.hypeMuffle);
      peak = Math.max(peak, s.musicMuffle);
    }
    const before = s.musicStream?.time ?? 0;
    await page.waitForTimeout(1000);
    const after = await read();
    await page.keyboard.up('ArrowUp');
    assert.equal(after.alphaMuffle, on);
    assert.ok(director > 0.3, `director ${director}`);
    if (on) {
      assert.ok(peak > 0.3, `mixer muffle ${peak}`);
      assert.equal(after.musicStream?.graph, true, JSON.stringify(after.musicStream));
    } else {
      assert.equal(peak, 0);
      assert.notEqual(after.musicStream?.graph, true);
    }
    if (after.musicStream && !after.musicStream.paused) assert.ok(after.musicStream.time > before, 'the soundtrack keeps playing');
    assertNoErrors(errors);
    console.log(`  muffle ${on ? 'on' : 'off'}: director ${director.toFixed(2)}, mixer ${peak.toFixed(2)}, stream ${JSON.stringify(after.musicStream)}`);
    await page.close();
  }
  console.log('PASS: Rust Alpha speed and air muffle');
} finally {
  await browser.close();
}
