// Browser socket and drawing glue. Rust owns the protocol, identity validation,
// pose smoothing, lobby state and audio decisions.
export class Multiplayer {
  constructor({ codec, command, onState, onStart, onLeave, ready, paused, listener, audio, assets, clearAssets, spawn, report }) {
    Object.assign(this, {codec,command,onState,onStart,onLeave,ready,paused,listener,audio,assets,clearAssets,spawn,report});
    this.state = {}; this.socket = null; this.remote = []; this.started = false; this.loading = false; this.generation = 0;
    this.sendBusy = false; this.sampleBusy = false; this.receiveQueue = Promise.resolve(); this.pendingReceives = 0; this.droppedSnapshots = 0; this.lastDisconnect = null; this.reconnecting=false; this.reconnectUntil=0; this.reconnectAttempts=0; this.retryTimer=null;
    this.timer = setInterval(() => { void this.send(); if(this.paused?.()) void this.sample(); }, 50);
    this.pingTimer = setInterval(() => { if (this.socket?.readyState === WebSocket.OPEN) void this.control({Ping:{nonce:Math.floor(performance.now())>>>0}}).catch(e=>this.report(e.message)); }, 2000);
  }
  async control(packet) {
    const socket=this.socket, generation=this.generation;
    const bytes = await this.codec(85, new TextEncoder().encode(JSON.stringify(packet)));
    if (generation===this.generation && socket===this.socket && socket?.readyState === WebSocket.OPEN && socket.bufferedAmount < 32768) socket.send(bytes);
  }
  async connect(code, frontend, hostToken, resume=false, recovering=false) {
    await this.disconnect(false,recovering);
    const generation = ++this.generation;
    const state = await this.command(80, {quad_id:frontend.quadId,rider_id:frontend.riderId});
    if (generation !== this.generation) return;
    this.state = state; this.started = resume; this.reconnecting=recovering;
    const url = new URL(`/multiplayer/rooms/${encodeURIComponent(code)}`, location.origin); url.protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
    if (hostToken) url.searchParams.set('host',hostToken);
    const socket = this.socket = new WebSocket(url); socket.binaryType = 'arraybuffer';
    socket.onmessage = event => {
      if (generation !== this.generation) return;
      // Bounded work queue; a stalled worker must not retain unbounded packets.
      if (!(event.data instanceof ArrayBuffer) || event.data.byteLength > 8192) { socket.close(1008,'Message limit'); return; }
      // Mobile resume can deliver a burst of old poses. Drop surplus pose
      // frames, leaving 16 queue slots for roster/control messages. Rust still
      // validates every admitted packet; this sniff only decides what to drop.
      const header=new Uint8Array(event.data);
      if(this.pendingReceives>=48 && header.length>=8 && header[0]===65 && header[1]===84 && header[2]===86 && header[3]===77 && header[4]===2 && header[5]===4) { this.droppedSnapshots++; return; }
      if(this.pendingReceives>=64) { socket.close(1008,'Control message backlog'); return; }
      const received = Math.floor(performance.now()); this.pendingReceives++;
      this.receiveQueue = this.receiveQueue.then(async () => {
        if (generation !== this.generation) return;
        const result = JSON.parse(new TextDecoder().decode(await this.codec(82,new Uint8Array(event.data),received)));
        if (generation !== this.generation) return;
        this.state = result.state; this.onState(this.state);
        if(this.state.peer!=null && this.reconnecting) {
          this.reconnecting=false;this.reconnectUntil=0;this.reconnectAttempts=0;
          if(frontend.notice)frontend.notice.textContent='Reconnected to lobby.';
        }
        if (this.started && !this.loading && this.ready()) void this.audio().loadRemoteModels(this.state.lobby?.players ?? []).catch(e=>this.report(e.message));
        if (this.state.lobby?.started && !this.started) {
          this.started = true; this.loading = true;
          void this.onStart(this.state.environment, () => generation === this.generation).then(async () => {
            if (generation === this.generation) await this.audio().loadRemoteModels(this.state.lobby.players);
          }).catch(error => this.report(error.message)).finally(()=>{if(generation===this.generation)this.loading=false;});
        }
      }).catch(error => {this.report(error.message); socket.close(1008,'Invalid session message');}).finally(()=>this.pendingReceives--);
    };
    socket.onclose = event => {
      if (generation !== this.generation) return;
      this.lastDisconnect={room:this.state.code??code,code:event.code,reason:event.reason||'Connection lost',clean:Boolean(event.wasClean),time:new Date().toISOString(),pendingReceives:this.pendingReceives,droppedSnapshots:this.droppedSnapshots,visibility:globalThis.document?.visibilityState??'unknown'};
      try { globalThis.sessionStorage?.setItem('atv-last-disconnect',JSON.stringify(this.lastDisconnect)); } catch { /* Private browser storage may be unavailable. */ }
      console.warn('Multiplayer disconnected',this.lastDisconnect);
      const resumeRide=this.started&&!this.loading&&this.ready();
      const retryable=[1001,1005,1006,1011,1012,1013].includes(event.code)||(event.code===1000&&event.reason==='Connection timed out');
      if(hostToken && retryable && !this.reconnectUntil)this.reconnectUntil=performance.now()+25000;
      const retry=hostToken&&retryable&&performance.now()<this.reconnectUntil;
      const nextGeneration=++this.generation; this.clearAssets?.();
      this.remote = []; this.state = {}; this.started = false; this.loading = false;
      void this.codec(84,new Uint8Array()); this.audio()?.remoteFrame([]);
      if(retry) {
        this.reconnecting=true;
        if(frontend.notice)frontend.notice.textContent='Connection interrupted. Reconnecting…';
        this.retryTimer=setTimeout(()=>{
          if(this.generation!==nextGeneration)return;
          void this.connect(code,frontend,hostToken,resumeRide,true).catch(()=>{});
        },Math.min(3000,500*2**this.reconnectAttempts++));
      } else {
        this.reconnecting=false;this.reconnectUntil=0;
        this.onLeave(event.reason || `Connection lost (${event.code}).`);
      }
    };
    await new Promise((resolve,reject) => {
      const timer = setTimeout(() => {socket.close();reject(new Error('Lobby connection timed out.'));},10000);
      socket.onopen = async () => {
        clearTimeout(timer);
        try { await this.control({Join:{code:[...code].map(c=>c.charCodeAt(0)),player:{id:0,name:frontend.playerName.trim() || 'Rider',atv_model:frontend.quadId,rider_model:frontend.riderId}}}); resolve(); }
        catch(e) {reject(e);}
      };
      socket.onerror = () => {clearTimeout(timer);reject(new Error('Unable to connect to this lobby.'));};
    });
  }
  async action(action, frontend) {
    if (action.startsWith('spawn:')) {
      await this.spawn(Number(action.slice(6))); frontend.resume();
    } else if (action === 'host') {
      const code = (frontend.createCode ?? '').trim().toUpperCase();
      if(code && !/^[A-Z0-9]{8}$/.test(code)) throw new Error('Choose exactly 8 letters or numbers, or leave the code blank.');
      const response = await fetch('/multiplayer/rooms',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({code})});
      if (!response.ok) throw new Error(response.status===409?'That lobby code is taken. Choose another code.':`Lobby creation failed (${response.status}).`);
      const {code:createdCode,hostToken} = await response.json();
      try { globalThis.sessionStorage?.setItem('atv-host-session',JSON.stringify({code:createdCode,hostToken})); } catch { /* Memory-only host recovery remains available. */ }
      await frontend.show('lobby'); await this.connect(createdCode,frontend,hostToken);
    } else if (action === 'join') {
      const code = frontend.roomCode.trim().toUpperCase();
      if(!/^[A-Z0-9]{8}$/.test(code)) throw new Error('Enter the 8-letter or number lobby code.');
      let token;
      try {const saved=JSON.parse(globalThis.sessionStorage?.getItem('atv-host-session')??'null');if(saved?.code===code)token=saved.hostToken;} catch { /* Join as guest when no owner session is stored. */ }
      await frontend.show('lobby'); await this.connect(code,frontend,token);
    } else if (action === 'share') {
      const url = new URL('/play.html',location.origin); url.searchParams.set('room',this.state.code);
      try { await navigator.clipboard.writeText(url.href); frontend.notice.textContent = 'Invite link copied.'; }
      catch { frontend.notice.textContent = url.href; }
    } else if (action === 'lobbymap') {
      const lobby = this.state.lobby; if (lobby) await this.control({SetLobby:{epoch:lobby.epoch,map:this.state.nextMap,started:false}});
    } else if (action === 'lobbystart') {
      const lobby = this.state.lobby; if (lobby) await this.control({SetLobby:{epoch:lobby.epoch,map:lobby.map,started:true}});
    } else if (action === 'leave') {
      try { globalThis.sessionStorage?.removeItem('atv-host-session'); } catch { /* Storage may be unavailable. */ }
      await this.disconnect(); await frontend.show('multiplayer');
    }
  }
  async disconnect(notify = true, preserveRecovery = false) {
    clearTimeout(this.retryTimer);this.retryTimer=null;
    if(!preserveRecovery){this.reconnecting=false;this.reconnectUntil=0;this.reconnectAttempts=0;}
    this.generation++; this.clearAssets?.(); this.socket?.close(1000,'Left lobby'); this.socket = null; this.state = {}; this.started = false; this.loading = false; this.remote = [];
    await this.codec(84,new Uint8Array()); this.audio()?.remoteFrame([]);
    if (notify) this.onState(this.state);
  }
  async send() {
    if (!this.started || this.loading || !this.ready() || this.state.peer == null || this.socket?.readyState !== WebSocket.OPEN || this.socket.bufferedAmount >= 32768 || this.sendBusy) return;
    this.sendBusy = true; const generation = this.generation;
    try { const bytes = await this.codec(81,new Uint8Array([this.paused?.()?1:0]),Math.floor(performance.now())); if (generation === this.generation && this.socket?.readyState === WebSocket.OPEN && this.socket.bufferedAmount < 32768) this.socket.send(bytes); }
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
      const distance=Math.round(Math.hypot(...peer.position.map((v,i)=>v-origin[i])));
      node.firstChild.textContent=`P${peer.peer+1} ${player.name} · ${distance} ft`;
      // Keep the whole label visible on narrow screens, including edge arrows.
      const halfWidth=node.offsetWidth/2;
      node.style.left=`${Math.max(halfWidth+6,Math.min(innerWidth-halfWidth-6,marker.x))}px`;
      node.style.top=`${Math.max(node.offsetHeight+6,Math.min(innerHeight-6,marker.y))}px`;
    }
    for(const[id,node]of this.nodes)if(!live.has(id)){node.remove();this.nodes.delete(id);}
  }
}
