// Mirror the exact source-resource/start/input stream into a second WASM worker.
// Only the enhancement flag differs; compare every gameplay/camera/water/train
// field after removing the explicit presentation payload and echoed flag.
import assert from 'node:assert/strict';
export async function captureCrashIsolation(page) {
 await page.addInitScript(() => {
  window.crashIsolation = { compared: 0, mismatches: [], errors: [] };
  const Native = Worker;
  window.Worker = class extends Native {
   constructor(...args) {
    super(...args); this.baseline = new Native(...args); this.meta = new Map();
    const enhanced = new Map(), baseline = new Map();
    const receive = (store, other, data) => {
     if (data.error) crashIsolation.errors.push(data.error);
     if (this.meta.get(data.id) !== 50 || !data.bytes) return;
     const batches = JSON.parse(new TextDecoder().decode(data.bytes));
     for (const item of batches) { delete item.frame.funny_crash; if (item.frame.applied_input) delete item.frame.applied_input.funny_crashes; }
     store.set(data.id, JSON.stringify(batches));
     if (other.has(data.id)) {
      crashIsolation.compared += batches.length;
      if (store.get(data.id) !== other.get(data.id)) crashIsolation.mismatches.push(data.id);
      store.delete(data.id); other.delete(data.id); this.meta.delete(data.id);
     }
    };
    this.addEventListener('message',({data}) => receive(enhanced,baseline,data));
    this.baseline.addEventListener('message',({data}) => receive(baseline,enhanced,data));
   }
   postMessage(message, ...args) {
    this.meta.set(message.id,message.op);
    const mirror = {...message, bytes:message.bytes.slice()};
    if ([24,50].includes(message.op)) {
     const command=JSON.parse(new TextDecoder().decode(mirror.bytes));
     command.input.funny_crashes=false; mirror.bytes=new TextEncoder().encode(JSON.stringify(command));
    }
    this.baseline.postMessage(mirror);
    return super.postMessage(message,...args);
   }
   terminate() {this.baseline.terminate();super.terminate();}
  };
 });
}
export async function verifyCrashIsolation(page) {
 await page.waitForFunction(() => window.crashIsolation.compared > 100);
 const result=await page.evaluate(() => window.crashIsolation);
 assert.deepEqual(result.errors,[]);assert.deepEqual(result.mismatches,[]);
 console.log(`PASS: ${result.compared} exact paired WASM gameplay/camera steps with enhancement on/off.`);
 return result;
}
export async function captureCrashDraws(page) {
 await page.addInitScript(() => {
  window.crashDraws={quad:0,rider:0,baseline:0};
  const p=WebGL2RenderingContext.prototype,names=new WeakMap();
  const get=p.getUniformLocation,matrix=p.uniformMatrix4fv,draw=p.drawElements;
  let kind=null;
  p.getUniformLocation=function(program,name){const l=get.call(this,program,name);if(l)names.set(l,name);return l;};
  p.uniformMatrix4fv=function(l,transpose,v,...args){
   if(names.get(l)?.startsWith('quadBones')) {
    const status=window.atvTerrainStatus,frame=status?.frame,fx=frame?.funny_crash;
    const matches=m=>m?.length*16===v.length&&m.every((m,i)=>m.every((x,j)=>Math.fround(x)===v[i*16+j]));
    kind=status?.funnyCrashes&&fx ? matches(fx.quad_node_matrices)?'quad':matches(fx.rider_node_matrices)?'rider':null : matches(frame?.quad_node_matrices)||matches(frame?.rider_node_matrices)?'baseline':null;
   }
   return matrix.call(this,l,transpose,v,...args);
  };
  p.drawElements=function(...args){if(kind)crashDraws[kind]++;return draw.apply(this,args);};
 });
}

// Validate each mechanical damage transition against its actual contact step.
export function verifyCrashImpacts(frames) {
 let previous=0, reset=null, aerial=0, impacts=0;
 for(const {frame} of frames) {
  if(frame.reset_count!==reset) { reset=frame.reset_count; previous=0; }
  const parts=frame.funny_crash?.detached_parts??0;
  if(parts>previous) {
   assert(frame.crash_impact_speed>=(parts>2?50:25), 'New breakaway pieces require a hard actual ATV impact');
   impacts++;
  }
  if(frame.funny_crash?.rider_node_matrices.length&&parts===0&&frame.crash_impact_speed===0&&!frame.grounded.some(Boolean)) aerial++;
  if(frame.wreck.phase==='Riding')assert.equal(parts,0,'Recovery restores intact mechanical parts');
  previous=parts;
 }
 console.log(`PASS: ${aerial} intact-ATV aerial ragdoll frames; ${impacts} contact-triggered damage transitions.`);
 return {aerial,impacts};
}
