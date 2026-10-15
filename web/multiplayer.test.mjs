import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Multiplayer } from './multiplayer.js';
import { ENVIRONMENTS } from './launch.js';

const encode = value => new TextEncoder().encode(JSON.stringify(value));
const decode = bytes => JSON.parse(new TextDecoder().decode(bytes));
const deferred = () => { let resolve, reject; const promise = new Promise((yes,no) => {resolve=yes;reject=no;}); return {promise,resolve,reject}; };
const flush = async () => { for (let i=0;i<12;i++) await Promise.resolve(); };

function fixture() {
  const saved = new Map(['WebSocket','location','setInterval','clearInterval','setTimeout','clearTimeout'].map(key=>[key,Object.getOwnPropertyDescriptor(globalThis,key)]));
  const intervals=new Map(), timeouts=new Map();let nextTimer=0;
  globalThis.setInterval=(fn,delay)=>{const id=++nextTimer;intervals.set(id,{fn,delay});return id;};
  globalThis.clearInterval=id=>intervals.delete(id);
  globalThis.setTimeout=(fn,delay)=>{const id=++nextTimer;timeouts.set(id,{fn,delay});return id;};
  globalThis.clearTimeout=id=>timeouts.delete(id);
  globalThis.location={origin:'https://game.example',protocol:'https:'};
  const sockets=[];
  class Socket {
    static OPEN=1;
    constructor(url) {this.url=String(url);this.readyState=0;this.bufferedAmount=0;this.sent=[];this.closes=[];sockets.push(this);}
    open() {this.readyState=Socket.OPEN;return this.onopen?.();}
    send(bytes) {this.sent.push(bytes.slice());}
    message(bytes=new Uint8Array([1]).buffer) {this.onmessage?.({data:bytes});}
    close(code=1000,reason='') {this.closes.push({code,reason});this.readyState=3;this.onclose?.({code,reason});}
  }
  globalThis.WebSocket=Socket;
  const calls=[], overrides=new Map(), states=[], starts=[], leaves=[], reports=[], frames=[], models=[], assets=[], cleanups=[];
  const defaultState={peer:0,code:'ABCD2345',environment:'env01',nextMap:2,lobby:{host:0,epoch:1,map:1,started:false,players:[]}};
  const sound={active:true,context:{state:'running'},remotePlaying:()=>[],remoteFrame:v=>frames.push(v),loadRemoteModels:async v=>models.push(v)};
  const codec=async(op,bytes,index)=>{
    calls.push({op,bytes,index});
    if(overrides.has(op)) return overrides.get(op)(bytes,index);
    if(op===85) return bytes;
    if(op===82) return encode({state:defaultState,event:{}});
    if(op===83) return encode({state:mp.state,peers:[]});
    return new Uint8Array();
  };
  const mp=new Multiplayer({codec,command:async()=>({peer:null}),onState:v=>states.push(v),onStart:async map=>starts.push(map),onLeave:v=>leaves.push(v),ready:()=>true,listener:()=>({position:[0,0,0],forward:[0,0,1],velocity:[0,0,0]}),audio:()=>sound,assets:async(q,r)=>assets.push([q,r]),clearAssets:()=>cleanups.push(true),report:v=>reports.push(v)});
  const frontend={quadId:2,riderId:1,playerName:'Guest',tracks:ENVIRONMENTS.map(id=>({id})),notice:{},show:async()=>{}};
  async function connect() {
    const pending=mp.connect('ABCD2345',frontend,KEY);
    await flush();const socket=sockets.at(-1);assert(socket,'connection should construct socket');
    await socket.open();await pending;return socket;
  }
  async function cleanup() {
    mp.close();await flush();
    for(const[key,descriptor]of saved) {if(descriptor)Object.defineProperty(globalThis,key,descriptor);else delete globalThis[key];}
  }
  return {mp,calls,overrides,states,starts,leaves,reports,frames,models,assets,cleanups,sockets,Socket,intervals,timeouts,sound,frontend,defaultState,connect,cleanup};
}

