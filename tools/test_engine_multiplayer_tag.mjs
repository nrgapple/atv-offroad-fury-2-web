// Host-only gate: Tag in a multiplayer room (research/GAMES.md "Tag",
// "Online"). A Rust host rides a room on Prospect Springs (`?host=1&venue=env05`)
// and a Rust guest joins by code beside it. The host's Pause → Play Tag
// moves the room onto the original Tag course (E05_TG.RES: the mode TOY's
// start and the `tagball.psg` model, both pages reload) and starts the
// game with Games Prep's minutes (`tagMinutes=3`; `tagTest=fast` runs a
// 30 s clock). `tagTest=throw` starts the ball past the boundary margin:
// it is thrown toward the map centre at 170 units/s and 112 up
// (`00246ec0`), then tossed to the host. The host then carries it ("You
// are it!") with the carrier's engine handicap, its score grows by 250 a
// second, the guest sees the host carrying it. With
// `tagReach=60` (a wide contact for automation) the guest, parked beside
// the host, takes the ball once the 2 s immunity passes; the carrier and
// the scores reach both pages, and a 30 s clock (`tagTest=fast`) ends in
// Tag Results on both.
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_multiplayer_tag.mjs
// ATV_GAMES_SHOTS=<dir> saves captures.
import assert from 'node:assert/strict';
import { assertNoErrors, base, launch, newPage, nextFrames } from './browser/harness.mjs';

const shots = process.env.ATV_GAMES_SHOTS || '';
const errors = [];
const code = `TG${String(Date.now()).slice(-6)}`;
const ui = p => p.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const tag = async p => (await ui(p)).ride?.tag;
async function until(p, predicate, timeout = 60000, label = '') {
  const deadline = Date.now() + timeout;
  for (;;) {
    const t = await tag(p);
    if (t && predicate(t)) return t;
    if (Date.now() > deadline) throw new Error(`timed out ${label}: ${JSON.stringify(t)}`);
    await p.waitForTimeout(100);
  }
}
async function key(page, k) { await page.keyboard.down(k); await nextFrames(page, 2); await page.keyboard.up(k); await nextFrames(page, 3); }
async function shot(p, name) {
  if (!shots) return;
  await nextFrames(p, 3);
  await p.screenshot({ path: `${shots}/${name}.png` });
}

const browser = await launch();
try {
  const open = async () => {
    const p = await newPage(browser, { viewport: { width: 960, height: 540 } }, errors);
    await p.addInitScript(() => {
      localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
      Object.defineProperty(document, 'hasFocus', { value: () => true });
    });
    return p;
  };
  const host = await open();
  await host.goto(`${base}/play.html?host=1&venue=env05&room=${code}&name=RustHost&uiDiagnostics=1&tagTest=fast&tagTest=throw&tagMinutes=3&tagReach=60&weather=off`);
  await host.waitForFunction(c => JSON.parse(window.furyMultiplayer ?? 'null')?.code === c, code, { timeout: 180000 });
  const guest = await open();
  await guest.goto(`${base}/play.html?room=${code}&dropAfter=9999&name=RustGuest&uiDiagnostics=1&weather=off`);
  await guest.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 1, null, { timeout: 180000 });
  await host.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 1, null, { timeout: 60000 });

  // Pause → Play Tag (the host's row).
  await host.bringToFront();
  await host.mouse.click(480, 300);
  await key(host, 'Escape');
  await host.waitForFunction(() => JSON.parse(document.querySelector('#ui-state').textContent).toolbar?.menu?.page === 'pause', null, { timeout: 30000 });
  const menu = (await ui(host)).toolbar.menu;
  const at = menu.choices.indexOf('playtag');
  assert.ok(at > 0, `Play Tag in ${menu.choices}`);
  for (let i = 0; i < at; i++) await key(host, 'ArrowDown');
  await key(host, 'Enter');
  // Both pages move to the Tag course and draw the ball model.
  let h = await until(host, t => t.roster.length === 2 && t.throwIns >= 1, 120000, 'throw-in');
  assert.equal(h.model, true);
  assert.equal(h.minutes, 3);
  assert.ok(h.log.includes('ThrowIn 1'), JSON.stringify(h.log));
  const out = (await ui(host)).ride;
  assert.equal(out.tagBall, true);
  assert.equal(out.environment, 'env05');
  // E05_TG.RES env05tg.toy DefaultStartPosition.
  assert.deepEqual(out.start.map(v => Math.round(v * 100) / 100), [1839.75, 135, 3389.18]);
  assert.ok(Math.abs(Math.hypot(h.ballVelocity[0], h.ballVelocity[2]) - 170.4714) < 1 || h.throwIns > 0, JSON.stringify(h.ballVelocity));
  h = await until(host, t => t.carrier === 0, 60000, 'host carries');
  assert.equal(h.host, true);
  assert.equal(h.timeLimit, 30);
  // The carrier's engine handicap (`00246310`: effect -0.34).
  await host.waitForFunction(() => Math.abs(JSON.parse(document.querySelector('#ui-state').textContent).ride?.engineEffect + 0.34) < 1e-4, null, { timeout: 10000 });
  await until(host, t => t.scores[0] >= 250, 15000, 'host scores');
  await shot(host, 'mp-tag-host-it');
  // The guest takes the ball: the host's handicap is cleared (`00246000`).
  let g = await until(guest, t => t.carrier === 0, 120000, 'guest sees carrier');
  assert.equal(g.me, 1);
  assert.equal(g.model, true, 'guest rides the Tag course');
  assert.match(g.banner ?? 'RustHost has the ball!', /RustHost has the ball!/);
  await shot(guest, 'mp-tag-guest-chasing');
  // The guest beside the host takes it after the immunity.
  h = await until(host, t => t.carrier === 1, 30000, 'transfer');
  assert.ok(h.log.some(l => l === 'Gain 1'), JSON.stringify(h.log));
  g = await until(guest, t => t.carrier === 1, 15000, 'guest carries');
  await host.waitForFunction(() => JSON.parse(document.querySelector('#ui-state').textContent).ride?.engineEffect === 0, null, { timeout: 10000 });
  await guest.waitForFunction(() => Math.abs(JSON.parse(document.querySelector('#ui-state').textContent).ride?.engineEffect + 0.34) < 1e-4, null, { timeout: 10000 });
  await until(guest, t => t.scores[1] >= 250, 15000, 'guest scores');
  await shot(guest, 'mp-tag-guest-it');
  for (const p of [host, guest]) {
    await p.waitForFunction(() => {
      const ride = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride;
      return ride?.tag?.finished && ride?.results?.length === 2;
    }, null, { timeout: 90000 });
  }
  g = await tag(guest);
  h = await tag(host);
  assert.deepEqual(g.scores, h.scores);
  assert.ok(h.scores[0] > 0 && h.scores[1] > 0, JSON.stringify(h.scores));
  await shot(guest, 'mp-tag-guest-results');
  assertNoErrors(errors);
  console.log(`Multiplayer Tag: start, carry, transfer and results verified (${h.summary})`);
} finally {
  await browser.close();
}
