import { ENVIRONMENTS, buildRideUrl, copyRideUrl } from './launch.js';
// Browser socket and drawing glue. Rust owns the protocol, identity validation,
// pose smoothing, lobby state and audio decisions.
const SESSION = 'atv-mp-session';
const KEY = /^[0-9a-f]{32}$/;
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
// The relay returns a per-slot key in Welcome. This tab keeps it so a dropped
// connection or a reload reclaims the same rider slot; it never enters links.
function savedKey(code) {
  try {
    const saved = JSON.parse(globalThis.sessionStorage?.getItem(SESSION) ?? 'null');
    if (saved?.code === code && KEY.test(saved.key ?? '')) return saved.key;
    const legacy = JSON.parse(globalThis.sessionStorage?.getItem('atv-host-session') ?? 'null');
    if (legacy?.code === code && KEY.test(legacy.hostToken ?? '')) return legacy.hostToken;
  } catch { /* Private browser storage may be unavailable. */ }
  return undefined;
}
function saveKey(code, key) {
  try { globalThis.sessionStorage?.setItem(SESSION, JSON.stringify({code, key})); } catch { /* Memory-only recovery remains available. */ }
}
function forgetKey() {
  try { globalThis.sessionStorage?.removeItem(SESSION); globalThis.sessionStorage?.removeItem('atv-host-session'); } catch { /* Storage may be unavailable. */ }
}
export class Multiplayer {
  constructor({ codec, command, onState, onStart, onLeave, ready, paused, listener, audio, assets, clearAssets, spawn, report }) {
    Object.assign(this, {codec,command,onState,onStart,onLeave,ready,paused,listener,audio,assets,clearAssets,spawn,report});
    this.state = {}; this.socket = null; this.remote = []; this.started = false; this.loading = false; this.generation = 0;
    this.loadedMap = null; this.readySent = false; this.key = undefined;
    this.sendBusy = false; this.sampleBusy = false; this.receiveQueue = Promise.resolve(); this.pendingReceives = 0; this.droppedSnapshots = 0; this.lastDisconnect = null; this.reconnecting=false; this.reconnectUntil=0; this.reconnectAttempts=0; this.retryTimer=null; this.lastTraffic=0; this.lastPing=0; this.networkEncodeMs=0; this.networkEncodeCalls=0;
    this.timer = setInterval(() => { void this.send(); if(this.paused?.()) void this.sample(); }, 50);
    // Pings keep the socket warm and refine the shared world clock.
    this.pingTimer = setInterval(() => { if (this.socket?.readyState === WebSocket.OPEN && this.state.peer != null && (performance.now()-this.lastTraffic>=10000 || performance.now()-this.lastPing>=15000 || ((this.state.rttMs==null || !this.state.clockSynced) && performance.now()-this.lastPing>=2000))) void this.ping(); }, 2000);
  }
  ping() { return this.control({Ping:{nonce:Math.floor(performance.now())>>>0}}).catch(e=>this.report(e.message)); }
  async control(packet) {
    const socket=this.socket, generation=this.generation;
    if(packet.Ping)this.lastPing=performance.now();
    const bytes = await this.codec(85, new TextEncoder().encode(JSON.stringify(packet)));
    if (generation===this.generation && socket===this.socket && socket?.readyState === WebSocket.OPEN && socket.bufferedAmount < 32768) {socket.send(bytes);this.lastTraffic=performance.now();}
  }
  async connect(code, frontend, key, resume=false, recovering=false) {
    await this.disconnect(false,recovering);
    const generation = ++this.generation;
    const state = await this.command(80, {quad_id:frontend.quadId,rider_id:frontend.riderId});
    if (generation !== this.generation) return;
    this.state = state; this.started = resume; this.reconnecting=recovering; this.key = key; this.readySent = false;
    if (!resume) this.loadedMap = null;
    const url = new URL(`/multiplayer/rooms/${encodeURIComponent(code)}`, location.origin); url.protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
    if (key) url.searchParams.set('key',key);
    const socket = this.socket = new WebSocket(url); socket.binaryType = 'arraybuffer';
    socket.onmessage = event => {
      if (generation !== this.generation) return;
      // Bounded work queue; a stalled worker must not retain unbounded packets.
      if (!(event.data instanceof ArrayBuffer) || event.data.byteLength > 8192) { socket.close(1008,'Message limit'); return; }
      // Mobile resume can deliver a burst of old poses. Drop surplus pose
      // frames, leaving 16 queue slots for roster/control messages. Rust still
      // validates every admitted packet; this sniff only decides what to drop.
      const header=new Uint8Array(event.data);
      if(this.pendingReceives>=48 && header.length>=8 && header[0]===65 && header[1]===84 && header[2]===86 && header[3]===77 && ((header[4]===2 && header[5]===4)||(header[4]>=3 && header[5]===9))) { this.droppedSnapshots++; return; }
      if(this.pendingReceives>=64) { socket.close(1008,'Control message backlog'); return; }
      const received = Math.floor(performance.now()); this.pendingReceives++;
      this.receiveQueue = this.receiveQueue.then(async () => {
        if (generation !== this.generation) return;
        const result = JSON.parse(new TextDecoder().decode(await this.codec(82,new Uint8Array(event.data),received)));
        if (generation !== this.generation) return;
        this.state = result.state; this.onState(this.state);
        if (this.state.key && this.state.key !== this.key) { this.key = this.state.key; saveKey(this.state.code, this.key); }
        if(this.state.peer!=null && this.reconnecting) {
          this.reconnecting=false;this.reconnectUntil=0;this.reconnectAttempts=0;
          if(frontend.notice)frontend.notice.textContent='';
        }
        if (this.started && !this.loading && this.ready()) void this.audio().loadRemoteModels(this.state.lobby?.players ?? []).catch(e=>this.report(e.message));
        this.follow(generation);
      }).catch(error => {this.report(error.message); socket.close(1008,'Invalid session message');}).finally(()=>this.pendingReceives--);
    };
    socket.onclose = event => {
      if (generation !== this.generation) return;
      this.lastDisconnect={room:this.state.code??code,code:event.code,reason:event.reason||'Connection lost',clean:Boolean(event.wasClean),time:new Date().toISOString(),pendingReceives:this.pendingReceives,droppedSnapshots:this.droppedSnapshots,visibility:globalThis.document?.visibilityState??'unknown'};
      try { globalThis.sessionStorage?.setItem('atv-last-disconnect',JSON.stringify(this.lastDisconnect)); } catch { /* Private browser storage may be unavailable. */ }
      console.warn('Multiplayer disconnected',this.lastDisconnect);
      const resumeRide=this.started&&!this.loading&&this.ready();
      const retryable=[1001,1005,1006,1011,1012,1013].includes(event.code)||(event.code===1000&&event.reason==='Connection timed out');
      // Every rider (not only the host) holds a slot key and may reconnect.
      const rejoin=this.key;
      if(rejoin && retryable && !this.reconnectUntil)this.reconnectUntil=performance.now()+25000;
      const retry=rejoin&&retryable&&performance.now()<this.reconnectUntil;
      const nextGeneration=++this.generation; this.clearAssets?.();
      this.remote = []; this.state = {}; this.started = false; this.loading = false; this.readySent = false;
      void this.codec(84,new Uint8Array()); this.audio()?.remoteFrame([]);
      if(retry) {
        this.reconnecting=true;
        this.retryTimer=setTimeout(()=>{
          if(this.generation!==nextGeneration)return;
          void this.connect(code,frontend,rejoin,resumeRide,true).catch(()=>{});
        },Math.min(3000,500*2**this.reconnectAttempts++));
      } else {
        this.reconnecting=false;this.reconnectUntil=0;this.loadedMap=null;
        this.onLeave(event.reason || `Connection lost (${event.code}).`);
      }
    };
    await new Promise((resolve,reject) => {
      const timer = setTimeout(() => {socket.close();reject(new Error('Lobby connection timed out.'));},10000);
      socket.onopen = async () => {
        clearTimeout(timer);
        try {
          await this.control({Join:{code:[...code].map(c=>c.charCodeAt(0)),player:{id:0,name:frontend.playerName.trim() || 'Rider',atv_model:frontend.quadId,rider_model:frontend.riderId}}});
          void this.ping(); resolve();
        }
        catch(e) {reject(e);}
      };
      socket.onerror = () => {clearTimeout(timer);reject(new Error('Unable to connect to this lobby.'));};
    });
  }
  // Rides follow the room's map: join it, or reload when the host changes it.
  follow(generation) {
    if (generation !== this.generation || !this.state.lobby?.started || this.loading) return;
    if (!this.started || this.loadedMap !== this.state.environment) void this.ride(this.state.environment, generation);
    else if (!this.readySent && this.ready()) void this.markReady(generation);
  }
  async ride(environment, generation, pose=null) {
    // Arriving while others already ride: meet them instead of the start line.
    const meet = !pose && !this.loadedMap && (this.state.lobby?.players?.length ?? 0) > 1;
    this.started = true; this.loading = true; this.readySent = false;
    try {
      await this.onStart(environment, () => generation === this.generation, pose);
      if (generation !== this.generation) return;
      this.loadedMap = environment;
      await this.audio().loadRemoteModels(this.state.lobby?.players ?? []);
    } catch (error) {
      this.report(error.message);
      // A rider who cannot load the track must not linger in the room.
      if (generation === this.generation) { forgetKey(); await this.disconnect(false); }
      return;
    }
    finally { if (generation === this.generation) this.loading = false; }
    if (generation !== this.generation) return;
    await this.markReady(generation);
    if (meet) void this.meet(generation);
    this.follow(generation);
  }
  async markReady(generation) {
    const map = this.state.lobby?.map;
    if (!map || generation !== this.generation) return;
    await this.control({Ready:{map}});
    if (generation !== this.generation) return;
    this.readySent = true;
    await this.command(111, {});
  }
  // Wait briefly for the riders already here, then appear beside one of them.
  async meet(generation) {
    for (let i = 0; i < 12 && generation === this.generation; i++) {
      await sleep(250);
      const host = this.state.lobby?.host;
      const riding = this.remote.filter(peer => !peer.stale && peer.rider_state === 'Riding').sort((a,b) => (b.peer===host)-(a.peer===host) || a.peer-b.peer);
      for (const peer of riding) {
        if (generation !== this.generation) return;
        try { await this.spawn(peer.peer); return; } catch { /* Try the next rider, or keep the start line. */ }
      }
    }
  }
  // Hosting creates a room. Opening an invite joins its live room, or starts
  // that code again on the link's map when everyone has left.
  async host(frontend, code, map, open=false) {
    if(code && !/^[A-Z0-9]{8}$/.test(code)) throw new Error('Choose exactly 8 letters or numbers, or leave the code blank.');
    const response = await fetch('/multiplayer/rooms',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(open?{code,open:true}:{code})});
    if (!response.ok) throw new Error(response.status===409?'Unable to create Game, game name already exists':response.status===429?'Too many lobby attempts or multiplayer is at capacity. Try again shortly.':`Game creation failed! (${response.status})`);
    const room = await response.json();
    if (room.open) {
      const key = savedKey(code);
      if (!key && room.players >= (room.max ?? 4)) throw new Error('Game is full!');
      await frontend.show('lobby'); return this.connect(code,frontend,key);
    }
    saveKey(room.code,room.hostToken);
    await frontend.show('lobby'); await this.connect(room.code,frontend,room.hostToken);
    await this.configureLobby(map,true);
  }
  async action(action, frontend, setup) {
    if (action.startsWith('spawn:')) {
      await this.spawn(Number(action.slice(6))); frontend.resume();
    } else if (action === 'host') {
      await this.host(frontend,(frontend.createCode ?? '').trim().toUpperCase(),setup?.map ?? frontend.tracks?.[frontend.track]?.id ?? 'env01');
    } else if (action === 'join') {
      const code = frontend.roomCode.trim().toUpperCase();
      if(!/^[A-Z0-9]{8}$/.test(code)) throw new Error('Enter the 8-letter or number lobby code.');
      // An invite always works: an ended room opens again on the link's map.
      await this.host(frontend,code,setup?.map ?? frontend.tracks?.[frontend.track]?.id ?? 'env01',true);
    } else if (action === 'share') {
      await copyRideUrl(this.inviteUrl(frontend),frontend.notice,'Invite link');
    } else if (action === 'leave') {
      forgetKey();
      await this.disconnect(); await frontend.show('multiplayer');
    }
  }
  inviteUrl(frontend) {
    return buildRideUrl(location.origin,{mode:'multiplayer',room:this.state.code,map:this.state.environment,quadId:frontend.quadId,riderId:frontend.riderId},{includePlayer:false});
  }
  // Host only: move everyone to another course without leaving the room.
  async setMap(environment) {
    const lobby = this.state.lobby, map = ENVIRONMENTS.indexOf(environment) + 1;
    if (!map) throw Error('Unsupported lobby map.');
    if (!lobby || this.state.peer !== lobby.host) throw Error('Only the host can change the track.');
    if (lobby.map !== map) await this.control({SetLobby:{epoch:lobby.epoch,map,started:true}});
  }
  // Change ATV/rider mid-session: tell the room, then reload in place.
  async changeRide(frontend) {
    const generation = this.generation, environment = this.loadedMap;
    if (!environment || !this.state.lobby) return;
    const pose = await this.command(112,{}).catch(() => null);
    await this.control({Profile:{id:0,name:frontend.playerName.trim() || 'Rider',atv_model:frontend.quadId,rider_model:frontend.riderId}});
    await this.command(110,{quad_id:frontend.quadId,rider_id:frontend.riderId});
    if (generation === this.generation) await this.ride(environment, generation, pose);
  }
  async configureLobby(environment, started) {
    const map = ENVIRONMENTS.indexOf(environment) + 1;
    if (!map) throw Error('Unsupported lobby map.');
    const generation = this.generation, deadline = performance.now() + 10000;
    // The socket opening does not mean Welcome/host admission has arrived.
    while (this.generation===generation && !this.state.lobby && performance.now()<deadline) await new Promise(resolve=>setTimeout(resolve,25));
    if (this.generation!==generation || !this.state.lobby) throw Error('Lobby admission failed or timed out.');
    if (this.state.peer!==this.state.lobby.host) throw Error('Only the lobby host can set up a ride.');
    await this.control({SetLobby:{epoch:this.state.lobby.epoch,map,started:Boolean(started)}});
    while (this.generation===generation && (this.state.lobby?.map!==map || this.state.lobby?.started!==Boolean(started)) && performance.now()<deadline) await new Promise(resolve=>setTimeout(resolve,25));
    if (this.generation!==generation || this.state.lobby?.map!==map || this.state.lobby?.started!==Boolean(started)) throw Error('Lobby setup failed or timed out.');
  }
  async disconnect(notify = true, preserveRecovery = false) {
    clearTimeout(this.retryTimer);this.retryTimer=null;
    if(!preserveRecovery){this.reconnecting=false;this.reconnectUntil=0;this.reconnectAttempts=0;this.loadedMap=null;}
    this.generation++; this.clearAssets?.(); this.socket?.close(1000,'Left lobby'); this.socket = null; this.state = {}; this.started = false; this.loading = false; this.readySent = false; this.remote = [];
    await this.codec(84,new Uint8Array()); this.audio()?.remoteFrame([]);
    if (notify) this.onState(this.state);
  }
  async send() {
    if (!this.started || this.loading || !this.readySent || !this.ready() || this.state.peer == null || this.socket?.readyState !== WebSocket.OPEN || this.socket.bufferedAmount >= 32768 || this.sendBusy) return;
    this.sendBusy = true; const generation = this.generation;
    try {
      const started=performance.now();
      const bytes = await this.codec(81,new Uint8Array([this.paused?.()?1:0]),Math.floor(started));
      this.networkEncodeMs+=performance.now()-started;this.networkEncodeCalls++;
      if (bytes.length && generation === this.generation && this.socket?.readyState === WebSocket.OPEN && this.socket.bufferedAmount < 32768) {
        this.socket.send(bytes);this.lastTraffic=performance.now();
        // Baselines advance only after an actual send. Unsent/dropped frames
        // can never become the reference for a later delta.
        if(bytes[5]===9||bytes[5]===10) await this.codec(109,bytes);
      }
    }
    catch (e) {this.report(e.message);} finally {this.sendBusy=false;}
  }
  async sample() {
    if (!this.started || this.loading || !this.ready() || this.sampleBusy) return;
    this.sampleBusy=true; const generation=this.generation;
    try {
      const audio=this.audio(); const payload={listener:this.listener(),playing:audio.remotePlaying(),audible:audio.active && audio.context?.state==='running'};
      const result=JSON.parse(new TextDecoder().decode(await this.codec(83,new TextEncoder().encode(JSON.stringify(payload)),Math.floor(performance.now()))));
      if (generation !== this.generation) return;
      this.remote=result.peers; this.state=result.state;
      for (const peer of this.remote) void this.assets(peer.atv_model,peer.rider_model).catch(e=>this.report(e.message));
      audio.remoteFrame(this.remote);
    } catch(e) {this.report(e.message);} finally {this.sampleBusy=false;}
  }
  close(recoverable=false) {
    clearInterval(this.timer);clearInterval(this.pingTimer);
    if(recoverable) { this.generation++;this.socket?.close();this.socket=null; }
    void this.disconnect(false);
  }
}