const KEY='0123456789abcdef0123456789abcdef';
// These tests cover transport glue races and queue bounds, while Rust tests own
// wire-format correctness and numerical interpolation. No game assets are used.
test('handshake uses selected model and slot key URL, then pings for the shared clock',async()=>{
  const f=fixture();try {
    const socket=await f.connect();
    assert.equal(socket.binaryType,'arraybuffer');
    const url=new URL(socket.url);assert.equal(url.protocol,'wss:');assert.equal(url.pathname,'/multiplayer/rooms/ABCD2345');assert.equal(url.searchParams.get('key'),KEY);assert.equal(url.searchParams.get('host'),null);
    const join=decode(socket.sent[0]);assert.deepEqual(join.Join.player,{id:0,name:'Guest',atv_model:2,rider_model:1});assert.equal(String.fromCharCode(...join.Join.code),'ABCD2345');
    await flush();assert(decode(socket.sent[1]).Ping,'join is followed by a clock ping');
    assert.deepEqual([...f.intervals.values()].map(v=>v.delay),[50,2000]);assert.equal(f.timeouts.size,0);
    f.mp.close();await flush();assert.equal(f.intervals.size,0);assert.equal(f.mp.socket,null);
  }finally{await f.cleanup();}
});

test('stale control encoding never sends commands into a replacement room socket',async()=>{
  const f=fixture();try {
    const old=await f.connect();const pending=deferred();f.overrides.set(85,()=>pending.promise);
    const control=f.mp.control({SetLobby:{epoch:1,map:2,started:true}});
    await flush();f.mp.generation++;const replacement=new f.Socket('wss://game.example/new');replacement.readyState=f.Socket.OPEN;f.mp.socket=replacement;
    await flush();const sentBefore=old.sent.length;pending.resolve(new Uint8Array([9]));await control;
    assert.equal(old.sent.length,sentBefore);assert.equal(replacement.sent.length,0,'old-room host command must not cross sessions');
  }finally{await f.cleanup();}
});

test('outbound snapshots allow only one encode and drop frames under socket backpressure',async()=>{
  const f=fixture();try {
    const socket=await f.connect();await flush();const joined=socket.sent.length;f.mp.started=true;f.mp.readySent=true;f.mp.state={peer:0};
    socket.bufferedAmount=32768;await f.mp.send();assert.equal(f.calls.filter(c=>c.op===81).length,0);
    socket.bufferedAmount=0;const pending=deferred();f.overrides.set(81,()=>pending.promise);
    const first=f.mp.send();await f.mp.send();assert.equal(f.calls.filter(c=>c.op===81).length,1);
    socket.bufferedAmount=32768;pending.resolve(new Uint8Array([4]));await first;assert.equal(socket.sent.length,joined,'backpressure that rises during encoding must drop the stale snapshot');
    socket.bufferedAmount=0;f.overrides.set(81,async()=>new Uint8Array([5]));await f.mp.send();assert.deepEqual([...socket.sent.at(-1)],[5]);assert.equal(f.mp.sendBusy,false);
  }finally{await f.cleanup();}
});

test('old receive result cannot replace new session state or launch its map',async()=>{
  const f=fixture();try {
    const socket=await f.connect();const pending=deferred();f.overrides.set(82,()=>pending.promise);
    socket.message();await flush();assert.equal(f.calls.filter(c=>c.op===82).length,1);
    await f.mp.disconnect(false);f.mp.state={peer:2,code:'NEWROOM2'};
    pending.resolve(encode({state:{...f.defaultState,lobby:{...f.defaultState.lobby,started:true,map:7}},event:{}}));
    await f.mp.receiveQueue;
    assert.deepEqual(f.mp.state,{peer:2,code:'NEWROOM2'});assert.equal(f.states.length,0);assert.deepEqual(f.starts,[]);assert.equal(f.mp.pendingReceives,0);
  }finally{await f.cleanup();}
});

