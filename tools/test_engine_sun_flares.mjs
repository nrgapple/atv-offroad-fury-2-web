// Authored sun flares (research/SUN_FLARES.md) in ATV2 and MX venues: the
// flare shows while the sun projects on screen with clear terrain toward
// it, hides behind terrain, and is not drawn with the sun behind the view.
// Fixed cameras on the host; not a native-render or phone comparison.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, url } from './browser/harness.mjs';

// venue, camera (x,y,z,yaw,pitch,focal), expected visible, coverage test
const cases = [
  ['env02', '2880,700,2880,1.571,0.2,1.2', true, c => c === 1, 'ATV2 sun over the ridge'],
  ['env02', '2880,700,2880,-1.571,0.2,1.2', false, c => c === 0, 'ATV2 sun behind the camera'],
  ['mx_nat01', '2962,340,2080,-2.207,0,1.2', true, c => c === 1, 'MX Goblin Gulch sunset'],
  ['mx_sm03', '2880,190,2880,-2.069,0.3,1.2', true, c => c === 0, 'MX The Glades sun behind terrain'],
];
const browser = await launch();
try {
  for (const [venue, camera, visible, coverage, label] of cases) {
    const errors = [];
    const page = await newPage(browser, { viewport: { width: 720, height: 405 }, hasTouch: true }, errors);
    await page.addInitScript(() => localStorage.setItem('furyScript', JSON.stringify([{ frames: 60, input: {} }])));
    await page.goto(url(`/play.html?venue=${venue}&quad=29&graphics=original&weather=off&uiDiagnostics=1&traceAll=1&script=storage&camera=${camera}`));
    await page.waitForSelector('#trace', { state: 'attached', timeout: 240000 });
    await page.waitForTimeout(500);
    const state = JSON.parse(await page.locator('#ui-state').textContent());
    assert.equal(state.ride?.environment, venue);
    const [flare] = state.sunFlares ?? [];
    assert.ok(flare, `${label}: the venue's authored flare loaded`);
    assert.equal(flare.visible, visible, `${label}: visible`);
    assert.ok(coverage(flare.coverage), `${label}: coverage ${flare.coverage}`);
    assertNoErrors(errors);
    console.log(`${label}: visible ${flare.visible}, coverage ${flare.coverage}`);
    await page.close();
  }
  console.log('PASS: authored sun flares follow the sun, terrain occlusion and the view');
} finally { await browser.close(); }
