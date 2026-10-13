// Live Rust relay gate. WS_MODULE points to an installed ws package; no game data.
// Build fixture: cargo build -p atv-relay --example codec.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import http from 'node:http';
import https from 'node:https';
import { homedir } from 'node:os';
import path from 'node:path';
const { default: WebSocket } = await import(pathToFileURL(process.env.WS_MODULE).href);
const metadata = process.env.ATV_CODEC ? null : spawnSync(path.join(homedir(), '.cargo/bin/cargo'),
  ['metadata', '--no-deps', '--format-version', '1'], { encoding: 'utf8', cwd: new URL('..', import.meta.url) });
const codec = process.env.ATV_CODEC ?? path.join(JSON.parse(metadata.stdout).target_directory, 'debug/examples/codec');
const base = process.env.ATV_RELAY_URL ?? 'http://127.0.0.1:8788';
const origin = process.env.ATV_RELAY_ORIGIN ?? base;
const verifierHeaders = { 'User-Agent': 'ATV-multiplayer-relay-verifier/1.0' };
const report = { checks: [], relay: base, reachability: ['https://furyremastered.com', 'https://atv-offroad-fury-2.champtoday.workers.dev'].includes(base) ? 'public HTTPS' : 'host-only' };
const sockets = [];
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
function convert(mode, input) {
  const result = spawnSync(codec, [mode], { input, encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
  return result.stdout.trim();
}
const encode = packet => Buffer.from(convert('encode', JSON.stringify(packet)), 'hex');
const decode = bytes => JSON.parse(convert('decode', Buffer.from(bytes).toString('hex')));
const check = name => report.checks.push(name);
async function request(path, { method = 'GET', requestOrigin = origin, body }  = {}) {
  return new Promise((resolve, reject) => {
    const url = new URL(base + path);
    const req = (url.protocol === 'https:' ? https : http).request(url, {
      method, headers: { ...verifierHeaders, ...(requestOrigin ? { Origin: requestOrigin } : {}), ...(body ? {'Content-Type':'application/json'} : {}) },
    }, response => {
      const parts = [];
      response.on('data', part => parts.push(part));
      response.on('end', () => resolve({ status: response.statusCode,
        json: async () => JSON.parse(Buffer.concat(parts).toString()) }));
    });
    req.on('error', reject);
    req.end(body);
  });
}
async function rejectedSocket(code, expected, token = '', requestOrigin = origin) {
  const status = await new Promise((resolve, reject) => {
    const ws = new WebSocket(base.replace('http', 'ws') + '/multiplayer/rooms/' + code + token, { origin: requestOrigin, headers: verifierHeaders });
    ws.on('unexpected-response', (_req, response) => { response.resume(); ws.terminate(); resolve(response.statusCode); });
    ws.on('error', () => {});
    ws.on('open', () => { ws.close(1000,'Left lobby'); reject(new Error('Unexpected accepted socket')); });
  });
  assert.equal(status, expected);
}
async function connect(code, player, token = '') {
  const ws = new WebSocket(base.replace('http', 'ws') + '/multiplayer/rooms/' + code + token, { origin, headers: verifierHeaders });
  ws.packets = [];
  sockets.push(ws);
  ws.on('message', bytes => ws.packets.push(decode(bytes)));
  ws.on('error', error => { ws.lastError = error.message; });
  ws.on('close', (code, reason) => { ws.closed = { code, reason: reason.toString() }; });
  await new Promise((resolve, reject) => { ws.once('open', resolve); ws.once('error', reject); });
  ws.send(encode({ Join: { code: [...Buffer.from(code)], player } }));
  const welcome = await wait(ws, packet => packet.Welcome);
  ws.peer = welcome.Welcome.peer;
  return ws;
}
async function wait(ws, predicate, after = 0) {
  const deadline = Date.now() + 5000;
  while (Date.now() < deadline) {
    const found = ws.packets.slice(after).find(predicate);
    if (found) return found;
    if (ws.closed) throw new Error('Socket closed: ' + JSON.stringify(ws.closed));
    await sleep(10);
  }
  throw new Error('Packet timeout: ' + JSON.stringify(ws.packets));
}
async function closed(ws) {
  const deadline = Date.now() + 5000;
  while (!ws.closed && Date.now() < deadline) await sleep(10);
  assert.ok(ws.closed, 'Expected socket closure');
  return ws.closed;
}
function lastLobby(ws) {
  const packet = ws.packets.findLast(packet => packet.Lobby || packet.Welcome);
  return packet.Lobby ?? packet.Welcome.lobby;
}
const player = (name, atv_model = 1, rider_model = 0) => ({ id: 0, name, atv_model, rider_model });
const transform = { position: [100, 20, 40], rotation: [1, 0, 0, 0], scale: [1, 1, 1] };
const snapshot = {
  peer: 0, sequence: 7, time_ms: 1500, atv_model: 26, rider_model: 1,
  atv: transform, velocity: [0, 2, 30], angular_velocity: [0, 1, 0],
  atv_nodes: [transform], rider_nodes: [transform], rider_state: 'Wreck',
  rider: { ...transform, position: [110, 30, 40] }, rider_velocity: [1, 3, 12],
  rpm: 4500, throttle: 0.7, speed: 30, crash_serial: 5,
};
try {
  assert.equal((await request('/multiplayer/rooms', { method: 'POST', requestOrigin: '' })).status, 403);
  assert.equal((await request('/multiplayer/rooms', { method: 'POST', requestOrigin: 'https://foreign.invalid' })).status, 403);
  const version = await request('/multiplayer/version');
  assert.equal(version.status, 200);
  const identity = await version.json();
  assert.equal(identity.protocol, 3);
  assert.ok(identity.commit && identity.wasmSha256);
  report.identity = identity;
  await rejectedSocket('ABCD2345', 404);
  check('Missing/foreign origins rejected; unknown code cannot create room');
  const custom='T'+Date.now().toString(36).toUpperCase().slice(-7);
  assert.equal((await request('/multiplayer/rooms',{method:'POST',body:JSON.stringify({code:'too long!'})})).status,400);
  const named=await request('/multiplayer/rooms',{method:'POST',body:JSON.stringify({code:custom.toLowerCase()})});
  assert.equal(named.status,201);assert.equal((await named.json()).code,custom);
  assert.equal((await request('/multiplayer/rooms',{method:'POST',body:JSON.stringify({code:custom})})).status,409);
  check('Custom code preserved/normalized, invalid codes rejected and duplicates cannot replace rooms');
  const creation = await request('/multiplayer/rooms', { method: 'POST' });
  assert.equal(creation.status, 201);
  const { code, hostToken } = await creation.json();
  assert.match(code, /^[ABCDEFGHJKLMNPQRSTUVWXYZ23456789]{8}$/);
  assert.match(hostToken, /^[a-f0-9]{32}$/);
  await rejectedSocket(code, 409);
  await rejectedSocket(code, 403, '?host=incorrect');
  let host = await connect(code, { ...player('Host'), id: 3 }, '?host=' + hostToken);
  assert.equal(host.peer, 0);
  assert.equal(lastLobby(host).players[0].id, 0);
  const replacedHost=host;host=await connect(code,player('Host'),'?host='+hostToken);
  assert.equal((await closed(replacedHost)).reason,'Host connection replaced');
  check('Secure host reservation and server-assigned identity');
  const guest = await connect(code, player('Guest female', 26, 1));
  const guest2 = await connect(code, player('Guest 2'));
  const guest3 = await connect(code, player('Guest 3'));
  assert.deepEqual([guest.peer, guest2.peer, guest3.peer], [1, 2, 3]);
  await wait(host, p => p.Lobby?.players.length === 4);
  await rejectedSocket(code, 409);
  check('Four-player capacity enforced');
  const oldEpoch = lastLobby(host).epoch - 1;
  let after = host.packets.length;
  host.send(encode({ SetLobby: { epoch: oldEpoch, map: 9, started: true } }));
  await wait(host, p => p.Lobby?.epoch === oldEpoch + 1, after);
  assert.equal(lastLobby(host).started, false);
  check('Stale host command resynchronizes without disconnecting host');
  after = host.packets.length;
  host.send(encode({ SetLobby: { epoch: lastLobby(host).epoch, map: 9, started: false } }));
  await wait(host, p => p.Lobby?.map === 9 && !p.Lobby.started, after);
  for (const ws of [guest, guest2, guest3]) await wait(ws, p => p.Lobby?.map === 9 && !p.Lobby.started);
  check('Ninth environment selection reaches all four authenticated participants');
  after = host.packets.length;
  host.send(encode({ SetLobby: { epoch: lastLobby(host).epoch, map: 9, started: true } }));
  await wait(host, p => p.Lobby?.started && p.Lobby.map === 9, after);
  const activeEpoch = lastLobby(host).epoch;
  for (const attempted of [{ map: 1, started: true }, { map: 9, started: false }]) {
    after = host.packets.length;
    host.send(encode({ SetLobby: { epoch: activeEpoch, ...attempted } }));
    const retained = (await wait(host, p => p.Lobby, after)).Lobby;
    assert.equal(retained.map, 9);
    assert.equal(retained.started, true);
    assert.equal(retained.epoch, activeEpoch);
  }
  check('Active session map/start cannot change or restart');
  const receivedAt = host.packets.length;
  guest.send(encode({ Snapshot: snapshot }));
  const received = (await wait(host, p => p.Snapshot, receivedAt)).Snapshot;
  assert.equal(received.peer, 1);
  assert.deepEqual(received.rider.position, [110, 30, 40]);
  assert.equal(received.rider_state, 'Wreck');
  assert.equal(received.crash_serial, 5);
  assert.equal(received.rpm, 4500);
  assert.equal(received.atv_model, 26);
  const count = host.packets.filter(p => p.Snapshot).length;
  guest.send(encode({ Snapshot: snapshot }));
  await sleep(150);
  assert.equal(host.packets.filter(p => p.Snapshot).length, count);
  check('Validated pose/wreck/audio relayed with sender stamp; stale sequence dropped');
  after = host.packets.length;
  guest2.send(encode({ SetLobby: { epoch: lastLobby(host).epoch, map: 1, started: false } }));
  assert.equal((await closed(guest2)).code, 1008);
  await wait(host, p => p.Leave?.peer === 2, after);
  await wait(host, p => p.Lobby?.players.length === 3, after);
  const replacement = await connect(code, player('Replacement'));
  assert.equal(replacement.peer, 2);
  check('Guest cannot control lobby; departure precedes safe slot reuse');
  guest3.send(Buffer.from('malformed'));
  assert.equal((await closed(guest3)).code, 1008);
  check('Malformed binary packet closes only offender');
  const rate = await connect(code, player('Rate probe'));
  const ping = encode({ Ping: { nonce: 42 } });
  for (let i = 0; i < 500; i++) rate.send(ping);
  assert.equal((await closed(rate)).code, 1008);
  check('Live per-peer message rate limit enforced');
  guest.send(encode({ Snapshot: { ...snapshot, sequence: 8, atv_model: 1 } }));
  assert.equal((await closed(guest)).code, 1008);
  check('Model substitution rejected');
  host.close(1000,'Left lobby');
  await closed(host);
  assert.match((await closed(replacement)).reason, /Host left/);
  await rejectedSocket(code, 410);
  check('Host departure terminates session; no join resurrection');
  const reused=await request('/multiplayer/rooms',{method:'POST',body:JSON.stringify({code})});
  assert.equal(reused.status,201);const renewed=await reused.json();assert.equal(renewed.code,code);assert.notEqual(renewed.hostToken,hostToken);
  await rejectedSocket(code,403,'?host='+hostToken);
  const recoveredHost=await connect(code,player('Reused host'),'?host='+renewed.hostToken);
  const recoveryGuest=await connect(code,player('Recovery guest'));
  await wait(recoveredHost,p=>p.Lobby?.players.length===2);
  recoveredHost.send(encode({SetLobby:{epoch:lastLobby(recoveredHost).epoch,map:8,started:true}}));
  await wait(recoveryGuest,p=>p.Lobby?.started);
  recoveredHost.terminate();await sleep(500);
  assert.equal(recoveryGuest.closed,undefined);
  const restoredHost=await connect(code,player('Reused host'),'?host='+renewed.hostToken);
  assert.equal(restoredHost.peer,0);assert.equal(lastLobby(restoredHost).started,true);assert.equal(lastLobby(restoredHost).map,8);
  await wait(recoveryGuest,p=>p.Lobby?.started&&p.Lobby.players.length===2);
  await sleep(100);assert.equal(restoredHost.closed,undefined,'Old socket close must not terminate replacement');
  restoredHost.close(1000,'Left lobby');await closed(recoveryGuest);
  check('Closed codes are reusable with fresh authority; abnormal host loss recovers the running room');
  const abandonedCreation=await request('/multiplayer/rooms',{method:'POST'});
  const abandoned=await abandonedCreation.json();
  const abandonedHost=await connect(abandoned.code,player('Grace expiry host'),'?host='+abandoned.hostToken);
  const abandonedGuest=await connect(abandoned.code,player('Grace expiry guest'));
  abandonedHost.terminate();await sleep(500);assert.equal(abandonedGuest.closed,undefined);
  // Keep the guest active so only the host's recovery deadline can close this room.
  const keepalive=setInterval(()=>{if(abandonedGuest.readyState===WebSocket.OPEN)abandonedGuest.send(encode({Ping:{nonce:1}}));},2000);
  try {await sleep(35000);assert.match((await closed(abandonedGuest)).reason,/Host connection could not be restored/);}
  finally {clearInterval(keepalive);}
  const afterExpiry=await request('/multiplayer/rooms',{method:'POST',body:JSON.stringify({code:abandoned.code})});
  assert.equal(afterExpiry.status,201);
  const expiryHost=await connect(abandoned.code,player('Fresh owner'),'?host='+(await afterExpiry.json()).hostToken);
  expiryHost.close(1000,'Left lobby');await closed(expiryHost);
  check('Unrecovered host grace expires and releases the code while guests are active');


  const invalidCreation = await request('/multiplayer/rooms', { method: 'POST' });
  assert.equal(invalidCreation.status, 201);
  const invalidRoom = await invalidCreation.json();
  const invalidHost = await connect(invalidRoom.code, player('Invalid map probe'), '?host=' + invalidRoom.hostToken);
  // Bypass the shared encoder deliberately: the untrusted socket must reject
  // map41 even when the rest of the version3 control packet is valid.
  const invalidMap = encode({ SetLobby: { epoch: lastLobby(invalidHost).epoch, map: 9, started: false } });
  invalidMap.writeUInt16LE(41, 12);
  invalidHost.send(invalidMap);
  assert.equal((await closed(invalidHost)).code, 1008);
  await rejectedSocket(invalidRoom.code, 410);
  check('Map41 rejected on the real socket; host termination cannot resurrect the invalid room');
  console.log(JSON.stringify(report, null, 2));
} finally {
  for (const ws of sockets) if (ws.readyState < WebSocket.CLOSING) ws.close(1000,'Left lobby');
}
