// Opt-in real-content Rust/WASM + Rust/workerd multiplayer integration.
import assert from 'node:assert/strict';
import { ENVIRONMENTS } from '../web/launch.js';
import {assertNoErrors,base,button,gameState as state,launch,newPage,waitForAnimations,waitForSteps} from './browser/harness.mjs';
const map=Number(process.env.ATV_MP_MAP||1);assert.ok(Number.isInteger(map)&&map>=1&&map<=ENVIRONMENTS.length,'ATV_MP_MAP must be 1..40');
const browser=await launch();const errors=[];
const a=await newPage(browser,{viewport:{width:1280,height:896},hasTouch:true},errors),b=await newPage(browser,{viewport:{width:1280,height:896},hasTouch:true},errors);
for(const page of[a,b]) {await page.addInitScript(()=>{Object.defineProperty(document,'hasFocus',{value:()=>true});});}
// Tapping the already-selected course card rides at once; otherwise select it, then Ride.
const pickTrack=async(page,id)=>{const card=page.locator(`.ms-course[data-track-id=${id}]`);await card.waitFor({timeout:60000});const selected=await card.getAttribute('aria-selected')==='true';await card.click();if(!selected)await page.locator('button.ms-ride').click();};
const menu=async page=>{await page.goto(base+'/play.html');await button(page,'Press START').click({timeout:90000});await button(page,'Multiplayer').click();};
try{
 // Host Game → Select Track → course → Ride creates the room and starts riding.
 await menu(a);await button(a,'Host Game').click();await a.getByRole('textbox',{name:'Rider name',exact:true}).fill('Host');await button(a,'Select Track').click();
 await pickTrack(a,ENVIRONMENTS[map-1]);
 await a.waitForFunction(()=>window.atvTerrainStatus.multiplayer?.state.code&&window.atvTerrainStatus.simulation||window.atvTerrainStatus.multiplayerError,null,{timeout:120000});
 assert.equal((await state(a)).multiplayerError,undefined);
 const code=(await state(a)).multiplayer.state.code;assert.match(code,/^[A-HJ-NP-Z2-9]{8}$/);
 assert.equal((await state(a)).multiplayer.state.environment,ENVIRONMENTS[map-1]);
 await a.context().grantPermissions(['clipboard-read','clipboard-write'],{origin:base});
 await button(a,'Pause').click();await button(a,'Copy invite link').click();
 const invite=await a.evaluate(()=>navigator.clipboard.readText());await button(a,'Resume').click();
 const inviteUrl=new URL(invite);assert.equal(inviteUrl.origin,new URL(base).origin);assert.equal(inviteUrl.pathname,'/play.html');assert.equal(inviteUrl.searchParams.get('room'),code);assert.equal(inviteUrl.searchParams.get('action'),'join');assert.equal(inviteUrl.searchParams.get('play'),'1');assert.equal(inviteUrl.searchParams.get('map'),ENVIRONMENTS[map-1]);
 inviteUrl.searchParams.set('player','Guest');inviteUrl.searchParams.set('quad','2');inviteUrl.searchParams.set('rider','1');
 if(process.env.ATV_MP_SCREENSHOT)await a.screenshot({path:process.env.ATV_MP_SCREENSHOT.replace('.png','-host.png')});
 await b.goto(inviteUrl.href);
 if(process.env.ATV_MP_LEAVE_DURING_LOAD==='1') {
   // The host leaves while the guest is still loading: the guest's ride goes on
   // and it becomes the room's host.
   await b.waitForFunction(()=>window.atvTerrainStatus?.multiplayer?.state.code&&!window.atvTerrainStatus.simulation,null,{timeout:60000});
   await a.close();
   await b.waitForFunction(()=>{const s=window.atvTerrainStatus,m=s.multiplayer;return s.simulation&&m?.state.code&&m.state.lobby?.host===m.state.peer&&m.state.lobby.players.length===1&&m.remote.length===0||s.multiplayerError;},null,{timeout:120000});
   // Time-based on purpose: prove the guest's menu does not reappear shortly after the handoff.
   await b.waitForTimeout(300);
   assert.equal(await b.locator('#frontend').isHidden(),true,'guest keeps riding');
   const s=await state(b);assert.equal(s.multiplayer.state.code,code);assert.equal(s.multiplayer.remote.length,0);assert.equal(s.audio.multiplayer.voices,0);assert.equal(s.multiplayerError,undefined);
   assertNoErrors(errors);console.log(JSON.stringify({passed:true,hostLeftDuringLoad:true,guestBecameHost:true,map}));
 } else {
 for(const page of[a,b])await page.waitForFunction(()=>window.atvTerrainStatus.simulation||window.atvTerrainStatus.multiplayerError,null,{timeout:120000});
 for(const page of[a,b])await page.waitForFunction(()=>window.atvTerrainStatus.multiplayer?.remote.length===1 && window.atvTerrainStatus.multiplayerDraws>0 || window.atvTerrainStatus.multiplayerError,null,{timeout:60000});
 console.log('CONNECTED',JSON.stringify([await state(a),await state(b)].map(s=>({model:s.playerModel,net:s.multiplayer.state,draws:s.multiplayerDraws,error:s.multiplayerError,peer:s.multiplayer.remote.map(p=>({id:p.peer,position:p.position,state:p.rider_state,nodes:p.rider_node_matrices.length}))}))));
 for(const page of[a,b]) {let s=await state(page);assert.equal(s.multiplayerError,undefined);assert.equal(s.glError,0);assert.equal(s.multiplayer.remote.length,1);assert.ok(s.multiplayerDraws>0);}
 for(const page of[a,b]) {const s=await state(page);assert.equal(s.multiplayer.state.lobby.map,map);assert.equal(s.multiplayer.state.environment,ENVIRONMENTS[map-1]);if(map===9)assert.match(s.source,/free05/i);}
 if(process.env.ATV_MP_SCREENSHOT){await a.screenshot({path:process.env.ATV_MP_SCREENSHOT.replace('.png','-start.png')});await b.screenshot({path:process.env.ATV_MP_SCREENSHOT.replace('.png','-guest.png')});}
 const initial=(await state(b)).multiplayer.remote[0].position;
 await a.bringToFront();await a.evaluate(()=>window.dispatchEvent(new Event('focus')));await a.keyboard.down('KeyW');await a.waitForFunction(()=>window.atvTerrainStatus.frame?.applied_input?.throttle>.5,null,{timeout:15000}); await b.waitForFunction(initial=>{const p=window.atvTerrainStatus.multiplayer?.remote[0]?.position;return p&&Math.hypot(...p.map((v,i)=>v-initial[i]))>3;},initial,{timeout:30000});await a.keyboard.up('KeyW');await waitForSteps(a,15);
 const moved=(await state(b)).multiplayer.remote[0].position;assert.ok(Math.hypot(...moved.map((v,i)=>v-initial[i]))>3,'remote ATV moves after host throttle');
 if(process.env.ATV_MP_WRECK==='1') {
   await a.setViewportSize({width:390,height:844});
   await a.bringToFront();await a.keyboard.press('Backspace');
   await a.waitForFunction(()=>window.atvTerrainStatus.frame.wreck.phase==='Riding',null,{timeout:15000});
   await b.evaluate(()=>{window.remoteTrace=[];window.remoteTimer=setInterval(()=>{const s=window.atvTerrainStatus;const p=s.multiplayer?.remote[0];if(p){window.remoteTrace.push({state:p.rider_state,crash:p.crash_serial,quad:p.world_matrix.slice(12,15),rider:p.rider_world_matrix.slice(12,15),spatial:p.spatial,voices:s.audio.multiplayer});if(window.remoteTrace.length>1200)window.remoteTrace.shift();}},50);});
   // Optional two-thumb mode, real touch driving and airborne stunt gesture.
   await a.evaluate(()=>{localStorage.setItem('atv-mobile-one-handed','false');});
   await button(a,'Pause').click();await a.waitForFunction(()=>document.querySelector('#frontend').dataset.page==='pause'&&!document.querySelector('#frontend').hidden&&!document.getAnimations().some(a=>a.effect?.target?.id==='menu-stage'),null,{timeout:10000});await waitForAnimations(a);
   // The one-handed toggle lives under Pause → Settings → Controls.
   await button(a,'Settings').click();await button(a,'Controls').click();await button(a,'Keyboard bindings').waitFor();if(await button(a,'One-handed mobile: On').count())await button(a,'One-handed mobile: On').click();await button(a,'One-handed mobile: Off').waitFor();
   await a.keyboard.press('Escape');await button(a,'Audio').waitFor();await a.keyboard.press('Escape');await button(a,'Resume').waitFor();await waitForAnimations(a);await button(a,'Resume').click();await a.waitForFunction(()=>document.querySelector('#frontend').hidden,null,{timeout:10000});
   const cdp=await a.context().newCDPSession(a);
   const touch=(type,points=[])=>cdp.send('Input.dispatchTouchEvent',{type,touchPoints:points.map(p=>({radiusX:6,radiusY:6,force:1,...p}))});
   const left={id:1,x:60,y:560},right={id:2,x:310,y:530},drive={...right,y:460};
   await touch('touchStart',[left,right]);await touch('touchMove',[left,drive]);
   await a.waitForFunction(()=>window.atvTerrainStatus.frame.preload_armed,null,{timeout:45000});
   await touch('touchEnd',[drive]);await a.waitForFunction(()=>!window.atvTerrainStatus.frame.grounded.some(Boolean),null,{timeout:10000});
   await touch('touchStart',[left,right]);await touch('touchMove',[left,{...right,y:right.y-70}]);
   await a.waitForFunction(()=>window.atvTerrainStatus.frame.rider_mode==='wreck',null,{timeout:20000});await touch('touchCancel');
   await b.waitForFunction(()=>window.remoteTrace.some(p=>p.state==='Wreck'&&Math.hypot(...p.rider.map((v,i)=>v-p.quad[i]))>1),null,{timeout:20000});
   await a.waitForFunction(()=>window.atvTerrainStatus.frame.wreck.phase==='Riding'&&!window.atvTerrainStatus.frame.rider_wreck,null,{timeout:25000});
   await b.waitForFunction(()=>window.atvTerrainStatus.multiplayer.remote[0]?.rider_state==='Riding',null,{timeout:15000});
   const trace=await b.evaluate(()=>{clearInterval(window.remoteTimer);return window.remoteTrace;});
   assert.ok(trace.some(p=>p.crash>0));assert.ok(trace.some(p=>p.voices.impacts>0),'original crash sample played remotely');
   assert.ok(new Set(trace.map(p=>p.spatial.pan)).size>1,'relative movement changes pan');
   console.log('WRECK',JSON.stringify({frames:trace.length,states:[...new Set(trace.map(p=>p.state))],crashes:[...new Set(trace.map(p=>p.crash))],detached:trace.filter(p=>p.state==='Wreck'&&Math.hypot(...p.rider.map((v,i)=>v-p.quad[i]))>1).length,impactFrames:trace.filter(p=>p.voices.impacts>0).length,pans:[...new Set(trace.map(p=>p.spatial.pan))]}));
 }
 if(process.env.ATV_MP_SCREENSHOT)await a.screenshot({path:process.env.ATV_MP_SCREENSHOT});
 const summary=[await state(a),await state(b)].map(s=>({model:s.playerModel.quad_id,rider:s.playerModel.rider_id,draws:s.multiplayerDraws,rtt:s.multiplayer.state.rttMs,sent:s.multiplayer.state.sentBytes,received:s.multiplayer.state.receivedBytes,remote:s.multiplayer.remote.map(p=>({position:p.position,state:p.rider_state,crash:p.crash_serial,nodes:p.rider_node_matrices.length})),audio:s.audio.multiplayer}));
 // Host leaves: the guest keeps riding in the same room and becomes its host.
 await a.bringToFront();await button(a,'Pause').click();await button(a,'Leave Game').click();await a.waitForFunction(()=>!window.atvTerrainStatus.multiplayer?.state.code,null,{timeout:15000});await a.close();
 await b.waitForFunction(()=>{const m=window.atvTerrainStatus.multiplayer;return m?.state.code&&m.state.lobby?.host===m.state.peer&&m.state.lobby.players.length===1&&m.remote.length===0;},null,{timeout:45000});
 const after=await state(b);assert.equal(after.multiplayer.state.code,code);assert.ok(after.simulation,'guest ride continues');assert.equal(after.multiplayerError,undefined);
 assertNoErrors(errors);console.log(JSON.stringify({passed:true,code,remoteDistance:Math.hypot(...moved.map((v,i)=>v-initial[i])),handoff:{host:after.multiplayer.state.lobby.host,peer:after.multiplayer.state.peer},summary}));
 }
}finally{await browser.close();}
