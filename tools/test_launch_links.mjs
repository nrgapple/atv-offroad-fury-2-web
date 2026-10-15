// Opt-in actual hosted content + Rust relay. No original resources enter CI.
import assert from 'node:assert/strict';
import { assertNoErrors, base, button, gameState as state, launch, newPage } from './browser/harness.mjs';
const browser = await launch();
const errors = [],
  pages = [];
async function page(viewport = { width: 1280, height: 896 }) {
  const p = await newPage(browser, { viewport, hasTouch: true }, errors);
  pages.push(p);
  await p.addInitScript(() => {
    Object.defineProperty(document, 'hasFocus', { value: () => true });
    window.testConnections = 0;
    const Native = WebSocket;
    window.WebSocket = class extends Native {
      constructor(...args) {
        super(...args);
        window.testConnections++;
      }
    };
  });
  return p;
}
async function open(p, query, root = false) {
  console.log('OPEN', query);
  await p.goto(base + (root ? '/?' : '/play.html?') + query);
}
async function playing(p) {
  await p.waitForFunction(
    () => window.atvTerrainStatus?.simulation && document.querySelector('#frontend').hidden,
    null,
    { timeout: 120000 }
  );
  assert.equal(await p.locator('#disc').isHidden(), true);
}
async function copy(p, name = 'Copy ride link') {
  await p.context().grantPermissions(['clipboard-read', 'clipboard-write'], { origin: base });
  await button(p, name).click();
  return p.evaluate(() => navigator.clipboard.readText());
}
try {
  const p = await page();
  // Explicit setup wins over conflicting persistent selections.
  await p.addInitScript(() =>
    localStorage.setItem(
      'atv-player-selection',
      JSON.stringify({ quadId: 46, riderId: 0, freestyleEnvironment: 'env07' })
    )
  );
  await open(p, 'mode=single&map=env02&quad=14&rider=female&player=Sam%20%26%20Jo', true);
  await playing(p);
  let s = await state(p);
  assert.match(s.source, /env02/i);
  assert.equal(s.playerModel.quad_id, 14);
  assert.equal(s.playerModel.rider_id, 1);
  await button(p, 'Pause').click();
  const ride = await copy(p);
  let url = new URL(ride);
  assert.equal(url.searchParams.get('map'), 'env02');
  assert.equal(url.searchParams.get('quad'), '14');
  assert.equal(url.searchParams.get('rider'), '1');
  assert.equal(url.searchParams.get('player'), 'Sam & Jo');
  console.log('SINGLE_AND_SHARE', ride);
  if (process.env.ATV_LINK_SCREENSHOT) await p.screenshot({ path: process.env.ATV_LINK_SCREENSHOT });
  await p.goto(ride);
  await playing(p);
  assert.equal((await state(p)).playerModel.quad_id, 14);
  // Deterministic omitted defaults do not inherit the stored motorcycle.
  await open(p, 'map=free05');
  await playing(p);
  s = await state(p);
  assert.match(s.source, /free05/i);
  assert.equal(s.playerModel.quad_id, 1);
  await p.setViewportSize({ width: 390, height: 844 });
  await open(p, 'map=env01&vehicle=34&rider=male');
  await playing(p);
  assert.equal((await state(p)).playerModel.quad_id, 34);
  await open(p, 'mode=single&activity=race&map=env01&quad=2&rider=1&laps=2&cpu=0');
  await playing(p);
  await p.waitForFunction(() => window.atvTerrainStatus.frame.race?.laps === 2, null, { timeout: 15000 });
  s = await state(p);
  assert.equal(s.frame.race.field, 1);
  assert.equal(s.playerModel.quad_id, 2);
  await button(p, 'Pause').click();
  url = new URL(await copy(p));
  assert.equal(url.searchParams.get('activity'), 'race');
  assert.equal(url.searchParams.get('laps'), '2');
  assert.equal(url.searchParams.get('cpu'), '0');
  console.log('MOTORCYCLE_AND_RACE', url.href);
  await open(p, 'map=env02&quad=26&rider=1&play=0');
  await button(p, 'Ride Tioga Valley').waitFor({ timeout: 90000 });
  assert.equal((await state(p)).simulation, false);
  await button(p, 'Ride Tioga Valley').click();
  await playing(p);
  assert.equal((await state(p)).playerModel.quad_id, 26);
  for (const q of [
    'map=bad',
    'room=ABCD1234&quad=27',
    'mode=multiplayer&room=ABCD1234&activity=race',
    'quad=1&quad=2'
  ]) {
    await open(p, q);
    await button(p, 'Single Player').waitFor({ timeout: 90000 });
    assert.equal((await state(p)).simulation, false);
    assert.equal(await p.evaluate(() => window.testConnections), 0);
    assert.match(await p.locator('#menu-notice').textContent(), /Cannot open ride link/);
  }
  // An invite to a room that has ended reopens that code on the link's map.
  const ended = 'ZZ' + Date.now().toString(36).slice(-6).toUpperCase();
  await open(p, `room=${ended}&map=env03`);
  await playing(p);
  s = await state(p);
  assert.equal(s.multiplayer.state.code, ended);
  assert.equal(s.multiplayer.state.environment, 'env03');
  assert.equal(s.multiplayer.state.lobby.host, s.multiplayer.state.peer);
  assert.equal(s.multiplayer.state.lobby.players.length, 1);
  assert.equal(s.multiplayerError, undefined);
  await button(p, 'Pause').click();
  await button(p, 'Leave Game').click();
  await p.waitForFunction(
    () =>
      !window.atvTerrainStatus.multiplayer.state.code &&
      document.querySelector('#frontend').dataset.page === 'multiplayer',
    null,
    { timeout: 15000 }
  );
  console.log('INVALID_AND_REOPENED_ROOM', JSON.stringify({ code: ended, map: s.multiplayer.state.environment }));
  const host = await page(),
    guest = await page({ width: 390, height: 844 });
  await open(host, 'mode=multiplayer&action=create&map=free05&quad=14&rider=0&player=LinkHost');
  await playing(host);
  const hostState = await state(host),
    code = hostState.multiplayer.state.code;
  assert.equal(hostState.multiplayer.state.lobby.map, 9);
  assert.equal(hostState.multiplayer.state.peer, 0);
  await button(host, 'Pause').click();
  const invite = await copy(host, 'Copy invite link');
  assert.equal(new URL(invite).searchParams.get('map'), 'free05');
  url = new URL(invite);
  assert.equal(url.searchParams.get('room'), code);
  assert.equal(url.searchParams.get('action'), 'join');
  assert.equal(url.searchParams.has('player'), false);
  assert.equal(url.searchParams.has('host'), false);
  // The room owns its course. A guest's differing map parameter cannot change it.
  url.searchParams.set('player', 'LinkGuest');
  url.searchParams.set('quad', '26');
  url.searchParams.set('rider', '1');
  url.searchParams.set('map', 'env01');
  await guest.goto(url.href);
  await playing(guest);
  for (const peer of [host, guest]) await peer.evaluate(() => window.dispatchEvent(new Event('focus')));
  for (const peer of [host, guest])
    await peer.waitForFunction(
      () => window.atvTerrainStatus.multiplayerDraws > 0 && window.atvTerrainStatus.multiplayer.remote.length === 1,
      null,
      { timeout: 120000 }
    );
  s = await state(guest);
  assert.equal(s.playerModel.quad_id, 26);
  assert.equal(s.playerModel.rider_id, 1);
  assert.equal(s.multiplayer.state.lobby.map, 9);
  assert.match(s.source, /free05/i);
  console.log(
    'DIRECT_CREATE_AND_LATE_JOIN',
    JSON.stringify({
      code,
      draws: [(await state(host)).multiplayerDraws, s.multiplayerDraws],
      players: s.multiplayer.state.lobby.players
    })
  );
  // The host leaves; the guest keeps riding and becomes host, then leaves too.
  await button(host, 'Leave Game').click();
  await guest.waitForFunction(
    code => {
      const m = window.atvTerrainStatus.multiplayer;
      return m.state.code === code && m.state.lobby?.host === m.state.peer && m.remote.length === 0;
    },
    code,
    { timeout: 15000 }
  );
  assert.ok((await state(guest)).simulation);
  await button(guest, 'Pause').click();
  await button(guest, 'Leave Game').click();
  await guest.waitForFunction(() => !window.atvTerrainStatus.multiplayer.state.code, null, { timeout: 15000 });
  // Prepare/create opens Host Game with the requested track chosen, without connecting.
  await open(host, 'action=create&map=env02&quad=2&play=0');
  await button(host, 'Select Track').waitFor({ timeout: 90000 });
  assert.equal(await host.evaluate(() => window.testConnections), 0);
  await button(host, 'Select Track').click();
  await host.locator('.ms-course[data-track-id=env02][aria-selected=true]').waitFor({ timeout: 15000 });
  assert.equal(await host.evaluate(() => window.testConnections), 0);
  await host.locator('button.ms-ride').click();
  await playing(host);
  s = await state(host);
  assert.equal(s.multiplayer.state.lobby.map, 2);
  assert.equal(s.multiplayer.state.lobby.started, true);
  assert.equal(s.playerModel.quad_id, 2);
  await button(host, 'Pause').click();
  await button(host, 'Leave Game').click();
  await host.waitForFunction(() => !window.atvTerrainStatus.multiplayer.state.code, null, { timeout: 15000 });
  console.log('PREPARED_CREATE');
  // A failed runtime resource returns to a usable Multiplayer menu instead of a spinner.
  const failed = await page();
  await failed.route('**/content/manifest.json', async route => {
    const response = await route.fetch(),
      manifest = await response.json();
    manifest.files = manifest.files.filter(f => f.path !== '/QUAD/BIKESB/ATV14.RES');
    await route.fulfill({ response, json: manifest });
  });
  await open(failed, 'action=create&map=env01&quad=14');
  await failed.waitForFunction(
    () =>
      ['multiplayer', 'create-lobby'].includes(document.querySelector('#frontend').dataset.page) &&
      document.querySelector('#frontend').dataset.error === 'true',
    null,
    { timeout: 90000 }
  );
  assert.match(await failed.locator('#menu-notice').textContent(), /archive is missing/);
  s = await state(failed);
  assert.equal(s.simulation, false);
  await button(
    failed,
    (await failed.locator('#frontend').getAttribute('data-page')) === 'multiplayer' ? 'Host Game' : 'Select Track'
  ).waitFor({ timeout: 15000 });
  const failedRoom = s.multiplayer?.state.code ?? null;
  console.log(
    'FAILED_RESOURCE_RECOVERY',
    JSON.stringify({ page: await failed.locator('#frontend').getAttribute('data-page'), code: failedRoom })
  );
  // Ordinary navigation still reaches title.
  await p.goto(base + '/play.html?utm_source=test');
  await button(p, 'Press START').waitFor({ timeout: 90000 });
  assert.equal((await state(p)).simulation, false);
  // A rider whose ride failed to load must not stay in the room as an invisible member.
  assert.equal(
    failedRoom,
    null,
    `failed load left the client connected to room ${failedRoom} while showing the Multiplayer menu`
  );
  assertNoErrors(errors);
  console.log(
    'PASS direct setup/share/reload/defaults/motorcycle/race/prepare/validation/missing room/host/late join/lobby authority/ordinary title; no disc picker or page errors'
  );
} catch (error) {
  for (const p of pages)
    try {
      console.error(
        'FAIL_STATE',
        JSON.stringify(
          await p.evaluate(() => ({
            url: location.href,
            page: document.querySelector('#frontend').dataset,
            notice: document.querySelector('#menu-notice').textContent,
            state: window.atvTerrainStatus
          }))
        )
      );
    } catch {}
  throw error;
} finally {
  await browser.close();
}
