import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
const {chromium}=createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE||'playwright');
const base=process.env.ATV_TEST_URL||'http://127.0.0.1:4191';
const browser=await chromium.launch({headless:true});
try {
 for(const mobile of[false,true]){
  const page=await browser.newPage({viewport:mobile?{width:390,height:844}:{width:960,height:600}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.addInitScript(()=>{
   const NativeWorker=Worker;window.wheelEffectTrace=[];
   window.Worker=class extends NativeWorker{
    constructor(...args){super(...args);this.ids=new Map();this.addEventListener('message',({data})=>{const op=this.ids.get(data.id);this.ids.delete(data.id);if(op===50&&data.bytes&&!data.error){for(const sample of JSON.parse(new TextDecoder().decode(data.bytes)))window.wheelEffectTrace.push(sample.frame);}});}
    postMessage(data,...args){this.ids.set(data.id,data.op);super.postMessage(data,...args);}
   };
   const draw=WebGL2RenderingContext.prototype.drawArraysInstanced;
   WebGL2RenderingContext.prototype.drawArraysInstanced=function(...args){
    if(!window.captureWheelPixels||!this.getUniformLocation(this.getParameter(this.CURRENT_PROGRAM),'atlas'))return draw.apply(this,args);
    const before=new Uint8Array(this.canvas.width*this.canvas.height*4),after=new Uint8Array(before.length);
    this.readPixels(0,0,this.canvas.width,this.canvas.height,this.RGBA,this.UNSIGNED_BYTE,before);draw.apply(this,args);this.readPixels(0,0,this.canvas.width,this.canvas.height,this.RGBA,this.UNSIGNED_BYTE,after);
    let changed=0;for(let i=0;i<before.length;i+=4)if([0,1,2].some(c=>before[i+c]!==after[i+c]))changed++;
    if(changed){window.wheelPixelEvidence={changed,glError:this.getError()};window.captureWheelPixels=false;}
   };
  });
  try{
   await page.goto(`${base}/play.html?drive=1&environment=env04`);
   await page.waitForFunction(()=>window.atvTerrainStatus?.ready||document.querySelector('#status.error'),null,{timeout:120000});assert((await page.evaluate(()=>window.atvTerrainStatus)).ready,await page.locator('#status').textContent());
   await page.evaluate(()=>window.captureWheelPixels=true);await page.keyboard.down('KeyW');
   await page.waitForFunction(()=>window.atvTerrainStatus.wheelEffects?.frame?.total_emitted[0]>20,null,{timeout:40000});
   await page.waitForFunction(()=>window.wheelPixelEvidence,null,{timeout:30000});
   const first=await page.evaluate(()=>structuredClone(window.atvTerrainStatus.wheelEffects));assert(first.draws===1);assert(first.particles<=600);
   await page.waitForFunction(()=>window.atvTerrainStatus.frame.speed_magnitude>46,null,{timeout:40000});
   await page.keyboard.down('KeyD');await page.waitForFunction(()=>window.atvTerrainStatus.wheelEffects?.frame?.total_emitted[2]>0,null,{timeout:30000});
   await page.keyboard.up('KeyD');await page.keyboard.up('KeyW');
   const roost=await page.evaluate(()=>structuredClone(window.atvTerrainStatus.wheelEffects));assert(roost.frame.total_emitted[1]>0&&roost.frame.total_emitted[2]>0);
   if(process.env.ATV_WHEEL_EFFECT_SCREENSHOTS)await page.screenshot({path:`${process.env.ATV_WHEEL_EFFECT_SCREENSHOTS}${mobile?'-mobile':''}.png`});
   await page.keyboard.press('Escape');await page.getByRole('button',{name:'Resume',exact:true}).waitFor();await page.waitForTimeout(100);
   const paused=await page.evaluate(()=>window.atvTerrainStatus.wheelEffects.frame);await page.waitForTimeout(250);assert.deepEqual(await page.evaluate(()=>window.atvTerrainStatus.wheelEffects.frame),paused);
   await page.getByRole('button',{name:'Resume',exact:true}).click();await page.waitForFunction(t=>window.atvTerrainStatus.wheelEffects?.frame?.time>t,paused.time);
   await page.evaluate(()=>{window.loseWheel=document.querySelector('#game').getContext('webgl2').getExtension('WEBGL_lose_context');window.loseWheel.loseContext();});await page.waitForFunction(()=>!window.atvTerrainStatus.ready);await page.waitForTimeout(100);await page.evaluate(()=>window.loseWheel.restoreContext());await page.waitForFunction(()=>window.atvTerrainStatus.ready);await page.waitForFunction(t=>window.atvTerrainStatus.wheelEffects?.frame?.time>t,paused.time);
   const trace=await page.evaluate(()=>window.wheelEffectTrace.map(f=>({time:f.wheel_particles.time,emitted:f.wheel_particles.emitted,centers:f.wheel_effects.centers,terrain:f.wheel_effects.terrain_heights,water:f.wheel_effects.water_heights})));assert(trace.length>50);
   for(const frame of trace){if(frame.emitted[0]||frame.emitted[1]||frame.emitted[2])assert([2,3].some(i=>frame.centers[i][1]-frame.terrain[i]<=1.3001));}
   assert.equal(await page.evaluate(()=>document.querySelector('#game').getContext('webgl2').getError()),0);assert.deepEqual(errors,[]);
   console.log(JSON.stringify({mobile,pixels:await page.evaluate(()=>window.wheelPixelEvidence),dry:first.frame.total_emitted,turning:roost.frame.total_emitted,gpuBytes:roost.gpuBytes,cpuMilliseconds:roost.cpuMilliseconds,frames:trace.length}));
  }finally{await page.close();}
 }
 console.log('PASS: original normal riding dust and steering dirt chunks/spray change pixels, pause/resume and restore at desktop/390px. Wet/mud and exact GS parity are separate gates.');
}finally{await browser.close();}
