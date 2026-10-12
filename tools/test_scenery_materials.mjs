// Original-content inspection views: no game fixtures or camera changes ship.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import fs from 'node:fs/promises';
const {chromium}=createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE||'playwright');
const base=process.env.ATV_TEST_URL||'http://127.0.0.1:4173';
const browser=await chromium.launch({headless:true});
const results=[];
try {
  for(const [environment,model,material,velocity] of [['env04','sx13rain','rain',2],['env07','lava','lava',0],['env08','waterduct1','waterflow',.983],['free05','skijump','skijump',0]]) {
    const page=await browser.newPage({viewport:{width:960,height:600}}),errors=[];
    page.on('pageerror',e=>errors.push(e.message));
    await page.addInitScript(model => {
      window.sceneryModelName = model; window.sourceColors = new Set(['[128,128,128,128]']); window.checkedColorVaos = new WeakSet(); window.materialDraws = 0; window.materialPixels = 0; window.uvPixels = 0;
      const NativeWorker = Worker;
      window.Worker = class extends NativeWorker {
        constructor(...args) {
          super(...args); this.ops = new Map();
          this.addEventListener('message', ({ data }) => {
            const op = this.ops.get(data.id); this.ops.delete(data.id);
            if (op === 11 && data.bytes && !data.error) {
              const mesh = JSON.parse(new TextDecoder().decode(data.bytes));
              for (const group of mesh.lods[0].groups) for (const vertex of group.vertices) if (vertex.color) window.sourceColors.add(JSON.stringify(vertex.color));
            }
            if (op === 60 && data.bytes && !data.error) {
              const scene = JSON.parse(new TextDecoder().decode(data.bytes));
              window.sceneryModels = scene.models.filter(m => m.name.toLowerCase() === window.sceneryModelName);
            }
          });
        }
        postMessage(data, ...args) { this.ops.set(data.id, data.op); super.postMessage(data, ...args); }
      };
      const names = new WeakMap(), getLocation = WebGL2RenderingContext.prototype.getUniformLocation;
      WebGL2RenderingContext.prototype.getUniformLocation = function (...args) { const location=getLocation.apply(this,args); if(location) names.set(location,args[1]); return location; };
      const matrix = WebGL2RenderingContext.prototype.uniformMatrix4fv;
      WebGL2RenderingContext.prototype.uniformMatrix4fv = function (location, transpose, value) {
        const selected = window.sceneryModels?.[0];
        const p = this.getParameter(this.CURRENT_PROGRAM);
        if (selected && names.get(location) === 'viewProjection') {
          const center = selected.world_matrix.slice(12, 15), eye = [center[0], center[1] + 60, center[2] - 90];
          const forward = [0, -60 / Math.hypot(60, 90), 90 / Math.hypot(60, 90)];
          const right = [-1, 0, 0], up = [0, forward[2], -forward[1]];
          const view = [right[0], up[0], -forward[0], 0, right[1], up[1], -forward[1], 0, right[2], up[2], -forward[2], 0,
            -right.reduce((s,v,i) => s + v * eye[i], 0), -up.reduce((s,v,i) => s + v * eye[i], 0), forward.reduce((s,v,i) => s + v * eye[i], 0), 1];
          const halfWidth = 100, halfHeight = halfWidth * this.canvas.height / this.canvas.width;
          const projection = [1/halfWidth,0,0,0,0,1/halfHeight,0,0,0,0,-2/1999,0,0,0,-2001/1999,1];
          value = new Float32Array(16);
          for (let c=0;c<4;c++) for (let r=0;r<4;r++) for (let k=0;k<4;k++) value[c*4+r] += projection[k*4+r] * view[c*4+k];
          this.uniform3fv(this.getUniformLocation(p, 'eye'), eye);
        }
        return matrix.call(this, location, transpose, value);
      };
      const draw = WebGL2RenderingContext.prototype.drawElements;
      WebGL2RenderingContext.prototype.drawElements = function (...args) {
        const p = this.getParameter(this.CURRENT_PROGRAM), location = this.getUniformLocation(p, 'useVertexColor');
        if (!location || !this.getUniform(p, location)) return draw.apply(this,args);
        window.materialDraws++;
        const vao=this.getParameter(this.VERTEX_ARRAY_BINDING);
        if (!window.checkedColorVaos.has(vao)) {
          const previous=this.getParameter(this.ARRAY_BUFFER_BINDING);
          this.bindBuffer(this.ARRAY_BUFFER,this.getVertexAttrib(6,this.VERTEX_ATTRIB_ARRAY_BUFFER_BINDING));
          const firstColor=new Float32Array(4);this.getBufferSubData(this.ARRAY_BUFFER,32,firstColor);
          if(!window.sourceColors.has(JSON.stringify([...firstColor])))throw new Error('Uploaded RGBA differs from original decoded vertex color');
          this.bindBuffer(this.ARRAY_BUFFER,previous);window.checkedColorVaos.add(vao);
        }
        if (!this.getVertexAttrib(6, this.VERTEX_ATTRIB_ARRAY_ENABLED) || this.getVertexAttrib(6, this.VERTEX_ATTRIB_ARRAY_STRIDE) !== 48) throw new Error('Original RGBA vertex attribute is not connected');
        const selected = window.sceneryModels?.[0], ml = this.getUniformLocation(p, 'model');
        const matrix = ml && this.getUniform(p, ml);
        if (!selected || !matrix || !selected.world_matrix.every((v,i)=>Math.fround(v)===matrix[i]) || !window.captureMaterial) return draw.apply(this,args);
        const ul = this.getUniformLocation(p, 'sceneryUvOffset');
        const uv = this.getUniform(p, ul), count = this.canvas.width * this.canvas.height;
        const before = new Uint8Array(count*4), authored = new Uint8Array(count*4);
        this.readPixels(0,0,this.canvas.width,this.canvas.height,this.RGBA,this.UNSIGNED_BYTE,before);
        draw.apply(this,args);
        this.readPixels(0,0,this.canvas.width,this.canvas.height,this.RGBA,this.UNSIGNED_BYTE,authored);
        let changed=0; for(let i=0;i<count;i++) if(authored.subarray(i*4,i*4+3).some((v,j)=>v!==before[i*4+j])) changed++;
        window.materialPixels=Math.max(window.materialPixels,changed);
        if (uv.some(v=>Math.abs(v)>0)) window.uvPixels=Math.max(window.uvPixels,changed);
        return undefined;
      };
    }, model);
    await page.goto(`${base}/play.html?drive=1&environment=${environment}`);
    await page.waitForFunction(()=>window.atvTerrainStatus?.ready&&window.atvTerrainStatus?.simulation,null,{timeout:120000});
    await page.locator('#game').focus();
    let state=await page.evaluate(()=>window.atvTerrainStatus);
    assert(state.sceneryPrelitGroups>0);assert.deepEqual(state.sceneryUnsupportedAnimations,[]);
    if(velocity) {
      assert(state.sceneryUv[material]);
      const previous=state.sceneryUv[material],steps=state.simulationSteps;
      await page.waitForFunction(n=>window.atvTerrainStatus.simulationSteps>n+10,steps);
      state=await page.evaluate(()=>window.atvTerrainStatus);
      assert.notDeepEqual(state.sceneryUv[material],previous);
      await page.keyboard.press('Escape');
      await page.waitForTimeout(150);
      const paused=await page.evaluate(()=>window.atvTerrainStatus.sceneryUv);
      await page.waitForTimeout(250);assert.deepEqual(await page.evaluate(()=>window.atvTerrainStatus.sceneryUv),paused);
      await page.getByRole('button',{name:'Resume',exact:true}).click();
    }
    await page.evaluate(()=>window.captureMaterial=true);
    await page.waitForFunction(()=>window.materialPixels>0,null,{timeout:15000});
    const result=await page.evaluate(()=>({draws:window.materialDraws,pixels:window.materialPixels,uvPixels:window.uvPixels,uv:window.atvTerrainStatus.sceneryUv,gl:window.atvTerrainStatus.glError}));
    assert(result.draws>0);assert(result.pixels>0);if(velocity)assert(result.uvPixels>0);assert.equal(result.gl,0);
    await page.evaluate(()=>window.captureMaterial=false);
    if(process.env.ATV_SCENERY_SCREENSHOTS)await page.screenshot({path:`${process.env.ATV_SCENERY_SCREENSHOTS}-${environment}.png`});
    // Pause makes phase retention independent of browser event scheduling.
    await page.keyboard.press('Escape'); await page.waitForTimeout(150);
    // Context restoration must retain the Rust UV phase instead of restarting.
    await page.evaluate(()=>{window.sceneryContext=document.querySelector('#game').getContext('webgl2').getExtension('WEBGL_lose_context');window.sceneryContext.loseContext();});
    await page.waitForTimeout(150);const lost=await page.evaluate(()=>window.atvTerrainStatus.sceneryUv);
    await page.evaluate(()=>window.sceneryContext.restoreContext());
    await page.waitForFunction(()=>window.atvTerrainStatus.ready);
    assert.deepEqual(await page.evaluate(()=>window.atvTerrainStatus.sceneryUv),lost);
    await page.getByRole('button',{name:'Resume',exact:true}).click();
    await page.locator('#game').focus();
    await page.waitForFunction(()=>window.atvTerrainStatus.glError===0);
    assert.deepEqual(errors,[]);results.push({environment,material,...result});console.log(JSON.stringify(results.at(-1)));
    await page.close();
  }
  if(process.env.ATV_SCENERY_REPORT)await fs.writeFile(process.env.ATV_SCENERY_REPORT,JSON.stringify(results,null,2));
  console.log('PASS: source prelit vertex attributes and animated material draws, pause and context retention. Inspection views do not establish original hardware equivalence.');
}finally{await browser.close();}