test('socket close cancels pending receive/sample and clears audio and visible peers',async()=>{
  const f=fixture();try {
    const socket=await f.connect();const receive=deferred(),sample=deferred();f.overrides.set(82,()=>receive.promise);f.overrides.set(83,()=>sample.promise);
    f.mp.started=true;f.mp.state={peer:0};f.mp.remote=[{peer:1}];
    socket.message();const rendering=f.mp.sample();await flush();
    socket.close(1000,'Host departed');const closedGeneration=f.mp.generation;
    receive.resolve(encode({state:{...f.defaultState,lobby:{...f.defaultState.lobby,started:true}},event:{}}));sample.resolve(encode({state:f.defaultState,peers:[{peer:1}]}));
    await rendering;await f.mp.receiveQueue;
    assert.deepEqual(f.mp.remote,[]);assert.deepEqual(f.mp.state,{});assert.equal(f.mp.started,false);assert.equal(f.mp.generation,closedGeneration);
    assert.deepEqual(f.frames.at(-1),[]);assert.deepEqual(f.leaves,['Host departed']);assert.equal(f.mp.sampleBusy,false);assert.equal(f.mp.pendingReceives,0);
  }finally{await f.cleanup();}
});

test('receive queue is bounded even when the worker is stalled and malformed frames close the socket',async()=>{
  const f=fixture();try {
    const socket=await f.connect();const pending=deferred();f.overrides.set(82,()=>pending.promise);
    for(let n=0;n<64;n++)socket.message();assert.equal(f.mp.pendingReceives,64);
    socket.message();assert.equal(socket.closes.at(-1).code,1008);assert.equal(socket.closes.at(-1).reason,'Control message backlog');assert.equal(f.mp.pendingReceives,64);
    pending.resolve(encode({state:f.defaultState,event:{}}));await f.mp.receiveQueue;assert.equal(f.mp.pendingReceives,0);
    const next=await f.connect();next.message(new Uint8Array(8193).buffer);assert.equal(next.closes.at(-1).code,1008);
    const third=await f.connect();third.message('text packet');assert.equal(third.closes.at(-1).code,1008);
  }finally{await f.cleanup();}
});

test('receive serialization preserves arrival ordering and stale queued packets skip decoding',async()=>{
  const f=fixture();try {
    const socket=await f.connect();const first=deferred();let count=0;
    f.overrides.set(82,()=>{count++;return count===1?first.promise:encode({state:f.defaultState,event:{}});});
    socket.message();socket.message();await flush();assert.equal(count,1,'second receive must wait for first decode');
    await f.mp.disconnect(false);first.resolve(encode({state:f.defaultState,event:{}}));await f.mp.receiveQueue;
    assert.equal(count,1,'queued old-session frames must not reach the reset Rust client');assert.equal(f.mp.pendingReceives,0);
  }finally{await f.cleanup();}
});

test('one outstanding presentation prevents worker backlog and cannot publish into a replacement room',async()=>{
  const f=fixture();try {
    await f.connect();f.mp.started=true;f.mp.state={peer:0};const pending=deferred();f.overrides.set(83,()=>pending.promise);
    const first=f.mp.sample();await f.mp.sample();assert.equal(f.calls.filter(c=>c.op===83).length,1);
    const request=decode(f.calls.find(c=>c.op===83).bytes);assert.equal(request.audible,true);assert.deepEqual(request.playing,[]);
    await f.mp.disconnect(false);pending.resolve(encode({state:f.defaultState,peers:[{peer:1,atv_model:3,rider_model:1}]}));await first;
    assert.deepEqual(f.mp.remote,[]);assert.deepEqual(f.assets,[]);assert.equal(f.mp.sampleBusy,false);
  }finally{await f.cleanup();}
});

