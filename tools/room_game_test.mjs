// Shared driver of the room King of the Hills / Treasure Hunt browser tests
// (research/MULTIPLAYER.md "Room games"); see the test files for what each
// verifies. Not a test itself.
import assert from 'node:assert/strict';
import { assertNoErrors, base, launch, newPage, nextFrames } from './browser/harness.mjs';

export async function roomGame({ prefix, action, hook, verify, label }) {
  const shots = process.env.ATV_GAMES_SHOTS || '';
  const errors = [];
  const code = `${prefix}${String(Date.now()).slice(-6)}`;
  const ui = p => p.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
  const ride = async p => (await ui(p)).ride ?? {};
  async function until(p, predicate, timeout = 60000, what = '') {
    const deadline = Date.now() + timeout;
    for (;;) {
      const r = await ride(p).catch(() => ({}));
      if (r.game && r.roomGame && predicate(r.game, r.roomGame, r)) return r;
      if (Date.now() > deadline) throw new Error(`timed out ${what}: ${JSON.stringify({ game: r.game && { ...r.game, starts: undefined, rules: undefined }, room: r.roomGame })} errors ${errors.slice(0, 5).join(' | ')}`);
      await p.waitForTimeout(200);
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
    const flags = `uiDiagnostics=1&weather=off&gameTest=${hook}`;
    const host = await open();
    await host.goto(`${base}/play.html?host=1&venue=env05&room=${code}&name=RustHost&gameTest=fast&gameMinutes=3&${flags}`);
    await host.waitForFunction(c => JSON.parse(window.furyMultiplayer ?? 'null')?.code === c, code, { timeout: 180000 });
    const guest = await open();
    await guest.goto(`${base}/play.html?room=${code}&dropAfter=9999&name=RustGuest&${flags}`);
    await guest.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 1, null, { timeout: 180000 });
    await host.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 1, null, { timeout: 60000 });

    // Pause → Play <game> (the host's row).
    await host.bringToFront();
    await host.mouse.click(480, 300);
    await key(host, 'Escape');
    await host.waitForFunction(() => JSON.parse(document.querySelector('#ui-state').textContent).toolbar?.menu?.page === 'pause', null, { timeout: 30000 });
    const menu = (await ui(host)).toolbar.menu;
    const at = menu.choices.indexOf(action);
    assert.ok(at > 0, `${action} in ${menu.choices}`);
    for (let i = 0; i < at; i++) await key(host, 'ArrowDown');
    await key(host, 'Enter');

    // Both reload onto the game's course scene and load the mode TOY.
    let h = await until(host, (g, r) => g.loaded && r.present[0] && r.present[1], 240000, 'host loads the game');
    assert.equal(h.roomGame.host, true);
    assert.equal(h.game.minutes, 3);
    let g = await until(guest, (g, r) => g.loaded && r.serial === h.roomGame.serial, 240000, 'guest joins the game');
    assert.equal(g.roomGame.host, false);
    assert.equal(g.roomGame.me, 1);
    // One shared countdown, then play.
    await until(guest, game => game.started, 60000, 'guest starts');
    await verify({ host, guest, until, shot, ride });
    // Time Up (gameTest=fast: a 20 s clock): the same results on both.
    for (const p of [host, guest]) {
      await until(p, game => game.over && game.reported, 120000, 'results');
      await p.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride?.results?.length >= 1, null, { timeout: 60000 });
    }
    h = await ride(host);
    g = await ride(guest);
    assert.deepEqual(g.roomGame.standings.map(s => [s.rider, s.score]), h.roomGame.standings.map(s => [s.rider, s.score]), 'agreed standings');
    assert.equal(g.game.summary, h.game.summary, 'agreed results');
    assert.match(h.game.summary, /RustHost/);
    assert.match(h.game.summary, /RustGuest/);
    await shot(host, `${prefix}-host-results`);
    const first = h.game.summary;

    // Results → Continue → Play Again: the host's next game; the guest
    // follows in place. Then the guest leaves mid-game and is dropped.
    await host.bringToFront();
    await key(host, 'Enter');
    await key(host, 'Enter');
    const serial = h.roomGame.serial;
    h = await until(host, (game, r) => r.serial === serial + 1 && game.started, 120000, 'game 2 starts');
    await until(guest, (game, r) => r.serial === serial + 1 && game.started, 60000, 'guest plays game 2');
    await guest.close();
    h = await until(host, (game, r) => !r.present[1], 60000, 'the guest is dropped');
    h = await until(host, game => game.over && game.reported, 120000, 'game 2 results');
    assert.match(h.game.summary, /RustHost/);
    assert.doesNotMatch(h.game.summary, /RustGuest/, 'the leaver is dropped');
    assert.equal(h.roomGame.standings.length, 1);
    assertNoErrors(errors);
    console.log(`${label}: shared start, host-authoritative play and results (${first}), Play Again and a mid-game leaver (${h.game.summary}) verified`);
  } finally {
    await browser.close();
  }
}
