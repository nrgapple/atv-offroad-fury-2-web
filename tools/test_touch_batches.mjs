// Actual-content fixed-step touch transport regression. Replays loaded runtime
// resources into two fresh WASM instances, never edits vehicle poses or forces.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {writeFile} from 'node:fs/promises';
const {chromium}=createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || 'playwright');
const browser=await chromium.launch({headless:true});
try {
 const page=await browser.newPage({viewport:{width:390,height:844},hasTouch:true,isMobile:true});
  await page.addInitScript(() => localStorage.setItem('atv-mobile-one-handed', 'false')); // Explicit optional two-finger mode.
 const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.addInitScript(()=>{
   window.initialPackets=[];window.orbitRequests=[];
   const ops=new Set([20,21,22,23,26,27,28,29,44,45,46,47,48,49,51,52,53,64,65,67,69,91]);
   const post=Worker.prototype.postMessage;
   Worker.prototype.postMessage=function(data,...rest){
     if(ops.has(data.op)) window.initialPackets.push({op:data.op,index:data.index,bytes:data.bytes.slice()});
     if(data.op===50) window.orbitRequests.push(JSON.parse(new TextDecoder().decode(data.bytes)).orbit);
     return post.call(this,data,...rest);
   };
 });
 const url=new URL('/play.html?drive=1',process.env.ATV_TEST_URL||'http://127.0.0.1:4173');
 await page.goto(url.href);
 await page.waitForFunction(()=>window.atvTerrainStatus?.simulation,null,{timeout:90000});
 const cdp=await page.context().newCDPSession(page);
 const touch=(type,points=[])=>cdp.send('Input.dispatchTouchEvent',{type,touchPoints:points.map(p=>({radiusX:6,radiusY:6,force:1,...p}))});
 const steps=async()=>{const start=await page.evaluate(()=>window.atvTerrainStatus.simulationSteps);await page.waitForFunction(n=>window.atvTerrainStatus.simulationSteps>=n+4,start);};
 // Right touch must reacquire focus, not become a camera pointer. Both thumbs
 // include horizontal motion so a leaked orbit handler cannot pass unnoticed.
 await page.waitForFunction(()=>window.atvTerrainStatus.frame.grounded.some(Boolean),null,{timeout:10000});
 await page.evaluate(()=>window.dispatchEvent(new Event('blur')));
 const right={id:1,x:300,y:530},left={id:2,x:60,y:560};
 await touch('touchStart',[right]);await touch('touchMove',[{...right,x:350,y:600}]);await steps();
 assert.equal(await page.evaluate(()=>window.atvTerrainStatus.input.preload_axis),1);
 await touch('touchStart',[{...right,x:350,y:600},left]);
 await touch('touchMove',[{...right,x:280,y:480},{...left,x:120,y:630}]);await steps();
 assert(await page.evaluate(()=>window.orbitRequests.every(value=>value===0)),'Gameplay thumbs must never change camera orbit');
 await touch('touchCancel');await steps();
 assert.equal(await page.evaluate(()=>window.atvTerrainStatus.input.throttle),0);
 await page.evaluate(()=>window.dispatchEvent(new Event('blur')));
 const result=await page.evaluate(async()=>{
   const encode=value=>new TextEncoder().encode(JSON.stringify(value));
   const instances=await Promise.all([0,1].map(()=>WebAssembly.instantiateStreaming(fetch('atv_web.wasm'),{})));
   const invoke=(instance,op,bytes,index=0)=>{
     const wasm=instance.instance.exports;
     if(!wasm.input_resize(bytes.length))throw Error('input allocation');
     new Uint8Array(wasm.memory.buffer,wasm.input_ptr(),bytes.length).set(bytes);
     if(!wasm.run(op,index))throw Error(new TextDecoder().decode(new Uint8Array(wasm.memory.buffer,wasm.output_ptr(),wasm.output_len())));
     return JSON.parse(new TextDecoder().decode(new Uint8Array(wasm.memory.buffer,wasm.output_ptr(),wasm.output_len())));
   };
   let initial;
   for(const packet of window.initialPackets){
     for(const instance of instances){const value=invoke(instance,packet.op,packet.bytes,packet.index);if(packet.op===23)initial=value;}
   }
   let previous=initial, released=false, sawAir=false, sawLanding=false, transitions=0;
   const trace=[], expectedFrames=[], packets=[];
   const canonical=value=>JSON.stringify(value,(_,v)=>typeof v==='number'?Math.fround(v):v);
   for(let batch=0;batch<140;batch++){
     const releasing = !released && previous.preload_armed;
     if(releasing)released=true;
     const input={throttle:0,brake:0,steer:0,lean:released?.25:0,reverse:false,preload_axis:releasing?-1:released?null:1,
       touch_drive:{generation:1,held:!released,axis:released?0:1}};
     const frames=invoke(instances[0],50,encode({count:4,input,orbit:0,cycle:false}));
     for(let i=0;i<4;i++){
       const sequential=invoke(instances[1],24,encode({dt:1/60,input}));
       invoke(instances[1],46,encode({dt:1/60,orbit:0}));
       const frame=frames[i].frame;
       // Compare every numeric field at its native f32 precision.
       if(canonical(frame)!==canonical(sequential))throw Error(`batch/sequential f32 mismatch at ${batch*4+i}`);
       const grounded=previous.grounded.some(Boolean), applied=frame.applied_input;
       if(released&&previous.wreck.phase==='Riding'){
         if(applied.lean!==.25||applied.brake!==0||applied.throttle!==0)throw Error('left lean/right release must persist on either side of ground contact');
       }
       if(i<3&&grounded!==frame.grounded.some(Boolean))transitions++;
       if(released&&!frame.grounded.some(Boolean))sawAir=true;
       else if(sawAir&&released&&frame.grounded.some(Boolean))sawLanding=true;
       expectedFrames.push(frame); packets.push(input);
       trace.push({step:batch*4+i,groundedBefore:grounded,groundedAfter:frame.grounded.some(Boolean),speedBefore:previous.speed,input:applied,wreck:frame.wreck.phase});
       previous=frame;
     }
     if(released&&sawAir&&sawLanding)break;
   }
   // Replay the exact same per-step inputs with each prefix phase at release,
   // then four-step batches. This finds takeoff inside a batch without any
   // pose/force modification or changing when the player released preload.
   const releaseStart=packets.findIndex(p=>p.preload_axis===-1);
   let insideTakeoff=null;
   for(let phase=1;phase<=3;phase++){
   // A player reset preserves world/effects clocks and increments placement
   // epoch. Start a fresh loaded instance to replay the whole frame identically.
   instances[0]=await WebAssembly.instantiateStreaming(fetch('atv_web.wasm'),{});
   for(const packet of window.initialPackets)invoke(instances[0],packet.op,packet.bytes,packet.index);
   for(let offset=0;offset<packets.length;){
     const input=packets[offset];let count=offset===releaseStart?phase:Math.min(4,packets.length-offset);
     while(count>1&&JSON.stringify(packets[offset+count-1])!==JSON.stringify(input))count--;
     const frames=invoke(instances[0],50,encode({count,input,orbit:0,cycle:false}));
     for(let i=0;i<count;i++){
       const frame=frames[i].frame, step=offset+i;
       if(canonical(frame)!==canonical(expectedFrames[step]))throw Error(`shifted chunk f32 mismatch at ${step}`);
       if(i<count-1&&trace[step].groundedBefore&&!trace[step].groundedAfter){
         const next=frames[i+1].frame.applied_input;
         if(next.lean!==.25||next.brake!==0||next.throttle!==0)throw Error('takeoff inside batch changed left lean or retained released right throttle');
         insideTakeoff={phase,step,nextStep:step+1,input:next};
       }
     }
     offset+=count;
   }
   }
   const cancel={throttle:0,brake:0,steer:0,lean:0,reverse:false,touch_drive:{generation:2,held:false,axis:0}};
   const cancelled=invoke(instances[0],50,encode({count:4,input:cancel,orbit:0,cycle:false}));
   for(const {frame} of cancelled){
     const sequential=invoke(instances[1],24,encode({dt:1/60,input:cancel}));
     invoke(instances[1],46,encode({dt:1/60,orbit:0}));
     if(frame.applied_input.throttle!==0||frame.applied_input.brake!==0||frame.applied_input.lean!==0)throw Error('cancelled input carried through batch');
     if(JSON.stringify(frame.applied_input)!==JSON.stringify(sequential.applied_input))throw Error('cancel batch mismatch');
   }
   return {released,sawAir,sawLanding,transitions,insideTakeoff,steps:trace.length,cancelSteps:cancelled.length,trace};
 });
 if(process.env.ATV_TOUCH_BATCH_TRACE)await writeFile(process.env.ATV_TOUCH_BATCH_TRACE,JSON.stringify(result,null,2));
 assert(result.released&&result.sawAir&&result.sawLanding,'Original map must reach preload/takeoff/landing');
 assert(result.insideTakeoff,'Takeoff must occur inside a batch and the following step must retain lean with throttle/brake0');
 assert(result.transitions>0,'At least one ground/air transition must occur within a four-step batch');
 assert.deepEqual(errors,[]);
 console.log(`PASS: ${result.steps} original-map steps match four-step batches vs sequential calls; left lean and right release remain independent through takeoff/landing; both portrait thumb drags preserve orbit after focus loss.`);
}finally{await browser.close();}