test('codec rejection drains receives and closes current socket without retaining a presentation queue',async()=>{
  const f=fixture();try {
    const socket=await f.connect();f.overrides.set(82,async()=>{throw Error('bad protocol version');});
    socket.message();await f.mp.receiveQueue;
    assert.equal(f.mp.pendingReceives,0);assert.equal(socket.closes.at(-1).code,1008);assert(f.reports.includes('bad protocol version'));assert.deepEqual(f.mp.remote,[]);
  }finally{await f.cleanup();}
});


test('explicit disconnect disposes remote presentation assets and cannot run old socket close callbacks',async()=>{
  const f=fixture();try {
    const socket=await f.connect();const count=f.cleanups.length;
    f.mp.remote=[{peer:1}];f.mp.started=true;
    await f.mp.disconnect();
    assert.equal(f.cleanups.length,count+1);assert.deepEqual(f.mp.remote,[]);assert.deepEqual(f.frames.at(-1),[]);
    assert.equal(f.mp.socket,null);assert.equal(f.mp.started,false);assert.deepEqual(f.leaves,[],'explicit leave should not replay network close UI');
    socket.close(1000,'Late old close');assert.deepEqual(f.leaves,[]);
  }finally{await f.cleanup();}
});

test('local pause flag crosses the Rust snapshot boundary without queueing extra sends',async()=>{
  const f=fixture();try {
    await f.connect();f.mp.started=true;f.mp.readySent=true;f.mp.state={peer:0};f.mp.paused=()=>true;
    f.overrides.set(81,async()=>new Uint8Array([4]));await f.mp.send();
    assert.deepEqual([...f.calls.find(c=>c.op===81).bytes],[1]);
    f.mp.paused=()=>false;await f.mp.send();assert.deepEqual([...f.calls.filter(c=>c.op===81).at(-1).bytes],[0]);
  }finally{await f.cleanup();}
});

test('one authoritative start loads only once and abandoned map completion cannot load audio into a replacement room',async()=>{
  const f=fixture();try {
    const socket=await f.connect();const pending=deferred();f.mp.onStart=map=>{f.starts.push(map);return pending.promise;};
    f.overrides.set(82,async()=>encode({state:{...f.defaultState,environment:'env03',nextMap:4,lobby:{...f.defaultState.lobby,started:true,map:3}},event:{}}));
    socket.message();await f.mp.receiveQueue;assert.deepEqual(f.starts,['env03']);
    await f.mp.disconnect(false);pending.resolve();await flush();assert.deepEqual(f.models,[]);
  }finally{await f.cleanup();}
});

test('superseded asynchronous session configuration cannot replace the newer socket',async()=>{
  const f=fixture();try {
    const config=deferred();let calls=0;f.mp.command=()=>++calls===1?config.promise:Promise.resolve({peer:null});
    const first=f.mp.connect('OLDROOM2',f.frontend).catch(error=>({error}));await flush();assert.equal(calls,1);
    const second=f.mp.connect('NEWROOM2',f.frontend);await flush();const current=f.sockets.at(-1);await current.open();await second;
    config.resolve({peer:null});await flush();
    // Settle a regressed implementation so the assertion remains the failure,
    // instead of leaving a fake pending connection with an unexecuted timer.
    if(f.sockets.length>1)await f.sockets.at(-1).open();
    await first;assert.equal(f.sockets.length,1,'cancelled configuration must not construct a late socket');assert.equal(f.mp.socket,current);
  }finally{await f.cleanup();}
});

