// Original-content near-vegetation packet/state gate. Distant vegetation has a separate gate.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import fs from 'node:fs/promises';
const native=process.env.ATV_ECO_NATIVE?JSON.parse(await fs.readFile(process.env.ATV_ECO_NATIVE,'utf8')):null;
const {chromium}=createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE||'playwright');
const base=process.env.ATV_TEST_URL||'http://127.0.0.1:4290';
const browser=await chromium.launch({headless:true});
try {for(const environment of (process.env.ATV_ECO_MAPS||'env01,env04,free05').split(',')) {
 const page=await browser.newPage({viewport:{width:960,height:600}}),errors=[];
 page.on('pageerror',error=>errors.push(error.message));
 // The recovered LOD leaves ENV07's starting near queue outside the view.
 // Inspect the nearest authored fern with a test-only frozen camera.
 if(environment==='env07')await page.route('**/game.js',async route=>{const response=await route.fetch();await route.fulfill({response,body:(await response.text())+`\nwindow.inspectNearVegetation=()=>{const slot=terrain.ecosystem.species[0].slot,p=renderCore.call(143,new Uint8Array()).placements.filter(p=>p.slot===slot).reduce((best,p)=>Math.hypot(p.position[0]-camera.x,p.position[2]-camera.z)<Math.hypot(best.position[0]-camera.x,best.position[2]-camera.z)?p:best);menuOpen=false;focused=false;cameraFrame=null;$('frontend').hidden=true;camera.x=p.position[0]+10;camera.z=p.position[2]+10;camera.y=p.position[1]+p.radius*1.5;camera.yaw=Math.atan2(-10,-10);camera.pitch=Math.atan2(-p.radius,Math.sqrt(200));};`});});
 await page.addInitScript(()=>{
  window.ecoMaterialNames=[];window.ecoPasses={};window.ecoDraws=0;window.ecoPixels=0;window.ecoVehicleDraws=0;
  const NativeWorker=Worker;
  window.Worker=class extends NativeWorker {
   constructor(...args){super(...args);this.ops=new Map();this.addEventListener('message',({data})=>{
    const input=this.ops.get(data.id);this.ops.delete(data.id);if(data.error||!data.bytes||![59,60,145].includes(input?.op))return;
    const value=JSON.parse(new TextDecoder().decode(data.bytes));
    if(input?.op===59)ecoMaterialNames.push(...value.mesh.materials);
    if(input?.op===60)window.ecoScene=value;
    // Rust material passes report their base binding with the parsed pass.
    if(input?.op===145)for(const described of value)if(described.binding&&described.pass)ecoPasses[described.binding.toLowerCase()]=described.pass;
   });}
   postMessage(data,...args){this.ops.set(data.id,{op:data.op});super.postMessage(data,...args);}
  };
  const prototype=WebGL2RenderingContext.prototype,draw=prototype.drawElementsInstanced,vehicle=prototype.drawElements,shader=prototype.shaderSource;
  prototype.shaderSource=function(s,text){if(text.includes('ecoSunDirection'))window.ecoVertexSource=text;return shader.call(this,s,text);};
  prototype.drawElementsInstanced=function(...args){
   const program=this.getParameter(this.CURRENT_PROGRAM),uniform=name=>{const location=this.getUniformLocation(program,name);return location&&this.getUniform(program,location);};
   if(!uniform('useEcosystem')||uniform('ecoBillboard'))return draw.apply(this,args);
   const names=[...new Set(ecoMaterialNames)];if(names.length!==1)throw Error('Vegetation gate requires a single source material family per inspected map');
   const pass=ecoPasses[names[0].toLowerCase()];if(!pass)throw Error('Source vegetation material metadata unavailable');
   if(!uniform('useVertexColor')||!uniform('unlit'))throw Error('Original near vegetation color is not connected');
   const lighting=ecoScene.ecosystem_lighting;
   for(const [name,key]of [['ecoAmbient','ambient'],['ecoSunlight','sunlight'],['ecoSunDirection','direction']])if(![...uniform(name)].every((v,i)=>v===Math.fround(lighting[key][i])))throw Error('Vegetation Sun matrix differs from Rust preparation');
   if(Boolean(uniform('ecoSunPresent'))!==lighting.sun_present)throw Error('Sun enable differs');
   if(Boolean(uniform('sceneryBlend'))!==pass.blend||this.isEnabled(this.BLEND)!==pass.blend||Boolean(uniform('fogEnabled'))!==Boolean(ecoScene.fog&&pass.fog))throw Error('Vegetation source blend/fog switches differ');
   if(this.getParameter(this.DEPTH_WRITEMASK)!==(uniform('alphaFailPass')?false:pass.depth_write))throw Error('Vegetation source depth write switch differs');
   ecoDraws++;
   if(!window.captureEco)return draw.apply(this,args);
   const before=new Uint8Array(this.canvas.width*this.canvas.height*4),after=new Uint8Array(before.length);
   this.readPixels(0,0,this.canvas.width,this.canvas.height,this.RGBA,this.UNSIGNED_BYTE,before);draw.apply(this,args);this.readPixels(0,0,this.canvas.width,this.canvas.height,this.RGBA,this.UNSIGNED_BYTE,after);
   let pixels=0;for(let i=0;i<before.length;i+=4)if(before[i]!==after[i]||before[i+1]!==after[i+1]||before[i+2]!==after[i+2])pixels++;ecoPixels=Math.max(ecoPixels,pixels);
  };
  prototype.drawElements=function(...args){const p=this.getParameter(this.CURRENT_PROGRAM);if(p){const value=name=>{const location=this.getUniformLocation(p,name);return location&&this.getUniform(p,location);};if(value('useQuadPose')&&!value('useEcosystem')){if(value('useVertexColor')||value('sceneryBlend'))throw Error('Vegetation state leaked to a vehicle or rider');ecoVehicleDraws++;}}return vehicle.apply(this,args);};
 });
 await page.goto(`${base}/play.html?drive=1&environment=${environment}`);
 await page.waitForFunction(()=>window.atvTerrainStatus?.ready&&window.atvTerrainStatus?.simulation,null,{timeout:120000});
 assert.match(await page.evaluate(()=>window.atvTerrainStatus.source),new RegExp(`${environment}\\.tr2$`),'Requested vegetation map must load');
 await page.evaluate(()=>window.captureEco=true);
 if(environment==='env07'){await page.keyboard.press('Escape');await page.waitForTimeout(250);await page.evaluate(()=>inspectNearVegetation());}
 await page.waitForFunction(()=>ecoPixels>0&&ecoVehicleDraws>0,null,{timeout:15000});
 await page.evaluate(()=>window.captureEco=false);
 const result=await page.evaluate(()=>({environment:atvTerrainStatus.source,lighting:atvTerrainStatus.vegetationLighting,draws:ecoDraws,vehicleDraws:ecoVehicleDraws,pixels:ecoPixels,gl:atvTerrainStatus.glError}));
 if(native&&environment==='env01')result.native=await page.evaluate(cases=>{
  const gl=document.querySelector('#game').getContext('webgl2'),prior=gl.getParameter(gl.CURRENT_PROGRAM),priorVao=gl.getParameter(gl.VERTEX_ARRAY_BINDING);
  const compile=(type,text)=>{const s=gl.createShader(type);gl.shaderSource(s,text);gl.compileShader(s);if(!gl.getShaderParameter(s,gl.COMPILE_STATUS))throw Error(gl.getShaderInfoLog(s));return s;};
  const vertex=compile(gl.VERTEX_SHADER,ecoVertexSource),fragment=compile(gl.FRAGMENT_SHADER,'#version 300 es\nprecision highp float;out vec4 color;void main(){color=vec4(1.0);}');
  const p=gl.createProgram();gl.attachShader(p,vertex);gl.attachShader(p,fragment);gl.transformFeedbackVaryings(p,['authoredColor'],gl.INTERLEAVED_ATTRIBS);gl.linkProgram(p);if(!gl.getProgramParameter(p,gl.LINK_STATUS))throw Error(gl.getProgramInfoLog(p));
  const vao=gl.createVertexArray(),buffer=gl.createBuffer(),feedback=gl.createTransformFeedback(),identity=[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1],out=new Float32Array(4);
  gl.bindVertexArray(vao);gl.bindTransformFeedback(gl.TRANSFORM_FEEDBACK,feedback);gl.bindBuffer(gl.TRANSFORM_FEEDBACK_BUFFER,buffer);gl.bufferData(gl.TRANSFORM_FEEDBACK_BUFFER,16,gl.DYNAMIC_READ);gl.bindBufferBase(gl.TRANSFORM_FEEDBACK_BUFFER,0,buffer);gl.useProgram(p);
  gl.uniformMatrix4fv(gl.getUniformLocation(p,'model'),false,identity);gl.uniformMatrix4fv(gl.getUniformLocation(p,'viewProjection'),false,identity);gl.uniform1i(gl.getUniformLocation(p,'useEcosystem'),true);gl.uniform1i(gl.getUniformLocation(p,'ecoSunPresent'),true);
  gl.vertexAttrib3f(0,0,0,0);gl.vertexAttrib3f(4,0,0,0);gl.enable(gl.RASTERIZER_DISCARD);let maxError=0,differingBytes=0;
  for(const item of cases){gl.vertexAttrib3fv(1,item.normal);gl.vertexAttrib2fv(5,item.scale);gl.uniform2fv(gl.getUniformLocation(p,'ecoRotation'),item.rotation);gl.uniform3fv(gl.getUniformLocation(p,'ecoAmbient'),item.ambient.map(v=>Math.fround(v*.5)));gl.uniform3fv(gl.getUniformLocation(p,'ecoSunlight'),item.sunlight.map(v=>Math.fround(v*.5)));gl.uniform3fv(gl.getUniformLocation(p,'ecoSunDirection'),item.preparedDirection);
   gl.beginTransformFeedback(gl.POINTS);gl.drawArrays(gl.POINTS,0,1);gl.endTransformFeedback();gl.getBufferSubData(gl.TRANSFORM_FEEDBACK_BUFFER,0,out);
   for(let i=0;i<4;i++){const error=Math.abs(out[i]-item.color[i]);if(!Number.isFinite(error)||error>1)throw Error(`Native vegetation color mismatch ${JSON.stringify({item,out:[...out]})}`);maxError=Math.max(maxError,error);if(error)differingBytes++;}
  }
  gl.disable(gl.RASTERIZER_DISCARD);gl.bindTransformFeedback(gl.TRANSFORM_FEEDBACK,null);gl.bindBufferBase(gl.TRANSFORM_FEEDBACK_BUFFER,0,null);gl.bindVertexArray(priorVao);gl.useProgram(prior);gl.deleteTransformFeedback(feedback);gl.deleteBuffer(buffer);gl.deleteVertexArray(vao);gl.deleteProgram(p);gl.deleteShader(vertex);gl.deleteShader(fragment);return{cases:cases.length,maxError,differingBytes};
 },native.cases);
 assert(result.draws>0&&result.vehicleDraws>0&&result.pixels>0);assert.equal(result.gl,0);assert.deepEqual(errors,[]);console.log(JSON.stringify(result));
 if(process.env.ATV_ECO_SCREENSHOTS)await page.screenshot({path:`${process.env.ATV_ECO_SCREENSHOTS}-${environment}.png`});
 await page.close();
 }console.log('PASS: near vegetation Sun/ambient preparation, source blend/fog/depth switches, visible vegetation and vehicle-state isolation; optional original arithmetic comparison. Full native visibility/GS hardware parity remain open.');
}finally{await browser.close();}
