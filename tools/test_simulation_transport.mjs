// Original-content transport equivalence and optional before/after WASM timing.
// Supply ATV_BASELINE_WASM from the preceding tested build; never publish it.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {readFile,writeFile} from 'node:fs/promises';
const {chromium}=createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE||'playwright');
const browser=await chromium.launch({headless:true});
const reports=[];
try {
 for(const environment of (process.env.ATV_TRANSPORT_MAPS||'env01,env04,env08,free05').split(',')) {
  const page=await browser.newPage();const errors=[];page.on('pageerror',e=>errors.push(e.message));
  if(process.env.ATV_BASELINE_WASM)await page.route('**/transport-baseline.wasm',async route=>route.fulfill({contentType:'application/wasm',body:await readFile(process.env.ATV_BASELINE_WASM)}));
  await page.addInitScript(()=>{
   window.transportPackets=[];
   const ops=new Set([20,21,22,23,26,27,28,29,44,45,47,48,49,51,52,53,64,65,67,69,91]);
   const post=Worker.prototype.postMessage;
   Worker.prototype.postMessage=function(data,...rest){if(ops.has(data.op))transportPackets.push({op:data.op,index:data.index,bytes:data.bytes.slice()});return post.call(this,data,...rest);};
  });
  const url=new URL('/play.html?drive=1',process.env.ATV_TEST_URL||'http://127.0.0.1:4173');url.searchParams.set('environment',environment);
  await page.goto(url.href);await page.waitForFunction(()=>window.atvTerrainStatus?.simulation,null,{timeout:90000});
  assert.match(await page.evaluate(()=>window.atvTerrainStatus.source),new RegExp(environment,'i'),'Requested map must actually load');
  await page.evaluate(()=>window.dispatchEvent(new Event('blur')));
  const report=await page.evaluate(async baseline=>{
   const encode=v=>new TextEncoder().encode(JSON.stringify(v)),decode=b=>JSON.parse(new TextDecoder().decode(b));
   const create=async name=>(await WebAssembly.instantiateStreaming(fetch(name),{})).instance.exports;
   const invoke=(wasm,op,bytes,index=0)=>{
    if(!wasm.input_resize(bytes.length))throw Error('input allocation');
    new Uint8Array(wasm.memory.buffer,wasm.input_ptr(),bytes.length).set(bytes);
    if(!wasm.run(op,index))throw Error(new TextDecoder().decode(new Uint8Array(wasm.memory.buffer,wasm.output_ptr(),wasm.output_len())));
    return new Uint8Array(wasm.memory.buffer,wasm.output_ptr(),wasm.output_len()).slice();
   };
   const load=w=>{for(const p of transportPackets)invoke(w,p.op,p.bytes,p.index);};
   const batch=await create('atv_web.wasm'),sequential=await create('atv_web.wasm'),old=baseline?await create('transport-baseline.wasm'):null;
   for(const w of [batch,sequential,old].filter(Boolean))load(w);
   const canonical=v=>JSON.stringify(v,(_,x)=>typeof x==='number'?Math.fround(x):x&&typeof x==='object'&&!Array.isArray(x)?Object.fromEntries(Object.keys(x).sort().map(k=>[k,x[k]])):x);
   let steps=0,maximumBytes=0;
   for(let block=0;block<120;block++){
    const count=block%4+1,input={throttle:block<80?.6:0,brake:0,steer:block%2?.1:-.1,lean:.15,reverse:false};
    const request=encode({count,input,orbit:.01,cycle:false}),bytes=invoke(batch,50,request),frames=decode(bytes);
    if(frames.length!==count)throw Error('Intermediate frame lost');maximumBytes=Math.max(maximumBytes,bytes.length);
    if(old){const previous=decode(invoke(old,50,request));for(let i=0;i<count;i++)for(const key of Object.keys(frames[i]))if(canonical(frames[i][key])!==canonical(previous[i][key]))throw Error(`Baseline mismatch ${steps+i}:${key}`);}
    for(const item of frames){
     if(item.train)invoke(sequential,68,encode({dt:1/60}));
     // Sequential operations have identical bytes inside the new batch.
     const frame=decode(invoke(sequential,24,encode({dt:1/60,input}))),camera=decode(invoke(sequential,46,encode({dt:1/60,orbit:.01})));
     if(JSON.stringify(item.frame)!==JSON.stringify(frame))throw Error(`Frame decimal changed at ${steps}`);
     if(JSON.stringify(item.camera)!==JSON.stringify(camera))throw Error(`Camera decimal changed at ${steps}`);
     steps++;
    }
   }
   // Timing excludes resource loading, browser rendering, JS JSON parsing and
   // worker transfer. Alternate order to reduce warmup/temperature bias.
   let timings=null;
   if(old){
    const request=encode({count:4,input:{throttle:0,brake:0,steer:0,lean:0,reverse:false},orbit:0,cycle:false});
    const measure=w=>{load(w);for(let i=0;i<30;i++)invoke(w,50,request);const start=performance.now();for(let i=0;i<200;i++)invoke(w,50,request);return(performance.now()-start)/200;};
    const before=[],after=[];for(let run=0;run<5;run++){if(run%2){after.push(measure(batch));before.push(measure(old));}else{before.push(measure(old));after.push(measure(batch));}}
    const median=a=>a.toSorted((x,y)=>x-y)[2];timings={before,after,beforeMedian:median(before),afterMedian:median(after),reductionPercent:100*(1-median(after)/median(before)),scope:'V8 WASM four-step operation plus output copy; 5 alternating runs of 200 batches, after 30 warmups; no rendering/worker/physical-phone claim'};
   }
   return{steps,maximumBytes,timings};
  },Boolean(process.env.ATV_BASELINE_WASM));
  assert.deepEqual(errors,[]);reports.push({environment,...report});console.log(JSON.stringify(reports.at(-1)));await page.close();
 }
 if(process.env.ATV_TRANSPORT_REPORT)await writeFile(process.env.ATV_TRANSPORT_REPORT,JSON.stringify(reports,null,2));
 console.log('PASS: all intermediate frames/cameras retain exact sequential numbers; optional baseline matches every field at native f32 precision.');
}finally{await browser.close();}
