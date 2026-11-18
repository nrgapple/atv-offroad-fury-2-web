// Accelerate and brake with real keyboard controls, using both vehicle rigs.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const errors = [];
const browser = await launch();
try {
  for (const [quad, mode] of [[1, 'sport'], [29, 'simulation']]) {
    const page = await newPage(browser, { viewport: { width: 960, height: 540 } }, errors);
    await page.addInitScript(mode => {
      localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
      localStorage.setItem('atv-handling', mode);
    }, mode);
    await page.goto(url(`/play.html?venue=env01&quad=${quad}&weather=off&uiDiagnostics=1`));
    await page.waitForFunction(mode =>
      JSON.parse(document.querySelector('#ui-state')?.textContent ?? 'null')?.settings?.handling === mode,
    mode, { timeout: 180000 });
    await page.mouse.click(480, 300);
    const settings = () => page.evaluate(() => JSON.parse(document.querySelector('#ui-state').textContent).settings);
    assert.equal((await settings()).hotRotors, false);
    for (let stop = 0; stop < 3 && !(await settings()).hotRotors; stop++) {
      await page.keyboard.down('KeyW');
      await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state').textContent).ride.speed > 30,
        null, { timeout: 60000 });
      await page.keyboard.up('KeyW');
      await page.keyboard.down('KeyS');
      await nextFrames(page, 20);
      await page.keyboard.up('KeyS');
    }
    const { brakeHeat, hotRotors } = await settings();
    assert.ok(brakeHeat > 0.02, `vehicle ${quad} reached brake heat ${brakeHeat}`);
    assert.ok(hotRotors, `vehicle ${quad} drew hot rotors at heat ${brakeHeat}`);
    if (process.env.ATV_SCREENSHOTS) {
      await page.keyboard.down('KeyX');
      await nextFrames(page, 18);
      await page.keyboard.up('KeyX');
      await page.screenshot({ path: `${process.env.ATV_SCREENSHOTS}/braking-${quad}.png` });
    }
    await page.close();
  }
  assertNoErrors(errors);
  console.log('Sport ATV and Simulation RM125 drew heated rotors after braking');
} finally {
  await browser.close();
}