test('host changes the track for everyone and every rider reloads onto the new map',async()=>{
  const f=fixture();try {
    const socket=await f.connect();
    f.overrides.set(82,async()=>encode({state:{...f.defaultState,environment:'env08',lobby:{...f.defaultState.lobby,started:true,map:8}},event:{}}));
    socket.message();await f.mp.receiveQueue;await flush();
    assert.deepEqual(f.starts,['env08']);assert.equal(f.mp.loadedMap,'env08');
    assert.deepEqual(decode(socket.sent.at(-1)),{Ready:{map:8}},'arrival is reported after loading');
    f.mp.state={...f.mp.state,environment:'env08',lobby:{...f.mp.state.lobby,map:8,epoch:3}};
    await f.mp.setMap('free05');
    assert.deepEqual(decode(socket.sent.at(-1)),{SetLobby:{epoch:3,map:9,started:true}});
    f.overrides.set(82,async()=>encode({state:{...f.defaultState,environment:'free05',lobby:{...f.defaultState.lobby,started:true,map:9,epoch:4}},event:{}}));
    socket.message();await f.mp.receiveQueue;await flush();
    assert.deepEqual(f.starts,['env08','free05']);assert.deepEqual(decode(socket.sent.at(-1)),{Ready:{map:9}});
    f.mp.state={...f.mp.state,peer:1};await assert.rejects(f.mp.setMap('env02'),/host/);
    await assert.rejects(f.mp.setMap('nope'),/map/);
  }finally{await f.cleanup();}
});

test('changing ATV and rider tells the room, then reloads the same map in place',async()=>{
  const f=fixture();try {
    const socket=await f.connect();const poses=[];
    f.mp.command=async(op,value)=>op===112?{origin:[1,2,3],yaw:0.5}:op===80?{peer:null}:null;
    f.mp.onStart=async(map,_current,pose)=>{f.starts.push(map);poses.push(pose);};
    f.overrides.set(82,async()=>encode({state:{...f.defaultState,environment:'env03',lobby:{...f.defaultState.lobby,started:true,map:3}},event:{}}));
    socket.message();await f.mp.receiveQueue;await flush();
    f.frontend.quadId=14;f.frontend.riderId=0;
    await f.mp.changeRide(f.frontend);
    const profile=socket.sent.map(decode).find(p=>p.Profile);
    assert.deepEqual(profile,{Profile:{id:0,name:'Guest',atv_model:14,rider_model:0}});
    assert.deepEqual(f.starts,['env03','env03']);assert.deepEqual(poses,[null,{origin:[1,2,3],yaw:0.5}]);
    assert.deepEqual(decode(socket.sent.at(-1)),{Ready:{map:3}});
  }finally{await f.cleanup();}
});

test('an invite to an ended room opens it again on the link map, and a taken race joins it',async()=>{
  const f=fixture();const savedFetch=globalThis.fetch;try {
    const requests=[];f.frontend.roomCode='ABCD2345';const configured=[];
    f.mp.connect=async(code,_frontend,key)=>{requests.push(['connect',code,key]);};
    f.mp.configureLobby=async(map,start)=>configured.push([map,start]);
    globalThis.fetch=async(url,init)=>{requests.push([init?.method??'GET',String(url)]);
      if(!init)return {status:410,ok:false};
      return {status:201,ok:true,json:async()=>({code:'ABCD2345',hostToken:KEY})};};
    await f.mp.action('join',f.frontend,{map:'env05'});
    assert.deepEqual(requests,[['GET','/multiplayer/rooms/ABCD2345'],['POST','/multiplayer/rooms'],['connect','ABCD2345',KEY]]);
    assert.deepEqual(configured,[['env05',true]]);
    requests.length=0;
    globalThis.fetch=async(url,init)=>{requests.push([init?.method??'GET',String(url)]);return init?{status:409,ok:false}:{status:404,ok:false};};
    await f.mp.action('join',f.frontend,{map:'env05'});
    assert.deepEqual(requests.at(-1)[0],'connect');assert.equal(configured.length,1,'joining a just-created room never reconfigures it');
    globalThis.fetch=async()=>({status:200,ok:true,json:async()=>({players:4,max:4})});
    try{globalThis.sessionStorage?.clear?.();}catch{}
    await assert.rejects(f.mp.action('join',f.frontend),/Game is full/);
  }finally{globalThis.fetch=savedFetch;await f.cleanup();}
});

