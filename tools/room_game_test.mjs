// Shared driver of the room King of the Hills / Treasure Hunt browser tests
// (research/MULTIPLAYER.md "Room games"); see the test files for what each
// verifies. Not a test itself.
import assert from 'node:assert/strict';
import { assertNoErrors, base, launch, newPage, nextFrames } from './browser/harness.mjs';

// `fromGames`: the host starts from Single Player → Games (Riders: Online
// Room, which hosts the room) instead of a hosted room's Pause row.
// `late`: a third rider joins after GO and spectates game 1.
export async function roomGame({ prefix, action, hook, verify, label, leaver = 'guest', fromGames = '', late = false }) {
  const shots = process.env.ATV_GAMES_SHOTS || '';
  const errors = [];
  let code = `${prefix}${String(Date.now()).slice(-6)}`;
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
    const front = async () => (await ui(host)).front ?? {};
    async function pick(choice) {
      const f = await front();
      const at = f.choices.indexOf(choice);
      assert.ok(at >= 0, `${choice} in ${f.choices}`);
      for (let i = f.selection; i < at; i++) await key(host, 'ArrowDown');
      for (let i = f.selection; i > at; i--) await key(host, 'ArrowUp');
    }
    const onPage = name => host.waitForFunction(n => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === n, name, { timeout: 60000 });
    if (fromGames) {
      // Games → <game> → Riders: Online Room → Start Game hosts the room
      // and starts the game for it (the store bought both games).
      await host.addInitScript(() => {
        localStorage.setItem('fury-profile', JSON.stringify({ store: { events: ['th', 'koth'] } }));
        localStorage.setItem('atv-player-selection', JSON.stringify({ quadId: 1, riderId: 0, playerName: 'RustHost' }));
      });
      await host.goto(`${base}/play.html?gameTest=fast&${flags}`);
      await onPage('title');
      await host.mouse.click(8, 8);
      await key(host, 'Enter');
      await onPage('main');
      await host.waitForFunction(() => (JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.store?.atvs ?? 0) > 20, null, { timeout: 60000 });
      await pick('single');
      await key(host, 'Enter');
      await onPage('single');
      await pick('games');
      await key(host, 'Enter');
      await onPage('games');
      await pick('gametype');
      for (let i = 0; i < 4 && !(await front()).labels[0].startsWith(`Game: ${fromGames}`); i++) await key(host, 'ArrowRight');
      await pick('gameriders');
      await key(host, 'ArrowRight');
      const f = await front();
      assert.equal(f.labels[f.choices.indexOf('gameriders')], 'Riders: Online Room');
      await pick('gamestart');
      await key(host, 'Enter');
      await host.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.code, null, { timeout: 180000 });
      code = await host.evaluate(() => JSON.parse(window.furyMultiplayer).code);
      // The game holds its start for an invitee (Waiting for riders).
      await until(host, game => game.loaded && !game.started && /Waiting for riders/.test(game.notice ?? ''), 240000, 'the host waits for riders');
    } else {
      await host.goto(`${base}/play.html?host=1&venue=env05&room=${code}&name=RustHost&gameTest=fast&gameMinutes=3&${flags}`);
      await host.waitForFunction(c => JSON.parse(window.furyMultiplayer ?? 'null')?.code === c, code, { timeout: 180000 });
    }
    const guest = await open();
    await guest.goto(`${base}/play.html?room=${code}&dropAfter=9999&name=RustGuest&${flags}`);
    await guest.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 1, null, { timeout: 180000 });
    await host.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 1, null, { timeout: 60000 });

    if (!fromGames) {
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
    }

    // Both reload onto the game's course scene and load the mode TOY.
    let h = await until(host, (g, r) => g.loaded && r.present[0] && r.present[1], 240000, 'host loads the game');
    assert.equal(h.roomGame.host, true);
    assert.equal(h.game.minutes, fromGames ? 5 : 3);
    assert.equal(h.game.kind, prefix === 'KH' ? 'koth' : 'treasure');
    let g = await until(guest, (g, r) => g.loaded && r.serial === h.roomGame.serial, 240000, 'guest joins the game');
    assert.equal(g.roomGame.host, false);
    assert.equal(g.roomGame.me, 1);
    // One shared countdown, then play.
    await until(guest, game => game.started, 60000, 'guest starts');
    // The guest's mirrored clock runs (host tables at 10 Hz, local between).
    const t0 = (await ride(guest)).game.timeLeft;
    await until(guest, game => game.timeLeft < t0 - 1, 30000, 'the guest clock runs');
    await verify({ host, guest, until, shot, ride });
    // A rider joining after GO follows onto the course and spectates: no
    // pickups or captures, not in the results; the same results show.
    let lateRider = null;
    if (late) {
      lateRider = await open();
      await lateRider.goto(`${base}/play.html?room=${code}&dropAfter=9999&name=RustLate&uiDiagnostics=1&weather=off`);
      const l = await until(lateRider, (game, r) => game.loaded && r.serial === h.roomGame.serial && r.spectating, 240000, 'the late rider spectates');
      assert.equal(l.roomGame.me, 2);
      assert.equal((await ride(host)).roomGame.players, 0b11, 'the host keeps the players fixed at GO');
      await shot(lateRider, `${prefix}-late-spectator`);
    }
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
    // The course by its Tracks.dat name, not its venue code (env05).
    assert.match(h.game.summary, / on Prospect Springs: /);
    assert.match(h.game.summary, /RustGuest/);
    if (lateRider) {
      assert.doesNotMatch(h.game.summary, /RustLate/, 'a spectator is not in the results');
      const l = await until(lateRider, game => game.over && game.reported, 60000, 'the spectator sees the results');
      assert.equal(l.game.summary, h.game.summary, 'the spectator sees the same results');
      await lateRider.close();
    }
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
    // `leaver: 'host'`: the host leaves instead; the guest takes over the
    // room (host handoff) and finishes the game alone from its mirror.
    const [stay, go, stayName, goName, goSlot] = leaver === 'host'
      ? [guest, host, 'RustGuest', 'RustHost', 0]
      : [host, guest, 'RustHost', 'RustGuest', 1];
    const before = (await ride(stay)).game.timeLeft;
    await go.close();
    await stay.bringToFront();
    if (leaver === 'host') {
      // Until the relay hands the room over, the guest's clock runs on from
      // the host's last table (it froze there before), so the handoff does
      // not stretch the game.
      await stay.waitForTimeout(3000);
      const s = await ride(stay);
      if (!s.roomGame.host && !s.game.over) {
        assert.ok(s.game.timeLeft < before - 1, `guest clock ran on without its host: ${before} -> ${s.game.timeLeft}`);
      }
    }
    h = await until(stay, (game, r) => !r.present[goSlot] && r.host, 90000, 'the leaver is dropped');
    h = await until(stay, game => game.over && game.reported, 120000, 'game 2 results');
    assert.equal(h.roomGame.serial, serial + 1, 'the same game finishes');
    assert.match(h.game.summary, new RegExp(stayName));
    assert.doesNotMatch(h.game.summary, new RegExp(goName), 'the leaver is dropped');
    assert.equal(h.roomGame.standings.length, 1);
    await shot(stay, `${prefix}-after-${leaver}-left`);
    assertNoErrors(errors);
    console.log(`${label}: shared start, host-authoritative play and results (${first}), Play Again and a mid-game ${leaver} leaving (${h.game.summary}) verified`);
  } finally {
    await browser.close();
  }
}
