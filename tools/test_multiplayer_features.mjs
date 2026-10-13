// Actual WASM/relay regression: late join, UI, moving train and owner contacts.
// Worker interception is test-only placement/readback; no production debug API.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
const {chromium}=createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE||'playwright');
const base=process.env.ATV_TEST_URL||'http://127.0.0.1:4195';
const browser=await chromium.launch({headless:true,args:process.env.ATV_BROWSER_GPU==='metal'?['--enable-gpu','--use-angle=metal']:[]});
const errors=[],pages=[];const state=p=>p.evaluate(()=>window.atvTerrainStatus),button=(p,name)=>p.getByRole('button',{name,exact:true});
async function page(name) {
 const p=await browser.newPage({viewport:{width:1280,height:896},hasTouch:true});pages.push(p);p.on('pageerror',e=>errors.push(e.message));
 await p.addInitScript(()=>{
   Object.defineProperty(document,'hasFocus',{value:()=>true});
   Object.defineProperty(document,'hidden',{get:()=>Boolean(window.testFreeze)});
   const NativeSocket=WebSocket;window.WebSocket=class extends NativeSocket {constructor(...args){super(...args);window.testSocket=this;}};
   const Native=Worker;let id=-1000;
   window.Worker=class extends Native {constructor(...args){super(...args);window.testWorker=this;this.pending=new Map();this.addEventListener('message',({data})=>{const r=this.pending.get(data.id);if(!r)return;this.pending.delete(data.id);data.error?r.reject(Error(data.error)):r.resolve(JSON.parse(new TextDecoder().decode(data.bytes)));});}};
   window.testCommand=(op,value)=>new Promise((resolve,reject)=>{const key=id--;testWorker.pending.set(key,{resolve,reject});testWorker.postMessage({id:key,op,index:0,bytes:new TextEncoder().encode(value==null?'':JSON.stringify(value))});});
 });
 await p.goto(base+'/play.html');await button(p,'Press START').click({timeout:90000});await button(p,'Multiplayer').click();assert.equal(await p.getByRole('textbox').count(),0);await button(p,name==='Host'?'Create lobby':'Join lobby').click();await p.getByRole('textbox',{name:'Rider name',exact:true}).fill(name);return p;
}
async function ready(p,count=1){await p.waitForFunction(n=>window.atvTerrainStatus.simulation&&window.atvTerrainStatus.multiplayer?.remote.length===n&&window.atvTerrainStatus.multiplayerDraws>0,count,{timeout:120000});assert.equal((await state(p)).multiplayerError,undefined);}
async function pause(p){await button(p,'Pause').click();await button(p,'Resume').waitFor();await p.waitForTimeout(120);}
try {
 const a=await page('Host');const desired='F'+Date.now().toString(36).toUpperCase().slice(-7);await a.getByRole('textbox',{name:'Lobby code (8 letters/numbers, optional)',exact:true}).fill(desired);await button(a,'Create lobby').click();await a.waitForFunction(()=>window.atvTerrainStatus.multiplayer.state.code);
 const code=(await state(a)).multiplayer.state.code;assert.equal(code,desired);
 for(let i=2;i<=8;i++){await button(a,'Next environment').click();await a.waitForFunction(i=>window.atvTerrainStatus.multiplayer.state.lobby.map===i,i);}
 await button(a,'Ride together').click();await a.waitForFunction(()=>window.atvTerrainStatus.simulation,null,{timeout:120000});
 await a.waitForFunction(()=>window.atvTerrainStatus.train?.time>6,null,{timeout:20000});
 const b=await page('Late Guest');await b.getByRole('textbox',{name:'Lobby code',exact:true}).fill(code);await button(b,'Join lobby').click();
 await Promise.all([ready(a),ready(b)]);
 await b.waitForFunction(()=>document.querySelector('#frontend').hidden,null,{timeout:15000});
 await Promise.all(pages.map(p=>p.waitForFunction(()=>window.atvTerrainStatus.minimap.peers?.length===1&&document.querySelectorAll('.player-indicator').length===1,null,{timeout:15000})));
 assert.equal((await state(a)).multiplayer.state.code,code);assert.equal((await state(b)).multiplayer.state.code,code);
 await b.waitForTimeout(1200);
 const phases=await Promise.all(pages.map(async p=>(await state(p)).train.time));assert(Math.abs(phases[0]-phases[1])<.25,JSON.stringify(phases));
 console.log('LATE_JOIN',JSON.stringify({code,phases,draws:await Promise.all(pages.map(async p=>(await state(p)).multiplayerDraws))}));
 // A code-less transport close must recover the owner without reloading the
 // environment, resetting the ride or leaving observers on the old sequence.
 const beforeRecovery=await state(a);
 await a.evaluate(()=>window.testSocket.close());
 await a.waitForFunction(()=>window.atvTerrainStatus.multiplayer.lastDisconnect&&window.atvTerrainStatus.multiplayer.state.peer===0&&!window.atvTerrainStatus.multiplayer.reconnecting,null,{timeout:15000});
 await Promise.all([ready(a),ready(b)]);
 await b.waitForFunction(()=>window.atvTerrainStatus.multiplayer.remote[0]?.stale===false,null,{timeout:15000});
 const afterRecovery=await state(a);assert.equal(afterRecovery.multiplayer.state.code,code);
 assert(afterRecovery.train.time>=beforeRecovery.train.time,'Recovery must preserve running environment');
 assert.equal((await state(b)).multiplayer.state.code,code);
 console.log('HOST_RECOVERY',JSON.stringify({close:afterRecovery.multiplayer.lastDisconnect,trainBefore:beforeRecovery.train.time,trainAfter:afterRecovery.train.time}));
 // Host stops at a safe ground position; guest spawns through the actual pause UI.
 await pause(a);await b.waitForFunction(()=>window.atvTerrainStatus.multiplayer.remote[0]?.rider_state==='Riding',null,{timeout:15000});
 await pause(b);await button(b,'Players').click();await button(b,'Spawn beside P1 Host').click();await b.waitForFunction(()=>document.querySelector('#frontend').hidden);
 await b.waitForTimeout(200);let near=await state(b);const host=near.multiplayer.remote[0].position;
 assert(Math.hypot(...near.frame.origin.map((v,i)=>v-host[i]))<25);assert(Math.hypot(...near.frame.origin.map((v,i)=>v-host[i]))>5);
 console.log('SPAWN',JSON.stringify({origin:near.frame.origin,host}));
 await b.setViewportSize({width:390,height:844});await b.waitForTimeout(350);
 const labelBox=await b.locator('.player-indicator').boundingBox();assert(labelBox.x>=0&&labelBox.x+labelBox.width<=390,'edge label must fit mobile screen');
 const view=await state(b);await a.evaluate(v=>testCommand(23,v),{origin:[view.frame.origin[0]+Math.sin(view.camera[3])*70,host[1],view.frame.origin[2]+Math.cos(view.camera[3])*70],yaw:0});
 await b.waitForTimeout(350);
 const indicator=await b.locator('.player-indicator').evaluate(n=>({font:parseFloat(getComputedStyle(n).fontSize),svg:parseFloat(getComputedStyle(n.querySelector('svg')).width),bitmap:n.querySelectorAll('canvas,img').length}));
 assert.equal(indicator.font,18);assert.equal(indicator.svg,18);assert.equal(indicator.bitmap,0);
 if(process.env.ATV_MP_SCREENSHOT)await b.screenshot({path:process.env.ATV_MP_SCREENSHOT});
 await pause(b);await b.setViewportSize({width:390,height:844});await button(b,'Players').click();
 await b.waitForTimeout(400);const hit=await button(b,'Spawn beside P1 Host').boundingBox();console.log('MOBILE_HIT',JSON.stringify(hit));assert(hit.width>40&&hit.height>=40);assert(hit.x>=0&&hit.x+hit.width<=390);
 if(process.env.ATV_MP_SCREENSHOT)await b.screenshot({path:process.env.ATV_MP_SCREENSHOT.replace('.png','-players.png')});
 await button(b,'Back').click();await b.setViewportSize({width:1280,height:896});
 // Put both owned vehicles into overlapping open ground proxies. Each owner
 // independently resolves its half; snapshots must propagate that response.
 const placement={origin:host,yaw:0};
 await a.evaluate(v=>testCommand(23,v),placement);await b.evaluate(v=>testCommand(23,{...v,origin:[v.origin[0]+4,v.origin[1],v.origin[2]]}),placement);
 await a.waitForTimeout(200);await button(a,'Resume').click();await button(b,'Resume').click();
 await a.waitForTimeout(700);const resolved=await Promise.all(pages.map(state));assert(resolved.some(s=>s.multiplayer.state.contacts>0));assert(Math.hypot(...resolved[0].frame.origin.map((v,i)=>v-resolved[1].frame.origin[i]))>5.5);
 const contacts=await Promise.all(pages.map(async p=>(await state(p)).multiplayer.state.contacts));console.log('CONTACTS',JSON.stringify(contacts));
 // Place the host just above the recovered flatbed collision deck, then let
 // actual suspension contacts attach it. No fabricated network attachment.
 await pause(a);await pause(b);
 // Online menus intentionally keep physics live. Freeze test clocks only while
 // placing on the moving deck, so the placement/velocity share one train frame.
 await Promise.all(pages.map(p=>p.evaluate(()=>{window.testFreeze=true;document.dispatchEvent(new Event('visibilitychange'));})));await a.waitForTimeout(150);
 const trainSetup=await a.evaluate(async()=>{
   const previous=await testCommand(68,null);const train=await testCommand(68,{dt:1/60});const car=train.cars.findIndex(c=>/flatbeda/i.test(c.model));const m=train.cars[car].matrix;
   const origin=[0,1,2].map(i=>m[12+i]+m[4+i]*17.+m[8+i]*10);
   const velocity=[0,1,2].map(i=>(m[12+i]-previous.cars[car].matrix[12+i])*60);
   await testCommand(23,{origin,velocity,yaw:Math.atan2(m[8],m[10])});
   return {car,origin,model:train.cars[car].model};
 });
 await Promise.all(pages.map(p=>p.evaluate(()=>{window.testFreeze=false;document.dispatchEvent(new Event('visibilitychange'));})));
 await button(a,'Resume').click();await button(b,'Resume').click();
 await b.waitForFunction(()=>window.atvTerrainStatus.multiplayer.remote[0]?.platform!=null,null,{timeout:15000});
 // Allow the landing transition and initial packets to reach the observer
 // before comparing steady deck-relative pose (not simultaneous airborne poses).
 await b.waitForTimeout(750);
 const trainSamples=[];
 for(let i=0;i<20;i++){
   const [own,observer]=await Promise.all([state(a),state(b)]);const peer=observer.multiplayer.remote[0];
   if(peer.platform!=null){
     const local=(m,p)=>[0,1,2].map(axis=>[0,1,2].reduce((sum,j)=>sum+m[axis*4+j]*(p[j]-m[12+j]),0));
     const ownerLocal=local(own.train.frame.cars[peer.platform].matrix,own.frame.origin);
     const seenLocal=local(observer.train.frame.cars[peer.platform].matrix,peer.position);
     trainSamples.push({error:Math.hypot(...ownerLocal.map((v,j)=>v-seenLocal[j])),ownerLocal,seenLocal});
   }
   await b.waitForTimeout(50);
 }
 assert(trainSamples.length>=10,JSON.stringify({trainSetup,trainSamples}));
 const maxTrainError=Math.max(...trainSamples.map(s=>s.error));assert(maxTrainError<4,JSON.stringify(trainSamples));
 console.log('TRAIN_RIDER',JSON.stringify({trainSetup,samples:trainSamples.length,maxTrainError}));
 if(process.env.ATV_MP_SCREENSHOT)await b.screenshot({path:process.env.ATV_MP_SCREENSHOT.replace('.png','-train.png')});
 // Opposing fast riders must enter the recovered native wreck flow on both
 // owners, and observers must receive a detached rider plus a new crash serial.
 await Promise.all(pages.map(p=>p.evaluate(()=>{window.testFreeze=true;document.dispatchEvent(new Event('visibilitychange'));})));await a.waitForTimeout(150);
 const beforeCrashes=await Promise.all(pages.map(state));
 await Promise.all(pages.map((p,i)=>p.evaluate(async({host,i})=>{
   await testCommand(23,{origin:[host[0],host[1]+1,host[2]+(i?8:-8)],velocity:[0,0,i?-60:60],yaw:i?Math.PI:0});
   window.testImpacts=[];window.testImpactTimer=setInterval(()=>{const s=window.atvTerrainStatus;window.testImpacts.push({frames:s.frames,steps:s.simulationSteps,origin:s.frame.origin,speed:s.frame.speed,visible:!document.hidden,focused:document.hasFocus(),wreck:s.frame.wreck,contacts:s.multiplayer.state.contacts,peer:s.multiplayer.remote[0]});},30);
 },{host,i})));
 await Promise.all(pages.map(p=>p.evaluate(()=>{window.testFreeze=false;document.dispatchEvent(new Event('visibilitychange'));})));
 await a.waitForTimeout(2000);
 const impacts=await Promise.all(pages.map(p=>p.evaluate(()=>{clearInterval(testImpactTimer);return testImpacts;})));
 for(let i=0;i<impacts.length;i++) {
   const samples=impacts[i],wreck=samples.find(s=>s.wreck.phase==='Falling'&&s.wreck.kind==='Front');
   if(!wreck)console.error('HEAD_ON_DIAGNOSTIC',JSON.stringify({i,first:samples[0]&&{steps:samples[0].steps,origin:samples[0].origin,wreck:samples[0].wreck},last:samples.at(-1)&&{steps:samples.at(-1).steps,origin:samples.at(-1).origin,wreck:samples.at(-1).wreck}}));
   assert(wreck,`Owner ${i} should crash front-first: ${JSON.stringify(samples.map(s=>({wreck:s.wreck,contacts:s.contacts})))}`);
   assert(samples.some(s=>s.contacts>beforeCrashes[i].multiplayer.state.contacts),'contact response must occur');
   const remote=samples.find(s=>s.peer?.crash_serial>beforeCrashes[i].multiplayer.remote[0].crash_serial&&s.peer.rider_state!=='Riding');
   assert(remote,`Observer ${i} should receive the crash`);
   console.log('HEAD_ON_CRASH',JSON.stringify({owner:i,kind:wreck.wreck.kind,peerState:remote.peer.rider_state,serial:remote.peer.crash_serial}));
 }
 // Reloading the owner's page uses the grace window; joining the same code
 // from this tab restores host authority from session storage.
 await a.reload();await button(a,'Press START').click({timeout:90000});await button(a,'Multiplayer').click();await button(a,'Join lobby').click();
 await a.getByRole('textbox',{name:'Rider name',exact:true}).fill('Host');await a.getByRole('textbox',{name:'Lobby code',exact:true}).fill(code);await button(a,'Join lobby').click();
 await Promise.all([ready(a),ready(b)]);assert.equal((await state(a)).multiplayer.state.peer,0);assert.equal((await state(b)).multiplayer.state.code,code);
 console.log('OWNER_RELOAD',JSON.stringify({code,peer:(await state(a)).multiplayer.state.peer}));
 // A paused guest must still receive current roster/poses. Departure removes it.
 await pause(b);await button(b,'Players').click();await pause(a);await button(a,'Lobby').click();await button(a,'Leave lobby').click();
 // Let the explicit Leave handshake complete before destroying its page. An
 // abrupt page close exercises the existing 30-second host recovery grace.
 await a.waitForFunction(()=>!window.atvTerrainStatus.multiplayer.state.code&&window.testSocket.readyState===WebSocket.CLOSED,null,{timeout:15000});
 await b.waitForFunction(()=>!window.atvTerrainStatus.multiplayer.state.code,null,{timeout:15000});assert.equal((await state(b)).multiplayer.remote.length,0);
 await a.close();
 assert.deepEqual(errors,[]);console.log('PASS late admission / minimap / smooth arrows / minimap dots / pause roster / nearby spawn / contacts / head-on wreck replication / departure');
}finally{await browser.close();}
