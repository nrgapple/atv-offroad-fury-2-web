// Private-content scene fog gate and actual-renderer GPU coverage/interpolation.
import assert from 'node:assert/strict';
import {assertNoErrors,base,launch,nativeData,newPage,waitForGame} from './browser/harness.mjs';
const native=await nativeData('ATV_FOG_NATIVE','native fog coverage samples (GPU coverage comparison)');
const browser=await launch();
try {for(const environment of ['env01','env04','free05']) {
 const errors=[],page=await newPage(browser,{viewport:{width:960,height:600}},errors);
 await page.addInitScript(()=>{
  window.fogPasses=[];window.fogDraws={enabled:0,disabled:0};
  const NativeWorker=Worker;
  window.Worker=class extends NativeWorker {
   constructor(...args){super(...args);this.ops=new Map();this.addEventListener('message',({data})=>{const op=this.ops.get(data.id);this.ops.delete(data.id);if(op===90&&data.bytes&&!data.error)fogPasses.push(JSON.parse(new TextDecoder().decode(data.bytes)));});}
   postMessage(data,...args){this.ops.set(data.id,data.op);super.postMessage(data,...args);}
  };
  const source=WebGL2RenderingContext.prototype.shaderSource;
  WebGL2RenderingContext.prototype.shaderSource=function(shader,text){if(text.includes('sceneryReflectionPlane'))window.fogVertexSource=text;if(text.includes('uniform bool sceneryBlend'))window.fogFragmentSource=text;return source.call(this,shader,text);};
  const draw=WebGL2RenderingContext.prototype.drawElements;
  WebGL2RenderingContext.prototype.drawElements=function(...args){const p=this.getParameter(this.CURRENT_PROGRAM);if(p&&!this.isContextLost()){const l=this.getUniformLocation(p,'fogEnabled');if(l)fogDraws[this.getUniform(p,l)?'enabled':'disabled']++;}return draw.apply(this,args);};
 });
 await page.goto(`${base}/play.html?drive=1&environment=${environment}`);
 await waitForGame(page);
 await page.waitForFunction(()=>fogDraws.enabled>0);
 const state=await page.evaluate(()=>({fog:atvTerrainStatus.fog,passes:fogPasses,draws:fogDraws}));
 assert.equal(state.fog.end,768);assert.equal(state.fog.start,environment==='free05'?352:0);
 assert(state.passes.some(p=>p.fog));
 let gpu=null;
 if(native&&environment==='env01')gpu=await page.evaluate(report=>{
  const gl=document.querySelector('#game').getContext('webgl2'),prior={program:gl.getParameter(gl.CURRENT_PROGRAM),vao:gl.getParameter(gl.VERTEX_ARRAY_BINDING),viewport:gl.getParameter(gl.VIEWPORT),framebuffer:gl.getParameter(gl.FRAMEBUFFER_BINDING),depth:gl.isEnabled(gl.DEPTH_TEST),blend:gl.isEnabled(gl.BLEND),cull:gl.isEnabled(gl.CULL_FACE)};
  const compile=(type,text)=>{const s=gl.createShader(type);gl.shaderSource(s,text);gl.compileShader(s);if(!gl.getShaderParameter(s,gl.COMPILE_STATUS))throw Error(gl.getShaderInfoLog(s));return s;};
  const v=compile(gl.VERTEX_SHADER,fogVertexSource),f=compile(gl.FRAGMENT_SHADER,fogFragmentSource),p=gl.createProgram();gl.attachShader(p,v);gl.attachShader(p,f);gl.transformFeedbackVaryings(p,['fogInterpolation'],gl.INTERLEAVED_ATTRIBS);gl.linkProgram(p);if(!gl.getProgramParameter(p,gl.LINK_STATUS))throw Error(gl.getProgramInfoLog(p));
  const location=name=>gl.getUniformLocation(p,name),identity=[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1],vao=gl.createVertexArray(),feedback=gl.createTransformFeedback(),buffer=gl.createBuffer();
  gl.bindVertexArray(vao);gl.useProgram(p);gl.uniformMatrix4fv(location('model'),false,identity);gl.uniformMatrix4fv(location('viewProjection'),false,identity);gl.uniform3f(location('eye'),0,0,0);gl.uniform1i(location('fogEnabled'),true);
  gl.vertexAttrib3f(1,0,1,0);gl.uniform3f(location('sunlightDirection'),0,1,0);
  for(const [name,unit] of [['baseTexture',0],['terrainColors',1],['terrainMapping',2],['terrainDetail',3]])gl.uniform1i(location(name),unit);
  gl.bindTransformFeedback(gl.TRANSFORM_FEEDBACK,feedback);gl.bindBuffer(gl.TRANSFORM_FEEDBACK_BUFFER,buffer);gl.bufferData(gl.TRANSFORM_FEEDBACK_BUFFER,24,gl.DYNAMIC_READ);gl.bindBufferBase(gl.TRANSFORM_FEEDBACK_BUFFER,0,buffer);gl.enable(gl.RASTERIZER_DISCARD);
  const f32bits=bits=>new Float32Array(new Uint32Array([bits]).buffer)[0],output=new Float32Array(2);let count=0,maximumByteDifference=0;
  for(const c of report.cases){const coefficient=f32bits(c.coefficient),bias=Math.fround(524288-Math.fround(f32bits(c.end)*coefficient));gl.uniform2f(location('fogPacking'),bias,coefficient);
   for(const sample of c.samples){gl.vertexAttrib3f(0,f32bits(sample.distance),0,0);gl.beginTransformFeedback(gl.POINTS);gl.drawArrays(gl.POINTS,0,1);gl.endTransformFeedback();gl.getBufferSubData(gl.TRANSFORM_FEEDBACK_BUFFER,0,output);const byte=Math.round(output[0]/output[1]*255),difference=Math.abs(byte-sample.coverage);if(!Number.isFinite(difference)||difference>1)throw Error(`Native/GPU fog coverage mismatch ${JSON.stringify({sample,byte})}`);maximumByteDifference=Math.max(maximumByteDifference,difference);count++;}
  }
  gl.uniform1i(location('fogEnabled'),false);gl.vertexAttrib3f(0,50000,0,0);gl.beginTransformFeedback(gl.POINTS);gl.drawArrays(gl.POINTS,0,1);gl.endTransformFeedback();gl.getBufferSubData(gl.TRANSFORM_FEEDBACK_BUFFER,0,output);if(output[0]/output[1]!==1)throw Error('Fog-disabled pass still fades');
  // Deliberately unequal clip W catches accidental perspective fog interpolation.
  const projection=identity.slice();projection[11]=1;const points=new Float32Array([-.8,-.8,0,1.6,-1.6,1,0,3.2,3]),positions=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,positions);gl.bufferData(gl.ARRAY_BUFFER,points,gl.STATIC_DRAW);gl.enableVertexAttribArray(0);gl.vertexAttribPointer(0,3,gl.FLOAT,false,12,0);gl.uniformMatrix4fv(location('viewProjection'),false,projection);gl.uniform2f(location('fogPacking'),524543,-80);gl.uniform1i(location('fogEnabled'),true);
  gl.beginTransformFeedback(gl.POINTS);gl.drawArrays(gl.POINTS,0,3);gl.endTransformFeedback();const vertices=new Float32Array(6);gl.getBufferSubData(gl.TRANSFORM_FEEDBACK_BUFFER,0,vertices);gl.disable(gl.RASTERIZER_DISCARD);
  const texture=gl.createTexture(),framebuffer=gl.createFramebuffer();gl.bindTexture(gl.TEXTURE_2D,texture);gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,1,1,0,gl.RGBA,gl.UNSIGNED_BYTE,null);gl.bindFramebuffer(gl.FRAMEBUFFER,framebuffer);gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0);gl.bindTexture(gl.TEXTURE_2D,null);if(gl.checkFramebufferStatus(gl.FRAMEBUFFER)!==gl.FRAMEBUFFER_COMPLETE)throw Error('Fog test framebuffer incomplete');gl.viewport(0,0,1,1);gl.disable(gl.DEPTH_TEST);gl.disable(gl.BLEND);gl.disable(gl.CULL_FACE);gl.uniform3f(location('surfaceColor'),1,1,1);gl.uniform3f(location('fogColor'),0,0,0);gl.uniform1i(location('unlit'),true);gl.clearColor(0,0,0,1);gl.clear(gl.COLOR_BUFFER_BIT);gl.drawArrays(gl.TRIANGLES,0,3);const pixel=new Uint8Array(4);gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
  const weights=[.25,.25,.5],coverage=weights.map((_,i)=>vertices[i*2]/vertices[i*2+1]),affine=weights.reduce((s,w,i)=>s+w*coverage[i],0)*255,perspective=weights.reduce((s,w,i)=>s+w*coverage[i]/vertices[i*2+1],0)/weights.reduce((s,w,i)=>s+w/vertices[i*2+1],0)*255;
  if(Math.abs(pixel[0]-affine)>2||Math.abs(pixel[0]-perspective)<10)throw Error(`Fog is not screen-linear: ${JSON.stringify({pixel:[...pixel],affine,perspective,error:gl.getError(),unlit:gl.getUniform(p,location('unlit')),surface:[...gl.getUniform(p,location('surfaceColor'))],fog:gl.getUniform(p,location('fogEnabled')),texture:gl.getUniform(p,location('useTexture')),mask:gl.getParameter(gl.COLOR_WRITEMASK)})}`);
  gl.bindFramebuffer(gl.FRAMEBUFFER,prior.framebuffer);gl.viewport(...prior.viewport);gl.bindTransformFeedback(gl.TRANSFORM_FEEDBACK,null);gl.bindBufferBase(gl.TRANSFORM_FEEDBACK_BUFFER,0,null);gl.bindVertexArray(prior.vao);gl.useProgram(prior.program);if(prior.depth)gl.enable(gl.DEPTH_TEST);if(prior.blend)gl.enable(gl.BLEND);if(prior.cull)gl.enable(gl.CULL_FACE);gl.deleteFramebuffer(framebuffer);gl.deleteTexture(texture);gl.deleteBuffer(positions);gl.deleteBuffer(buffer);gl.deleteTransformFeedback(feedback);gl.deleteVertexArray(vao);gl.deleteProgram(p);gl.deleteShader(v);gl.deleteShader(f);
  return {coverageSamples:count,maximumByteDifference,affineExpected:affine,perspectiveExpected:perspective,actual:pixel[0]};
 },native);
 assertNoErrors(errors);assert.equal(await page.evaluate(()=>document.querySelector('#game').getContext('webgl2').getError()),0);
 if(process.env.ATV_FOG_SCREENSHOTS)await page.screenshot({path:`${process.env.ATV_FOG_SCREENSHOTS}-${environment}.png`});
 console.log(JSON.stringify({environment,fog:state.fog,draws:state.draws,gpu}));await page.close();
}console.log('PASS: original scene fog range, mixed material switches, GPU coverage and screen-linear interpolation; full hardware/terrain/water parity remains open.');}finally{await browser.close();}
