// Original-resource rain gate. Optional reference URL verifies identical pixels.
// Reference and candidate are separate authored builds; source assets stay in R2.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
const{chromium}=createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE||'playwright');
const base=process.env.ATV_TEST_URL||'http://127.0.0.1:4173',reference=process.env.ATV_WEATHER_REFERENCE_URL;
const browser=await chromium.launch({headless:true});
async function capture(url,mobile){
 const page=await browser.newPage({viewport:mobile?{width:390,height:844}:{width:960,height:540}}),errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.addInitScript(()=>{
  const context=HTMLCanvasElement.prototype.getContext;HTMLCanvasElement.prototype.getContext=function(kind,options){return context.call(this,kind,kind==='webgl2'?{...options,preserveDrawingBuffer:true}:options);};
  const NativeWorker=Worker;
  window.Worker=class extends NativeWorker{
   constructor(...args){super(...args);this.steps=0;this.ops=new Map();this.addEventListener('message',({data})=>{const op=this.ops.get(data.id);this.ops.delete(data.id);if(op===60&&data.bytes&&!data.error){const scene=JSON.parse(new TextDecoder().decode(data.bytes)),models=scene.models.filter(model=>/rain$/i.test(model.name));if(models.length){const center=[0,0];for(const model of models){center[0]+=model.world_matrix[12]/models.length;center[1]+=model.world_matrix[14]/models.length;}window.weatherInspectionModel=models.reduce((best,m)=>{const distance=(m.world_matrix[12]-center[0])**2+(m.world_matrix[14]-center[1])**2;return !best||distance<best.distance?{matrix:m.world_matrix,distance}:best;},null);}}});}
   postMessage(data,...args){this.ops.set(data.id,data.op);if(data.op===23&&window.weatherInspectionModel){const request=JSON.parse(new TextDecoder().decode(data.bytes)),p=window.weatherInspectionModel.matrix.slice(12,15);request.origin=[p[0],p[1]+100,p[2]];request.yaw=0;data={...data,bytes:new TextEncoder().encode(JSON.stringify(request))};}if(data.op===50){if(this.steps>=90)return;const request=JSON.parse(new TextDecoder().decode(data.bytes));request.count=1;request.now=(++this.steps)*20;data={...data,bytes:new TextEncoder().encode(JSON.stringify(request))};}return super.postMessage(data,...args);}
  };
 });
 await page.goto(`${url}/play.html?drive=1&environment=env04`);
 await page.waitForFunction(()=>window.atvTerrainStatus?.ready||document.querySelector('#status.error'),null,{timeout:120000});assert((await page.evaluate(()=>window.atvTerrainStatus)).ready,await page.locator('#status').textContent());
 await page.waitForFunction(()=>window.atvTerrainStatus.simulationSteps>=90,null,{timeout:120000});
 const result=await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>{const state=window.atvTerrainStatus,canvas=document.querySelector('#game'),gl=canvas.getContext('webgl2'),pixels=new Uint8Array(canvas.width*canvas.height*4);gl.readPixels(0,0,canvas.width,canvas.height,gl.RGBA,gl.UNSIGNED_BYTE,pixels);resolve({state,pixels:Array.from(pixels),error:gl.getError()});})));
 assert.equal(result.error,0);assert.equal(result.state.glError,0);assert.deepEqual(errors,[]);await page.close();return result;
}
try{
 for(const mobile of [false,true]){
  const candidate=await capture(base,mobile),state=candidate.state;
  assert(state.weather.active);assert.equal(state.weather.definition.kind,'rain');assert.equal(state.weather.effective_chance,1);assert(state.rainRender.sourceInstances>1);assert(state.rainRender.avoidedMaterialUpdates>0);
  let mismatches=null;
  if(reference){const before=await capture(reference,mobile);assert.deepEqual(before.state.frame.origin,state.frame.origin);assert.deepEqual(before.state.camera,state.camera);assert.deepEqual(before.state.sceneryUv,state.sceneryUv);assert.equal(before.pixels.length,candidate.pixels.length);mismatches=0;for(let i=0;i<before.pixels.length;i++)if(before.pixels[i]!==candidate.pixels[i])mismatches++;assert.equal(mismatches,0,'Cached rain material state must preserve every framebuffer byte');}
  console.log(JSON.stringify({mobile,weather:state.weather,rain:state.rainRender,pixelBytes:candidate.pixels.length,mismatches}));
 }
}finally{await browser.close();}
