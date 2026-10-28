// Original scene-model visibility policy in /play.html (research/VISIBILITY.md).
//
// 1. env06 (Original preset): the nearest multi-level scene model draws its
//    finest level inside its first switch distance, the next level between
//    switches and its coarsest level far away (001b3a90), with fixed cameras
//    looking at it; its finest level's geometry stays drawn up close.
// 2. The same far camera under High (graphics enhancement) keeps every model
//    at its finest level.
// 3. env04: the drop-gate light pass (drawgate4, EnableMultiPassCull on) is
//    culled beyond 130 ft and drawn inside it (001b2390 -> 001b90b0); High
//    keeps it.
// 4. Every load reports one zone, no portals and no zone reference outside it.
// ATV_VISIBILITY_SCREENSHOT=<prefix> saves the near and far views.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, optionalScreenshot, url } from './browser/harness.mjs';

const FIXED_FOCAL = 1.7;
const state = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));

async function open(page, venue, graphics, camera) {
  const view = camera ? `&camera=${camera.join(',')}` : '';
  await page.goto(url(`/play.html?venue=${venue}&uiDiagnostics=1&weather=off&graphics=${graphics}${view}`));
  await page.waitForFunction(
    () => {
      const s = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}');
      return s.step > 5 && s.scenery;
    },
    null,
    { timeout: 120000 }
  );
  await nextFrames(page, 6);
  const s = (await state(page)).scenery;
  assert.equal(s.zones, 1, 'one scene zone');
  assert.equal(s.portals, 0, 'no portals');
  assert.equal(s.zonesOutside, 0, 'every zone reference is in the scene zone');
  return s;
}

// A fixed camera `distance` ft from `center`, raised 20 degrees, looking at it.
function cameraAt(center, distance, heading = 0.7) {
  const up = Math.sin((20 * Math.PI) / 180) * distance;
  const across = Math.cos((20 * Math.PI) / 180) * distance;
  const eye = [center[0] + Math.sin(heading) * across, center[1] + up, center[2] + Math.cos(heading) * across];
  const yaw = Math.atan2(center[0] - eye[0], center[2] - eye[2]);
  const pitch = Math.atan2(center[1] - eye[1], Math.hypot(center[0] - eye[0], center[2] - eye[2]));
  return [...eye, yaw, pitch, FIXED_FOCAL];
}

const browser = await launch({ args: ['--autoplay-policy=no-user-gesture-required'] });
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 960, height: 540 } }, errors);
  await page.addInitScript(() => localStorage.setItem('atv2-keyboard-help-seen-v1', '1'));

  // 1. Pick a multi-level model with at least two coarser levels if one is near.
  let s = await open(page, 'env06', 'original');
  assert.ok(s.nearest.length > 0, 'env06 has multi-level models');
  const target = s.nearest.find(m => m.switches.length >= 2) || s.nearest[0];
  // Switch distances for the fixed camera's focal: radius + threshold * F.
  const fixedFocal = 240 * FIXED_FOCAL;
  const thresholds = target.switches.map(d => (d - target.radius) / s.focal);
  const switches = thresholds.map(t => target.radius + t * fixedFocal);
  const levelOf = scenery => Number(scenery.modelLevels[target.model]);
  console.log(`target ${target.name} r=${target.radius.toFixed(1)} switches ${switches.map(d => d.toFixed(1))}`);

  const near = Math.max(switches[0] - 8, target.radius * 0.5 + 1);
  s = await open(page, 'env06', 'original', cameraAt(target.center, near));
  assert.equal(levelOf(s), 0, `finest level at ${near.toFixed(1)} ft`);
  assert.ok(s.levels[0] > 0, `finest geometry drawn near: ${JSON.stringify(s.levels)}`);
  const nearDraws = s.draws;
  await optionalScreenshot(page, 'ATV_VISIBILITY_SCREENSHOT', 'near');

  const mid = switches.length >= 2 ? (switches[0] + switches[1]) / 2 : switches[0] + 20;
  s = await open(page, 'env06', 'original', cameraAt(target.center, mid));
  assert.equal(levelOf(s), 1, `next level at ${mid.toFixed(1)} ft`);

  const far = switches[switches.length - 1] + 60;
  s = await open(page, 'env06', 'original', cameraAt(target.center, far));
  assert.equal(levelOf(s), switches.length, `coarsest level at ${far.toFixed(1)} ft`);
  assert.ok(s.coarseModels > 0);
  assert.ok(s.levels.slice(1).some(n => n > 0), `coarser geometry drawn far: ${JSON.stringify(s.levels)}`);
  const original = s;
  await optionalScreenshot(page, 'ATV_VISIBILITY_SCREENSHOT', 'far-original');

  // 2. High keeps the finest level everywhere.
  s = await open(page, 'env06', 'high', cameraAt(target.center, far));
  assert.equal(s.finest, true);
  assert.equal(levelOf(s), 0, 'High: finest level far away');
  assert.equal(s.coarseModels, 0);
  assert.deepEqual(s.levels.slice(1), [0, 0, 0]);
  assert.ok(s.triangles >= original.triangles, `High ${s.triangles} vs Original ${original.triangles} triangles`);
  await optionalScreenshot(page, 'ATV_VISIBILITY_SCREENSHOT', 'far-high');
  console.log(
    `env06 near ${nearDraws} draws; far Original ${original.draws} draws/${original.triangles} tris ` +
      `levels ${JSON.stringify(original.levels)}; High ${s.draws}/${s.triangles}`
  );
  assertNoErrors(errors);

  // 3. The drop gate's light pass beyond and inside 130 ft.
  s = await open(page, 'env04', 'original');
  const gate = s.farCullModels.find(m => /drawgate/i.test(m.name));
  assert.ok(gate, JSON.stringify(s.farCullModels));
  s = await open(page, 'env04', 'original', cameraAt(gate.center, 200));
  let watched = s.farCullModels.find(m => m.model === gate.model);
  assert.equal(watched.far, true, `far at ${watched.distance.toFixed(1)} ft`);
  assert.ok(s.farCulled >= 1, `later pass culled: ${s.farCulled}`);
  s = await open(page, 'env04', 'original', cameraAt(gate.center, 60));
  watched = s.farCullModels.find(m => m.model === gate.model);
  assert.equal(watched.far, false, `near at ${watched.distance.toFixed(1)} ft`);
  assert.equal(s.farCulled, 0, 'every pass draws inside 130 ft');
  s = await open(page, 'env04', 'high', cameraAt(gate.center, 200));
  assert.equal(s.farCulled, 0, 'High draws every pass');
  assertNoErrors(errors);

  // 4. A Supercross stadium and a National race keep the single zone.
  await open(page, 'sx01', 'original');
  await page.goto(url('/play.html?venue=nat03&race=1&laps=1&cpu=0&uiDiagnostics=1&weather=off&graphics=original'));
  await page.waitForFunction(
    () => {
      const s = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}');
      return s.step > 5 && s.scenery;
    },
    null,
    { timeout: 120000 }
  );
  s = (await state(page)).scenery;
  assert.equal(s.zones, 1);
  assert.equal(s.zonesOutside, 0);
  assert.ok(s.zoneRecords > 0, 'the race TOY has zone references');
  assertNoErrors(errors);
  console.log('visibility: LOD levels, far multi-pass cull, High finest and single zone pass');
} finally {
  await browser.close();
}
