// Exact original-vs-cropped HUD raster checks using the same runtime art/fonts.
import assert from 'node:assert/strict';
import {readFile,mkdir,writeFile} from 'node:fs/promises';
import {assertNoErrors,base,launch,newPage} from './browser/harness.mjs';
const out=process.env.ATV_PERF_OUTPUT||'work/verification/performance';await mkdir(out,{recursive:true});
const baseline=await readFile(`${out}/baseline/hud.js`,'utf8');
const browser=await launch();
const reports=[];
try{
 const errors=[],p=await newPage(browser,{viewport:{width:390,height:844},deviceScaleFactor:3},errors);
 await p.route('**/game.js*',async route=>{const r=await route.fetch();await route.fulfill({response:r,body:(await r.text())+'\nwindow.__hudAudit = () => hud;'});});
 await p.route('**/audit-hud-baseline.js',r=>r.fulfill({contentType:'application/javascript',body:baseline}));
 await p.goto(`${base}/play.html?drive=1`);await p.waitForFunction(()=>window.atvTerrainStatus?.simulation,null,{timeout:90000});await p.evaluate(()=>window.dispatchEvent(new Event('blur')));
 for(const viewport of [{width:390,height:844},{width:844,height:390},{width:360,height:568},{width:1280,height:720}]){
  await p.setViewportSize(viewport);
  const result=await p.evaluate(async()=>{
   const Old=(await import('./audit-hud-baseline.js')).HeadUpDisplay,New=(await import('./hud.js')).HeadUpDisplay,live=window.__hudAudit(),surface=document.querySelector('#game');
   const old=new Old(live.frontend,surface),next=new New(live.frontend,surface);old.canvas.id='hud-reference';next.canvas.id='hud-candidate';
   for(const renderer of [old,next]){renderer.layout=live.layout;renderer.images=live.images;renderer.ready=true;}
   const original=window.atvTerrainStatus.frame.hud, cases=[];
   for(const speedVisible of [true,false])for(const stunt of [false,true])for(const fill of [0,.45,1]){
    const frame={...original,speed_text:'123',preload_visible:true,preload_blue_alpha:113,preload_fill:fill,stunt_visible:stunt,stunt_name:'Nac Nac',stunt_points:'1234567',stunt_color:[173,87,209],stunt_alpha:163,stunt_scale:1.3,stunt_offsets:[[10,-3],[-5,4]]};
    old.draw(frame,true,{speedVisible});next.draw(frame,true,{speedVisible});
    const a=old.ctx.getImageData(0,0,old.canvas.width,old.canvas.height),full=document.createElement('canvas');full.width=old.canvas.width;full.height=old.canvas.height;
    const c=full.getContext('2d'),pixels=next.ctx.getImageData(0,0,next.canvas.width,next.canvas.height),dpr=Math.min(devicePixelRatio,3);
    if(!next.canvas.hidden)c.putImageData(pixels,Math.round(parseFloat(next.canvas.style.left)*dpr),Math.round(parseFloat(next.canvas.style.top)*dpr));
    const b=c.getImageData(0,0,full.width,full.height);const examples=[];let changed=0,max=0;for(let i=0;i<a.data.length;i++){const d=Math.abs(a.data[i]-b.data[i]);if(d){changed++;max=Math.max(max,d);if(examples.length<12)examples.push({x:Math.floor(i/4)%full.width,y:Math.floor(i/4/full.width),channel:i%4,a:a.data[i],b:b.data[i]});}}
    cases.push({speedVisible,stunt,fill,changed,max,examples,crop:[next.canvas.style.left,next.canvas.style.top,next.canvas.width,next.canvas.height],beforeBytes:a.data.length,afterBytes:pixels.data.length});
    full.width=full.height=1;
   }
   const frame={...original,stunt_visible:false};next.draw(frame,true,{speedVisible:true});const before=next.snapshot();for(let i=0;i<120;i++)next.draw(frame,true,{speedVisible:true});const unchanged=next.snapshot();next.draw(frame,false);const paused=next.snapshot();for(let i=0;i<120;i++)next.draw(frame,false);const hidden=next.snapshot();
   old.canvas.remove();next.canvas.remove();return{cases,before,unchanged,paused,hidden};
  });
  for(const c of result.cases)assert.equal(c.changed,0,`HUD pixels changed at ${JSON.stringify({viewport,...c})}`);
  assert.equal(result.before.draws,result.unchanged.draws);assert.equal(result.paused.clears,result.hidden.clears);assert.equal(result.hidden.backingBytes,4);
  reports.push({viewport,...result});console.log(JSON.stringify(reports.at(-1)));
 }
 assertNoErrors(errors);await writeFile(`${out}/paired-hud.json`,JSON.stringify({scope:'Same host Chromium, exact original art/fonts, DPR3 raster comparison, four viewports, render-only HUD fixtures',reports,errors},null,2));
 console.log('PASS: exact HUD raster, speed ownership, preload fill/alpha, stunt color/scale/offset and inactive/unchanged suppression');
}finally{await browser.close();}