test('host close invalidates the pending map loader before it can resume a ride',async()=>{
  const f=fixture();try {
    const socket=await f.connect(),pending=deferred();let current;
    f.mp.onStart=(_environment,isCurrent)=>{current=isCurrent;return pending.promise;};
    f.overrides.set(82,async()=>encode({state:{...f.defaultState,lobby:{...f.defaultState.lobby,started:true}},event:{}}));
    socket.message();await f.mp.receiveQueue;assert.equal(current(),true);
    socket.close(1000,'Host left');assert.equal(current(),false);
    pending.resolve();await flush();assert.equal(f.models.length,0);
    assert.equal(f.mp.state.code,undefined);
  }finally{await f.cleanup();}
});

test('active-room admission starts exactly once even when Welcome arrives during Join send',async()=>{
 const f=fixture();try {
   f.defaultState.lobby.started=true;
   const pending=f.mp.connect('ABCD2345',f.frontend);
   await flush();const socket=f.sockets.at(-1);
   const nativeSend=socket.send.bind(socket);socket.send=bytes=>{nativeSend(bytes);socket.message();};
   await socket.open();await pending;await flush();
   assert.deepEqual(f.starts,['env01']);socket.message();await flush();assert.equal(f.starts.length,1);
 }finally{await f.cleanup();}
});
test('join presents lobby before connecting so late-start loading cannot be covered afterwards',async()=>{
 const f=fixture();try {
   const order=[];f.frontend.roomCode='ABCD2345';f.frontend.show=async()=>order.push('lobby');
   f.mp.connect=async()=>{order.push('connect');order.push('loading');};
   await f.mp.action('join',f.frontend);assert.deepEqual(order,['lobby','connect','loading']);
 }finally{await f.cleanup();}
});
test('spawn only resumes after the validated placement completes',async()=>{
 const f=fixture();try {
   let resumed=0;f.frontend.resume=()=>resumed++;f.mp.spawn=async()=>{throw Error('stale');};
   await assert.rejects(f.mp.action('spawn:1',f.frontend),/stale/);assert.equal(resumed,0);
   f.mp.spawn=async id=>assert.equal(id,1);await f.mp.action('spawn:1',f.frontend);assert.equal(resumed,1);
 }finally{await f.cleanup();}
});

test('distant, offscreen and rear player indicators stay finite and inside viewport',async()=>{
 const {screenMarker}=await import('./multiplayer.js');
 const view=[1,0,0,0,0,1,0,0,0,0,1,-1,0,0,0,0];
 for(const p of [[0,0,-100],[10000,0,-10],[0,0,100],[0,0,0]]) {
   const m=screenMarker(p,view,390,844);assert(Object.values(m).every(v=>typeof v!=='number'||Number.isFinite(v)));
   assert(m.x>=0&&m.x<=390&&m.y>=0&&m.y<=844);
 }
 assert.equal(screenMarker([NaN,0,0],view,390,844),null);
 assert.equal(screenMarker([0,0,100],view,390,844).edge,true);
});

test('a resumed browser discards old snapshot bursts without closing its lobby',async()=>{
 const f=fixture();try {
   const socket=await f.connect(),pending=deferred();f.overrides.set(82,()=>pending.promise);
   const snapshot=new Uint8Array([65,84,86,77,2,4,0,0]).buffer;
   for(let i=0;i<250;i++)socket.message(snapshot);
   assert.equal(f.mp.pendingReceives,48);assert.equal(f.mp.droppedSnapshots,202);assert.equal(socket.closes.length,0);
   socket.message();assert.equal(f.mp.pendingReceives,49,'A control message retains queue space');
   pending.resolve(encode({state:f.defaultState,event:{}}));await f.mp.receiveQueue;
   assert.equal(f.mp.pendingReceives,0);assert.equal(socket.closes.length,0);
 }finally{await f.cleanup();}
});

