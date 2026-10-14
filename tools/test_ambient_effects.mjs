// Opt-in source-content gate. No textures, models or source fixture data in Git.
import assert from 'node:assert/strict';import {createRequire} from 'node:module';
const {chromium}=createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE||'playwright');const base=process.env.ATV_TEST_URL||'http://127.0.0.1:4198';
const environment=process.env.ATV_AMBIENT_ENVIRONMENT||'env07';assert(['env07','free06'].includes(environment));const browser=await chromium.launch({headless:true});const reports=[];
try{
 for(const mobile of[false,true]){
  const page=await browser.newPage({viewport:mobile?{width:390,height:844}:{width:960,height:600}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.addInitScript(environment=>{
   const WorkerBase=Worker;window.ambientPending=0;
   window.Worker=class extends WorkerBase{constructor(...args){super(...args);this.ops=new Map();this.addEventListener('message',({data})=>{const op=this.ops.get(data.id);this.ops.delete(data.id);if(op===50)window.ambientPending--;if(op===131&&environment==='free06'&&data.bytes){const plan=JSON.parse(new TextDecoder().decode(data.bytes)).plan;window.ambientTestPlacement=plan.placements.find(p=>p.type_id===0&&p.filename==='grillsmokeblowing.emt');}});}postMessage(data,...args){if(data.op===23&&window.ambientTestPlacement){const payload=JSON.parse(new TextDecoder().decode(data.bytes)),position=window.ambientTestPlacement.position;payload.origin=[position[0],position[1]+3,position[2]-16];payload.yaw=0;data.bytes=new TextEncoder().encode(JSON.stringify(payload));}this.ops.set(data.id,data.op);if(data.op===50)window.ambientPending++;super.postMessage(data,...args);}};
   const draw=WebGL2RenderingContext.prototype.drawArraysInstanced;
   WebGL2RenderingContext.prototype.drawArraysInstanced=function(...args){
    const p=this.getParameter(this.CURRENT_PROGRAM),atlas=p&&this.getUniformLocation(p,'ambientAtlas');
    if(!atlas||!window.captureAmbientPixels)return draw.apply(this,args);
    const before=new Uint8Array(this.canvas.width*this.canvas.height*4),after=new Uint8Array(before.length);this.readPixels(0,0,this.canvas.width,this.canvas.height,this.RGBA,this.UNSIGNED_BYTE,before);draw.apply(this,args);this.readPixels(0,0,this.canvas.width,this.canvas.height,this.RGBA,this.UNSIGNED_BYTE,after);
    let changed=0;for(let i=0;i<before.length;i+=4)if(before[i]!==after[i]||before[i+1]!==after[i+1]||before[i+2]!==after[i+2])changed++;
    if(changed){window.ambientPixels={changed,glError:this.getError()};window.captureAmbientPixels=false;}
   };
  },environment);
  await page.goto(`${base}/play.html?drive=1&environment=${environment}`);await page.waitForFunction(()=>window.atvTerrainStatus?.ready||document.querySelector('#status.error'),null,{timeout:120000});assert((await page.evaluate(()=>window.atvTerrainStatus)).ready,await page.locator('#status').textContent());await page.locator('#game').focus();
  await page.evaluate(()=>window.captureAmbientPixels=true);await page.waitForFunction(()=>window.ambientPixels&&window.atvTerrainStatus.ambientEffects.frame.total_emitted>20,null,{timeout:30000});
  const active=await page.evaluate(()=>structuredClone(window.atvTerrainStatus.ambientEffects));assert.equal(active.plan.declared,environment==='env07'?5:15);assert.equal(active.active,environment==='env07'?5:21);assert.deepEqual(active.unsupported,[]);assert(active.frame.particles.length<=600);assert(active.draws<=2);assert.equal(active.gpuBytes,286192);assert(active.visible>0);
  await page.keyboard.press('Escape');await page.getByRole('button',{name:'Resume',exact:true}).waitFor();await page.waitForFunction(()=>ambientPending===0);const paused=await page.evaluate(()=>structuredClone(window.atvTerrainStatus.ambientEffects.frame));await page.waitForTimeout(300);assert.deepEqual(await page.evaluate(()=>window.atvTerrainStatus.ambientEffects.frame),paused);
  await page.evaluate(()=>document.querySelector('#reset').click());await page.waitForTimeout(400);const reset=await page.evaluate(()=>window.atvTerrainStatus.ambientEffects.frame);assert.deepEqual(reset,paused,'ATV reset preserves world particles and clock');
  await page.getByRole('button',{name:'Resume',exact:true}).click();await page.waitForFunction(t=>window.atvTerrainStatus.ambientEffects.frame.time>t,paused.time);
  await page.keyboard.press('Escape');await page.getByRole('button',{name:'Resume',exact:true}).waitFor();await page.waitForFunction(()=>ambientPending===0);const before=await page.evaluate(()=>structuredClone(window.atvTerrainStatus.ambientEffects.frame));
  await page.evaluate(()=>{window.ambientLost=document.querySelector('#game').getContext('webgl2').getExtension('WEBGL_lose_context');ambientLost.loseContext();});await page.waitForFunction(()=>!window.atvTerrainStatus.ready);await page.waitForTimeout(100);await page.evaluate(()=>ambientLost.restoreContext());await page.waitForFunction(()=>window.atvTerrainStatus.ready);assert.deepEqual(await page.evaluate(()=>window.atvTerrainStatus.ambientEffects.frame),before);
  await page.getByRole('button',{name:'Resume',exact:true}).click();await page.waitForFunction(t=>window.atvTerrainStatus.ambientEffects.frame.time>t,before.time);assert.deepEqual(errors,[]);assert.equal(await page.evaluate(()=>document.querySelector('#game').getContext('webgl2').getError()),0);
  if(process.env.ATV_AMBIENT_SCREENSHOTS)await page.screenshot({path:`${process.env.ATV_AMBIENT_SCREENSHOTS}${mobile?'-mobile':''}.png`});
  const report={environment,testStart:environment==='free06'?'source static grill placement':'native spawn',mobile,pixels:await page.evaluate(()=>window.ambientPixels),active:active.active,particles:active.frame.particles.length,draws:active.draws,gpuBytes:active.gpuBytes};reports.push(report);console.log(JSON.stringify(report));await page.close();
 }
 console.log('PASS: selected source smoke changes actual desktop/390px pixels; bounded draws, pause, ATV reset and graphics restoration retain world clock/state. Other emitter families and PS2 hardware are separate gates.');
}finally{await browser.close();}
