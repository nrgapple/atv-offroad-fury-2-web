// Authored sun flares (research/SUN_FLARES.md) in ATV2 and MX venues: the
// flare shows while the sun projects on screen, is not drawn with the sun
// behind the view, and dims by the depth-tested share of the square around
// the sun: terrain and (unlike the terrain-ray fallback) foliage hide it.
// Fixed cameras on the host; not a native-render or phone comparison.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, url } from './browser/harness.mjs';

// venue, camera (x,y,z,yaw,pitch,focal), expected visible, terrain-ray
// coverage test, depth-query coverage test (visible cases)
const cases = [
  ['env02', '2880,700,2880,1.571,0.2,1.2', true, c => c === 1, d => d === 1, 'ATV2 sun over the ridge'],
  ['env02', '2880,700,2880,-1.571,0.2,1.2', false, c => c === 0, null, 'ATV2 sun behind the camera'],
  ['mx_nat01', '2962,340,2080,-2.207,0,1.2', true, c => c === 1, d => d === 1, 'MX Goblin Gulch sunset'],
  ['mx_sm03', '2880,190,2880,-2.069,0.3,1.2', true, c => c === 0, d => d === 0, 'MX The Glades sun behind terrain'],
  ['mx_nat02', '3246,444,1923,2.05,0.1,1.2', true, c => c === 1, d => d > 0 && d < 1, 'MX Tipperary sun in a tree'],
];
const browser = await launch();
try {
  for (const [venue, camera, visible, coverage, depth, label] of cases) {
    const errors = [];
    const page = await newPage(browser, { viewport: { width: 720, height: 405 }, hasTouch: true }, errors);
    await page.addInitScript(() => localStorage.setItem('furyScript', JSON.stringify([{ frames: 120, input: {} }])));
    await page.goto(url(`/play.html?venue=${venue}&quad=29&graphics=original&weather=off&uiDiagnostics=1&traceAll=1&script=storage&camera=${camera}`));
    await page.waitForSelector('#trace', { state: 'attached', timeout: 240000 });
    // The depth queries resolve two frames late and diagnostics read them
    // back a few frames after that: wait for a settled readback.
    let state;
    for (let tries = 0; tries < 50; tries++) {
      await page.waitForTimeout(200);
      state = JSON.parse(await page.locator('#ui-state').textContent());
      const [flare] = state.sunFlares ?? [];
      if (flare?.visible === visible && (!visible || flare.depthCoverage != null)) break;
    }
    assert.equal(state.ride?.environment, venue);
    const [flare] = state.sunFlares ?? [];
    assert.ok(flare, `${label}: the venue's authored flare loaded`);
    assert.equal(flare.visible, visible, `${label}: visible`);
    assert.ok(coverage(flare.coverage), `${label}: coverage ${flare.coverage}`);
    if (depth) {
      assert.ok(flare.queried, `${label}: the sprite reads depth queries`);
      assert.ok(depth(flare.depthCoverage), `${label}: depth coverage ${flare.depthCoverage}`);
    }
    assertNoErrors(errors);
    console.log(`${label}: visible ${flare.visible}, coverage ${flare.coverage}, queried ${flare.queried}, depth ${flare.depthCoverage}`);
    await page.close();
  }
  console.log('PASS: authored sun flares follow the sun, depth occlusion and the view');
} finally { await browser.close(); }