// Participant order follows original CompassArrow tint slots (002e68e0);
// The smaller overhead arrow is authored vector artwork for sharp display.
export const PLAYER_COLORS = ['#ff0000', '#0000ff', '#00ff00', '#ffff00'];
export function screenMarker(position, view, width, height) {
  if (!position?.every(Number.isFinite)) return null;
  const point=[position[0],position[1]+7,position[2],1];
  const clip=[0,1,2,3].map(r=>point.reduce((v,p,k)=>v+view[k*4+r]*p,0));
  if (!clip.every(Number.isFinite)) return null;
  const divisor=Math.max(.001,Math.abs(clip[3]));
  let x=clip[0]/divisor, y=-clip[1]/divisor;
  const edge=clip[3]<=0 || Math.abs(x)>.84 || Math.abs(y)>.72;
  if(edge) { if(Math.hypot(x,y)<.001)y=1; const scale=Math.max(Math.abs(x)/.84,Math.abs(y)/.72,1);x/=scale;y/=scale; }
  return {x:(x+1)*width/2,y:(y+1)*height/2,edge,angle:Math.atan2(y,x)};
}
export class PlayerIndicators {
  constructor(parent=document.body) {
    this.root=document.createElement('div');this.root.id='player-indicators';this.root.setAttribute('aria-hidden','true');parent.append(this.root);this.nodes=new Map();
  }
  draw(peers, lobby, view, origin, visible) {
    this.root.hidden=!visible;
    const live=new Set();
    for(const peer of visible ? peers : []) {
      if(peer.stale)continue;
      const player=lobby?.players.find(p=>p.id===peer.peer);if(!player)continue;
      const marker=screenMarker(peer.position,view,innerWidth,innerHeight);if(!marker)continue;
      live.add(peer.peer);
      let node=this.nodes.get(peer.peer);
      if(!node) { node=document.createElement('div');node.className='player-indicator';const label=document.createElement('span'),arrow=document.createElementNS('http://www.w3.org/2000/svg','svg');arrow.setAttribute('viewBox','0 0 24 24');const shape=document.createElementNS('http://www.w3.org/2000/svg','path');shape.setAttribute('d','M12 2 L21 21 L12 17 L3 21 Z');arrow.append(shape);node.append(label,arrow);this.root.append(node);this.nodes.set(peer.peer,node); }
      node.style.setProperty('--player-color',PLAYER_COLORS[peer.peer]);node.dataset.edge=String(marker.edge);
      node.lastChild.style.transform=marker.edge?`rotate(${marker.angle+Math.PI/2}rad)`:'rotate(180deg)';
      node.firstChild.textContent=player.name;
      // Keep the whole label visible on narrow screens, including edge arrows.
      const halfWidth=node.offsetWidth/2;
      node.style.left=`${Math.max(halfWidth+6,Math.min(innerWidth-halfWidth-6,marker.x))}px`;
      node.style.top=`${Math.max(node.offsetHeight+6,Math.min(innerHeight-6,marker.y))}px`;
    }
    for(const[id,node]of this.nodes)if(!live.has(id)){node.remove();this.nodes.delete(id);}
  }
}
