// Runtime surface-effect gate: original water terrain or a private audited COL point.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {assertNoErrors,base,button,launch,newPage,waitForGame} from './browser/harness.mjs';
const mud=process.env.ATV_WHEEL_MUD_FIXTURE?JSON.parse(await readFile(process.env.ATV_WHEEL_MUD_FIXTURE,'utf8')):null;
const browser=await launch();
try{for(const mobile of[false,true]){
 const errors=[],page=await newPage(browser,{viewport:mobile?{width:390,height:844}:{width:960,height:600}},errors);
 await page.addInitScript(({mud})=>{
  const NativeWorker=Worker;window.surfaceFrames=[];
  window.Worker=class extends NativeWorker{
   constructor(...args){super(...args);this.ids=new Map();this.addEventListener('message',({data})=>{const op=this.ids.get(data.id);this.ids.delete(data.id);if(data.error||!data.bytes)return;const parsed=()=>JSON.parse(new TextDecoder().decode(data.bytes));if(op===9)window.surfaceInfo=parsed();if(op===10)window.surfaceHeights=new Float32Array(data.bytes.buffer,data.bytes.byteOffset,data.bytes.byteLength/4);if(op===64)window.surfaceBase=parsed().base_height;if(op===50)for(const row of parsed())window.surfaceFrames.push(row.frame);});}
   postMessage(data,...args){
    if(data.op===23){const start=JSON.parse(new TextDecoder().decode(data.bytes));let point=mud?.position;
     if(!point&&Number.isFinite(window.surfaceBase)){const info=window.surfaceInfo,heights=window.surfaceHeights,width=info.width_cells+1,base=window.surfaceBase;let best;
      for(let z=32;z<info.depth_cells-32;z+=2)for(let x=32;x<info.width_cells-32;x+=2){const h=heights[z*width+x];if(h>=base-0.05||h<base-0.8)continue;
       const neighbors=[[-2,0],[2,0],[0,-2],[0,2]].map(([a,b])=>heights[(z+b)*width+x+a]);if(neighbors.some(v=>v>=base-0.05||v<base-1.2))continue;
       const position=[x*info.spacing,h+2.5,z*info.spacing],distance=(position[0]-start.origin[0])**2+(position[2]-start.origin[2])**2;if(!best||distance<best.distance)best={position,distance,terrainHeight:h,base};}
      if(!best)throw new Error('No original shallow wet terrain was found.');point=best.position;window.surfacePlacement=best;
     }
     if(point){window.surfacePlacement??={position:point,name:mud.name};data={...data,bytes:new TextEncoder().encode(JSON.stringify({...start,origin:point}))};}
    }
    this.ids.set(data.id,data.op);super.postMessage(data,...args);
   }
  };
  const draw=WebGL2RenderingContext.prototype.drawArraysInstanced;
  WebGL2RenderingContext.prototype.drawArraysInstanced=function(...args){const program=this.getParameter(this.CURRENT_PROGRAM);if(!window.captureWetPixels||!program)return draw.apply(this,args);const wet=this.getUniformLocation(program,'wet');if(!wet||!this.getUniform(program,wet))return draw.apply(this,args);
   const before=new Uint8Array(this.canvas.width*this.canvas.height*4),after=new Uint8Array(before.length);this.readPixels(0,0,this.canvas.width,this.canvas.height,this.RGBA,this.UNSIGNED_BYTE,before);draw.apply(this,args);this.readPixels(0,0,this.canvas.width,this.canvas.height,this.RGBA,this.UNSIGNED_BYTE,after);let changed=0;for(let i=0;i<before.length;i+=4)if([0,1,2].some(c=>before[i+c]!==after[i+c]))changed++;if(changed){window.wetPixelEvidence={changed,glError:this.getError()};window.captureWetPixels=false;}
  };
 },{mud});
 try{
  const environment=mud?.environment??'env04',kind=mud?'mud':'water',index=mud?1:0;
  await page.goto(`${base}/play.html?drive=1&environment=${environment}`);await waitForGame(page);
  await page.evaluate(()=>window.captureWetPixels=true);await page.keyboard.down('KeyW');await page.waitForFunction(i=>window.atvTerrainStatus.wheelEffects?.frame?.wet_total_emitted[i]>0,index,{timeout:40000});await page.waitForFunction(()=>window.wetPixelEvidence,null,{timeout:30000});await page.keyboard.up('KeyW');
  const visible=await page.evaluate(()=>structuredClone(window.atvTerrainStatus.wheelEffects));assert(visible.draws<=2);assert(visible.gpuBytes<1.2*1024*1024);assert(visible.frame.wet_total_emitted[index]>0);
  if(process.env.ATV_WHEEL_WET_SCREENSHOTS)await page.screenshot({path:`${process.env.ATV_WHEEL_WET_SCREENSHOTS}-${kind}${mobile?'-mobile':''}.png`});
  // Time-based on purpose: let any in-flight step settle, then prove the paused effect frame does not change.
  await page.keyboard.press('Escape');await button(page,'Resume').waitFor();await page.waitForTimeout(100);const paused=await page.evaluate(()=>window.atvTerrainStatus.wheelEffects.frame);await page.waitForTimeout(250);assert.deepEqual(await page.evaluate(()=>window.atvTerrainStatus.wheelEffects.frame),paused);await button(page,'Resume').click();await page.waitForFunction(t=>window.atvTerrainStatus.wheelEffects?.frame?.time>t,paused.time);
  // Kept: brief pause after context loss before restoring; no observable readiness signal.
  await page.evaluate(()=>{window.surfaceLost=document.querySelector('#game').getContext('webgl2').getExtension('WEBGL_lose_context');window.surfaceLost.loseContext();});await page.waitForFunction(()=>!window.atvTerrainStatus.ready);await page.waitForTimeout(100);await page.evaluate(()=>window.surfaceLost.restoreContext());await page.waitForFunction(()=>window.atvTerrainStatus.ready);await page.waitForFunction(t=>window.atvTerrainStatus.wheelEffects?.frame?.time>t,paused.time);
  const frames=await page.evaluate(()=>window.surfaceFrames),births=frames.flatMap(f=>f.wheel_particles?.wet_births??[]).filter(p=>p.kind===kind);assert(births.length>0);assert(births.every(p=>mud?p.tile>=8&&p.tile<16:p.tile>=0&&p.tile<8));
  if(mud)assert(frames.some(f=>f.wheel_effects.mud.some(Boolean)));else assert(frames.some(f=>f.wheel_effects.water_heights.some((h,i)=>h>f.wheel_effects.terrain_heights[i])));
  assert.equal(await page.evaluate(()=>document.querySelector('#game').getContext('webgl2').getError()),0);assertNoErrors(errors);console.log(JSON.stringify({kind,mobile,placement:await page.evaluate(()=>window.surfacePlacement),pixels:await page.evaluate(()=>window.wetPixelEvidence),births:births.length,draws:visible.draws,gpuBytes:visible.gpuBytes,cpuMilliseconds:visible.cpuMilliseconds}));
 }catch(error){console.log(JSON.stringify(await page.evaluate(()=>({placement:window.surfacePlacement,frame:window.atvTerrainStatus?.frame,fx:window.atvTerrainStatus?.wheelEffects,mudHits:window.surfaceFrames.filter(f=>f.wheel_effects.mud.some(Boolean)).length,maxTangent:Math.max(0,...window.surfaceFrames.map(f=>Math.abs(f.wheel_effects.wheel_tangential_speeds[0])))}))));throw error;}finally{await page.close();}
}console.log('PASS: original wet particle atlas changes pixels from authoritative tire surface hits at desktop/390px; fixed clock pauses and context restoration preserve the ring.');}finally{await browser.close();}
