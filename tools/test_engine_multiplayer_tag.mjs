// Host-only gate: Tag in a multiplayer room (research/GAMES.md "Tag",
// "Online"). A Rust host rides a room on Prospect Springs (`?host=1&venue=env05`)
// and a Rust guest joins by code beside it. The host's Pause → Play Tag
// starts the game: the ball is tossed to the host ("You are it!"), its
// score grows by 250 a second, the guest sees the host carrying it. With
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
  await host.goto(`${base}/play.html?host=1&venue=env05&room=${code}&name=RustHost&uiDiagnostics=1&tagTest=fast&tagReach=60&weather=off`);
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
  let h = await until(host, t => t.roster.length === 2 && t.carrier === 0, 30000, 'host carries');
  assert.equal(h.host, true);
  await until(host, t => t.scores[0] >= 250, 15000, 'host scores');
  await shot(host, 'mp-tag-host-it');
  let g = await until(guest, t => t.carrier === 0, 30000, 'guest sees carrier');
  assert.equal(g.me, 1);
  assert.match(g.banner ?? 'RustHost has the ball!', /RustHost has the ball!/);
  await shot(guest, 'mp-tag-guest-chasing');
  // The guest beside the host takes it after the immunity.
  h = await until(host, t => t.carrier === 1, 30000, 'transfer');
  assert.ok(h.log.some(l => l === 'Gain 1'), JSON.stringify(h.log));
  g = await until(guest, t => t.carrier === 1, 15000, 'guest carries');
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
