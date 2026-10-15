// Source-coordinate regression: observe real GPU projection and real input/body
// turns. No source coordinates, textures or synthetic physics fixtures in Git.
import assert from 'node:assert/strict';
import {mkdir,writeFile} from 'node:fs/promises';
import {assertNoErrors,base,button,launch,newPage,waitForSteps} from './browser/harness.mjs';
const output=process.env.ATV_ORIENTATION_OUTPUT||'work/verification/world-orientation';
await mkdir(output,{recursive:true});
const browser=await launch();
const report={turns:[],errors:[]};
try {
 const page=await newPage(browser,{viewport:{width:390,height:844},hasTouch:true},report.errors);
 await page.addInitScript(()=>{
  const p=WebGL2RenderingContext.prototype,names=new WeakMap(),get=p.getUniformLocation,upload=p.uniformMatrix4fv;
  window.orientationProbe={uploads:0,errors:[]};
  p.getUniformLocation=function(program,name){const loc=get.call(this,program,name);if(loc)names.set(loc,name);return loc;};
  p.uniformMatrix4fv=function(loc,transpose,value,...rest){
   if(names.get(loc)==='viewProjection') {
    const camera=window.atvTerrainStatus?.camera;
    if(camera){
     const right=[Math.cos(camera[3]),0,-Math.sin(camera[3])];
     const row=[value[0],value[4],value[8]],length=Math.hypot(...row);
     const agreement=row.reduce((s,v,i)=>s+v*right[i],0)/length;
     orientationProbe.uploads++;if(agreement<.999)orientationProbe.errors.push({agreement,camera});
    }
   }
   return upload.call(this,loc,transpose,value,...rest);
  };
 });
 await page.goto(`${base}/?mode=single&map=env02&quad=1`);
 await page.waitForFunction(()=>window.atvTerrainStatus?.simulation&&window.atvTerrainStatus.minimap?.visible,null,{timeout:90000});
 const state=()=>page.evaluate(()=>window.atvTerrainStatus);
 const heading=q=>Math.atan2(2*(q[0]*q[2]+q[1]*q[3]),1-2*(q[1]*q[1]+q[2]*q[2]));
 const cdp=await page.context().newCDPSession(page);
 const finger=(x)=>({id:1,x,y:620,radiusX:5,radiusY:5,force:1});
 for(const method of ['keyboard','controller','touch'])for(const direction of [-1,1]) {
  await button(page,'Pause').click();
  await button(page,'Reset ATV').click();
  await page.waitForFunction(()=>document.querySelector('#frontend').hidden&&window.atvTerrainStatus.simulation&&Math.abs(window.atvTerrainStatus.frame.speed)<1);
  await page.locator('#game').focus();
  if(method==='keyboard')await page.keyboard.down('KeyW');
  else if(method==='controller')await page.evaluate(()=>{
   window.turnPad={connected:true,axes:[0,0,0,0],buttons:Array.from({length:16},(_,i)=>({value:i===7?1:0,pressed:i===7}))};
   Object.defineProperty(navigator,'getGamepads',{configurable:true,value:()=>window.turnPad?[window.turnPad]:[]});
  });
  else await cdp.send('Input.dispatchTouchEvent',{type:'touchStart',touchPoints:[finger(180)]});
  await page.waitForFunction(()=>window.atvTerrainStatus.frame.speed>12,null,{timeout:15000});
  const before=await state(),yaw=heading(before.frame.orientation),right=[Math.cos(yaw),0,-Math.sin(yaw)];
  if(method==='keyboard')await page.keyboard.down(direction<0?'KeyA':'KeyD');
  else if(method==='controller')await page.evaluate(d=>window.turnPad.axes[0]=d*.65,direction);
  else await cdp.send('Input.dispatchTouchEvent',{type:'touchMove',touchPoints:[finger(180+direction*50)]});
  await page.waitForFunction(d=>window.atvTerrainStatus.input.steer*d>.4,direction);
  await waitForSteps(page,27);
  const after=await state(),angle=heading(after.frame.orientation)-yaw;
  const turn=Math.atan2(Math.sin(angle),Math.cos(angle));
  const side=after.frame.origin.reduce((sum,v,i)=>sum+(v-before.frame.origin[i])*right[i],0);
  assert(turn*direction>.04,`${method} ${direction}: original-world heading turn ${turn}`);
  assert(side*direction>.1,`${method} ${direction}: on-screen displacement ${side}`);
  report.turns.push({method,direction,turn,side,steer:after.input.steer});
  if(method==='keyboard'){await page.keyboard.up('KeyW');await page.keyboard.up(direction<0?'KeyA':'KeyD');}
  else if(method==='controller')await page.evaluate(()=>window.turnPad=null);
  else await cdp.send('Input.dispatchTouchEvent',{type:'touchCancel',touchPoints:[]});
  await page.waitForFunction(()=>Math.abs(window.atvTerrainStatus.input.steer)<.001);
 }
 report.projection=await page.evaluate(()=>window.orientationProbe);
 assert(report.projection.uploads>10);assert.deepEqual(report.projection.errors,[]);
 assertNoErrors(report.errors);
 console.log('PASS: original +X projects screen-right; keyboard/controller/touch turn left and right in the actual quad simulation.');
} finally {await writeFile(`${output}/results.json`,JSON.stringify(report,null,2));await browser.close();}
