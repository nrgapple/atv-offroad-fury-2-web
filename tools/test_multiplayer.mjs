// Opt-in real-content Rust/WASM + Rust/workerd multiplayer integration.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
const{chromium}=createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE||'playwright');
const base=process.env.ATV_TEST_URL||'http://127.0.0.1:4186';
const browser=await chromium.launch({headless:true,args:process.env.ATV_BROWSER_GPU==='metal'?['--enable-gpu','--use-angle=metal']:[]});const errors=[];
const a=await browser.newPage({viewport:{width:1280,height:896},hasTouch:true}),b=await browser.newPage({viewport:{width:1280,height:896},hasTouch:true});
for(const page of[a,b]) {page.on('pageerror',e=>errors.push(e.message));await page.addInitScript(()=>{Object.defineProperty(document,'hasFocus',{value:()=>true});});}
const button=(page,name)=>page.getByRole('button',{name,exact:true});
const state=page=>page.evaluate(()=>window.atvTerrainStatus);
const menu=async page=>{await page.goto(base+'/play.html');await button(page,'Press START').click({timeout:90000});await button(page,'Multiplayer').click();};
try{
 await menu(a);await a.getByRole('textbox',{name:'Rider name',exact:true}).fill('Host');await button(a,'Create lobby').click();
 await a.waitForFunction(()=>window.atvTerrainStatus.multiplayer?.state.code,null,{timeout:15000});
 const code=(await state(a)).multiplayer.state.code;assert.match(code,/^[A-HJ-NP-Z2-9]{8}$/);
 await a.context().grantPermissions(['clipboard-read','clipboard-write'],{origin:base});
 await button(a,'Copy invite link').click();
 const invite=await a.evaluate(()=>navigator.clipboard.readText());
 const inviteUrl=new URL(invite);assert.equal(inviteUrl.origin,new URL(base).origin);assert.equal(inviteUrl.pathname,'/play.html');assert.deepEqual([...inviteUrl.searchParams],[['room',code]]);
 await b.goto(invite);await b.getByRole('textbox',{name:'Rider name',exact:true}).fill('Guest',{timeout:90000});
 await button(b,'Quad and Rider').click();await button(b,'Next quad').click();await button(b,'Change rider').click();await button(b,'Done').click();await button(b,'Join code').click();
 await a.waitForFunction(()=>window.atvTerrainStatus.multiplayer?.state.lobby?.players.length===2,null,{timeout:15000});
 if(process.env.ATV_MP_SCREENSHOT)await a.screenshot({path:process.env.ATV_MP_SCREENSHOT.replace('.png','-lobby.png')});
 await a.bringToFront();await button(a,'Ride together').click();
 for(const page of[a,b])await page.waitForFunction(()=>window.atvTerrainStatus.simulation||window.atvTerrainStatus.multiplayerError,null,{timeout:120000});
 for(const page of[a,b])await page.waitForFunction(()=>window.atvTerrainStatus.multiplayer?.remote.length===1 && window.atvTerrainStatus.multiplayerDraws>0 || window.atvTerrainStatus.multiplayerError,null,{timeout:60000});
 console.log('CONNECTED',JSON.stringify([await state(a),await state(b)].map(s=>({model:s.playerModel,net:s.multiplayer.state,draws:s.multiplayerDraws,error:s.multiplayerError,peer:s.multiplayer.remote.map(p=>({id:p.peer,position:p.position,state:p.rider_state,nodes:p.rider_node_matrices.length}))}))));
 for(const page of[a,b]) {let s=await state(page);assert.equal(s.multiplayerError,undefined);assert.equal(s.glError,0);assert.equal(s.multiplayer.remote.length,1);assert.ok(s.multiplayerDraws>0);}
 if(process.env.ATV_MP_SCREENSHOT){await a.screenshot({path:process.env.ATV_MP_SCREENSHOT.replace('.png','-start.png')});await b.screenshot({path:process.env.ATV_MP_SCREENSHOT.replace('.png','-guest.png')});}
 const initial=(await state(b)).multiplayer.remote[0].position;
 await a.bringToFront();await a.evaluate(()=>window.dispatchEvent(new Event('focus')));await a.keyboard.down('KeyW');await a.waitForFunction(()=>window.atvTerrainStatus.frame?.applied_input?.throttle>.5,null,{timeout:15000}); await b.waitForFunction(initial=>{const p=window.atvTerrainStatus.multiplayer?.remote[0]?.position;return p&&Math.hypot(...p.map((v,i)=>v-initial[i]))>3;},initial,{timeout:30000});await a.keyboard.up('KeyW');await a.waitForTimeout(250);
 const moved=(await state(b)).multiplayer.remote[0].position;assert.ok(Math.hypot(...moved.map((v,i)=>v-initial[i]))>3,'remote ATV moves after host throttle');
 if(process.env.ATV_MP_WRECK==='1') {
   await a.setViewportSize({width:390,height:844});
   await a.bringToFront();await a.keyboard.press('Backspace');
   await a.waitForFunction(()=>window.atvTerrainStatus.frame.wreck.phase==='Riding',null,{timeout:15000});
   await b.evaluate(()=>{window.remoteTrace=[];window.remoteTimer=setInterval(()=>{const s=window.atvTerrainStatus;const p=s.multiplayer?.remote[0];if(p){window.remoteTrace.push({state:p.rider_state,crash:p.crash_serial,quad:p.world_matrix.slice(12,15),rider:p.rider_world_matrix.slice(12,15),spatial:p.spatial,voices:s.audio.multiplayer});if(window.remoteTrace.length>1200)window.remoteTrace.shift();}},50);});
   // Optional two-thumb mode, real touch driving and airborne stunt gesture.
   await a.evaluate(()=>{localStorage.setItem('atv-mobile-one-handed','false');});
   await button(a,'Pause').click();await a.waitForFunction(()=>document.querySelector('#frontend').dataset.page==='pause',null,{timeout:10000});if(await button(a,'One-handed mobile: On').count())await button(a,'One-handed mobile: On').click();await button(a,'Resume').click();
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
 await a.close();await b.waitForFunction(()=>!window.atvTerrainStatus.multiplayer?.state.code,null,{timeout:15000});assert.equal((await state(b)).multiplayer.remote.length,0);assert.deepEqual(errors,[]);console.log(JSON.stringify({passed:true,code,remoteDistance:Math.hypot(...moved.map((v,i)=>v-initial[i])),summary}));
}finally{await browser.close();}
