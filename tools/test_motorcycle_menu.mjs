// Original-content menu gate: standing MX clips must reach the GPU, animate,
// and remain separate from the original ATV rigs. No pose state is injected.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {writeFile} from 'node:fs/promises';
const {chromium}=createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE||'playwright');
const browser=await chromium.launch({headless:true,args:['--use-angle=metal']});
try {
 const page=await browser.newPage({viewport:{width:390,height:844},hasTouch:true}),errors=[];
 page.on('pageerror',e=>errors.push(e.message));
 await page.addInitScript(()=>{
  const get=WebGL2RenderingContext.prototype.getUniformLocation,mat=WebGL2RenderingContext.prototype.uniformMatrix4fv,locations=new WeakMap();
  WebGL2RenderingContext.prototype.getUniformLocation=function(...args){const loc=get.apply(this,args);if(loc)locations.set(loc,args[1]);return loc;};
  WebGL2RenderingContext.prototype.uniformMatrix4fv=function(loc,t,v){
   if(locations.get(loc)==='bones[0]'&&this.canvas.getAttribute('aria-label')?.startsWith('Rider'))window.menuPose=Array.from(v);
   return mat.call(this,loc,t,v);
  };
  const post=Worker.prototype.postMessage,pending=new WeakMap();
  Worker.prototype.postMessage=function(m,...rest){
   if(!pending.has(this)){const ids=new Set();pending.set(this,ids);this.addEventListener('message',({data})=>{if(ids.delete(data.id)&&data.bytes&&!data.error)window.menuRig=JSON.parse(new TextDecoder().decode(data.bytes)).mesh.nodes.map(n=>n.name);});}
   if(m.op===103)pending.get(this).add(m.id);
   return post.call(this,m,...rest);
  };
 });
 const button=name=>page.getByRole('button',{name,exact:true}),rider=page.locator('.model-view canvas').last();
 const ready=id=>page.waitForFunction(id=>[...document.querySelectorAll('.model-view canvas')].every(c=>c.dataset.ready==='true'&&c.dataset.selection===`${id}:0`)&&document.querySelectorAll('.model-view canvas')[1]?.dataset.clip?.startsWith('ui_rider_select_'),id,{timeout:90000});
 await page.goto(`${process.env.ATV_TEST_URL||'http://127.0.0.1:4398'}/play.html`);
 await button('Press START').click({timeout:120000});await button('Single Player').click();await button('Quad and Rider').click();
 await page.waitForFunction(()=>document.querySelectorAll('.model-view canvas')[1]?.dataset.clip?.startsWith('mrider_body_'));
 const report={models:[],clips:[],errors};
 // Traverse the whole motorcycle catalog, including the Mini/full-size boundary.
 for(let i=0;i<20;i++)await button('Previous quad').click();
 for(let id=27;id<=46;id++){
  if(id>27)await button('Next quad').click();await ready(id);
  await page.waitForFunction(()=>Number(document.querySelectorAll('.model-view canvas')[1].dataset.animationTime)>.1);
  const pose=await page.evaluate(()=>{
   const at=name=>window.menuPose.slice(window.menuRig.indexOf(name)*16+12,window.menuRig.indexOf(name)*16+15);
   return {head:at('Head'),pelvis:at('Pelvis'),leftFoot:at('Dummy_foot_L'),rightFoot:at('Dummy_foot_R')};
  });
  assert(pose.pelvis[1]>3,`Menu rider ${id} still crouches: ${JSON.stringify(pose)}`);
  assert(pose.head[1]>4.8,`Menu rider ${id} head must stand above riding pose`);
  assert.equal(await page.locator('#menu-notice').textContent(),'');
  report.models.push({id,pose});
  if([29,45].includes(id)){
   const before=await rider.screenshot(),boneBefore=await page.evaluate(()=>window.menuPose);
   await page.waitForTimeout(700);
   assert.notDeepEqual(await rider.screenshot(),before,'Source breathing must change rendered pixels');
   assert.notDeepEqual(await page.evaluate(()=>window.menuPose),boneBefore,'Source breathing must update GPU bones');
   if(process.env.ATV_MX_MENU_SCREENSHOTS)await page.screenshot({path:`${process.env.ATV_MX_MENU_SCREENSHOTS}-${id}.png`});
  }
 }
 const seen=new Set();
 for(let i=0;i<240;i++){
  seen.add(await rider.getAttribute('data-clip'));if([1,2,3].every(n=>seen.has(`ui_rider_select_idle${n}.mot`)))break;
  await page.waitForTimeout(150);
 }
 report.clips=[...seen];assert([1,2,3].every(n=>seen.has(`ui_rider_select_idle${n}.mot`)),'All three source idle clips must play');
 // Returning to an ATV must restore its original menu animation bank.
 await button('Next quad').click();await page.waitForFunction(()=>document.querySelectorAll('.model-view canvas')[1]?.dataset.selection==='1:0'&&document.querySelectorAll('.model-view canvas')[1]?.dataset.clip?.startsWith('mrider_body_'));
 assert.equal(await page.locator('#menu-notice').textContent(),'');assert.deepEqual(errors,[]);report.passed=true;
 await writeFile(process.env.ATV_MX_MENU_REPORT||'work/verification/mx-standing-menu.json',JSON.stringify(report,null,2));console.log(JSON.stringify(report));
}finally{await browser.close();}