test('any rider\'s transport loss retries its slot key without restarting the loaded ride',async()=>{
 const f=fixture();try {
  const socket=await f.connect();f.mp.state={...f.defaultState,lobby:{...f.defaultState.lobby,started:true}};f.mp.started=true;f.mp.loadedMap='env01';
  socket.close(1006,'');assert.equal(f.leaves.length,0);assert.equal(f.mp.reconnecting,true);
  f.timeouts.get(f.mp.retryTimer).fn();await flush();const next=f.sockets.at(-1);assert.notEqual(next,socket);
  await next.open();f.overrides.set(82,()=>encode({state:{...f.defaultState,lobby:{...f.defaultState.lobby,started:true}},event:{}}));
  next.message();await f.mp.receiveQueue;
  assert.equal(f.mp.reconnecting,false);assert.equal(f.mp.started,true);assert.deepEqual(f.starts,[]);
  assert.equal(new URL(next.url).searchParams.get('key'),KEY);
  assert.equal(f.mp.lastDisconnect.code,1006);
 }finally{await f.cleanup();}
});
test('intentional leave cancels a pending host recovery',async()=>{
 const f=fixture();try {
  const socket=await f.connect();socket.close(1006,'');const retry=f.mp.retryTimer;assert(f.timeouts.has(retry));
  await f.mp.disconnect();assert.equal(f.timeouts.has(retry),false);assert.equal(f.mp.reconnecting,false);
 }finally{await f.cleanup();}
});

test('suppressed state produces no WebSocket frame and compact baseline receipt follows only actual sends',async()=>{
 const f=fixture();try {
  const socket=await f.connect();await flush();f.mp.started=true;f.mp.readySent=true;f.mp.state={peer:0};const before=socket.sent.length;
  f.overrides.set(81,async()=>new Uint8Array());await f.mp.send();assert.equal(socket.sent.length,before);assert.equal(f.calls.filter(c=>c.op===109).length,0);
  const bytes=new Uint8Array([65,84,86,77,3,9,0,0]);f.overrides.set(81,async()=>bytes);await f.mp.send();assert.equal(f.calls.filter(c=>c.op===109).length,1);assert.deepEqual([...socket.sent.at(-1)],[...bytes]);
  const delayed=deferred();f.overrides.set(81,()=>delayed.promise);const sending=f.mp.send();socket.bufferedAmount=32768;delayed.resolve(bytes);await sending;assert.equal(f.calls.filter(c=>c.op===109).length,1,'dropped compact snapshot cannot advance delta baseline');
 }finally{await f.cleanup();}
});
test('URL host setup waits for admission, then sends one atomic map/start command',async()=>{
 const f=fixture();try {
  const socket=await f.connect();await flush();f.mp.state={peer:null};const controls=f.calls.filter(c=>c.op===85).length;const pending=f.mp.configureLobby('free05',true);
  assert.equal(f.calls.filter(c=>c.op===85).length,controls,'no setup command before Welcome');
  f.mp.state={...f.defaultState};f.timeouts.values().next().value.fn();await flush();
  assert.deepEqual(decode(socket.sent.at(-1)),{SetLobby:{epoch:1,map:9,started:true}});
  f.mp.state={...f.defaultState,lobby:{...f.defaultState.lobby,map:9,started:true}};
  f.timeouts.values().next().value.fn(); // first resolved timer remains in this fake registry
  for(const timer of f.timeouts.values())timer.fn();await pending;
 }finally{await f.cleanup();}
});
test('URL host setup cannot mutate a replacement session or a guest lobby',async()=>{
 const f=fixture();try {
  await f.connect();f.mp.state={peer:null};const pending=f.mp.configureLobby('env02',true);
  f.mp.generation++;for(const timer of f.timeouts.values())timer.fn();await assert.rejects(pending,/admission/);
  f.mp.state={...f.defaultState,peer:1};await assert.rejects(f.mp.configureLobby('env02',true),/host/);
  await assert.rejects(f.mp.configureLobby('nope',true),/map/);
 }finally{await f.cleanup();}
});
