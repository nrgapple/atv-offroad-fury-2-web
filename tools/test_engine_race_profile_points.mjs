// Race Profile Points in the browser (research/PROFILE.md): the race path
// driver rides the player (`autopilot=1`) through a one-lap Supercross race.
// At the finish the results carry each rider's stunt score, the player's
// Profile Points follow the original single-race rule (001f1e30: 10/5/1 for
// 1st–3rd plus stunt score / 1000 and combo points, nothing for 5th) and the saved profile
// (`fury-profile`) is credited once, keeping earlier points.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, optionalScreenshot, url } from './browser/harness.mjs';

const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 960, height: 540 } }, errors);
  await page.addInitScript(() => {
    if (sessionStorage.getItem('profile-test')) return;
    sessionStorage.setItem('profile-test', '1');
    localStorage.setItem('fury-profile', JSON.stringify({ points: 100, races: 4, wins: 1 }));
    localStorage.setItem('furyScript', JSON.stringify([{ frames: 9000, input: {} }]));
  });
  await page.goto(url('/play.html?venue=sx01&race=1&laps=1&cpu=3&seed=11&weather=off&autopilot=1&script=storage'));
  await page.waitForFunction(() => document.querySelector('#trace') || /error|fail/i.test(document.querySelector('#status')?.textContent), null, { timeout: 420000 });
  const trace = JSON.parse(await page.locator('#trace').textContent());
  const race = (Array.isArray(trace) ? trace.at(-1) : trace).race;
  assert.ok(race, 'race frame in the trace');
  const riders = race.standings.map(s => `${s.position}. ${s.name} ${s.time?.toFixed(2) ?? '-'} stunt ${s.stunt}`).join(', ');
  assert.equal(race.phase, 'finished', `player finished the race: ${riders}`);
  const you = race.standings.find(s => s.player);
  assert.ok(you && you.name === 'YOU', `player row: ${riders}`);
  assert.equal(you.position, race.position);
  for (const s of race.standings) assert.ok(s.stunt >= 0 && Number.isFinite(s.stunt), `stunt score ${riders}`);
  const place = [10, 5, 1][you.position - 1] ?? 0;
  // 001f22a0: each landed combo of n tricks also adds n - 1.
  const combos = race.player_stunts?.combo_points ?? 0;
  const award = you.position === 5 ? 0 : place + Math.trunc(you.stunt / 1000) + combos;
  // The results are drawn on the next frames; the profile is saved then.
  await page.waitForFunction(() => JSON.parse(localStorage.getItem('fury-profile') || '{}').races === 5, null, { timeout: 30000 });
  const profile = JSON.parse(await page.evaluate(() => localStorage.getItem('fury-profile')));
  assert.equal(profile.points, 100 + award, `profile ${JSON.stringify(profile)} after place ${you.position}, award ${award}`);
  assert.equal(profile.wins, 1 + (you.position === 1 ? 1 : 0));
  // The migrated PLAYER1 keeps the course record (00322080): one sx01 race,
  // no best lap (Track flag 8 skips the first lap, 00248b20) and no
  // full-length time for a one-lap race.
  assert.equal(profile.name, 'PLAYER1');
  assert.equal(profile.courses?.sx01?.races, 1, JSON.stringify(profile));
  assert.equal(profile.courses.sx01.best_lap_ms, undefined);
  assert.equal(profile.courses.sx01.best_total_ms, undefined);
  assert.equal(JSON.parse(await page.evaluate(() => localStorage.getItem('fury-profiles'))).profiles[0].races, 5);
  // Credited once: later frames leave the record alone.
  await page.waitForTimeout(1500);
  const again = JSON.parse(await page.evaluate(() => localStorage.getItem('fury-profile')));
  assert.deepEqual(again, profile);
  await optionalScreenshot(page, 'ATV_RACE_PROFILE_POINTS_SCREENSHOT', 'sx01');
  assertNoErrors(errors);
  console.log(`PASS: Rust engine race Profile Points: place ${you.position}, stunt ${you.stunt}, +${award} -> ${profile.points} (${riders})`);
} finally {
  await browser.close();
}
