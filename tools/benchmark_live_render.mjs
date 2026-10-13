// Opt-in serial live-render callback benchmark against a frozen main baseline.
// Does not isolate CPU execution from driver waits or measure phone battery.
import {createRequire} from 'node:module';
import {mkdir,writeFile} from 'node:fs/promises';
const {chromium}=createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE||'playwright');
const before=process.env.ATV_PERF_BASELINE,after=process.env.ATV_TEST_URL;
if(!before||!after)throw new Error('ATV_PERF_BASELINE and ATV_TEST_URL are required');
const out=process.env.ATV_PERF_OUTPUT||'work/verification/performance';await mkdir(out,{recursive:true});
const rows=[];
for(let pair=0;pair<4;pair++)for(const side of pair%2?['after','before']:['before','after']){
 const browser=await chromium.launch({headless:true,args:['--use-angle=metal']});
 try{
  const p=await browser.newPage({viewport:{width:390,height:844},deviceScaleFactor:3,hasTouch:true});
  await p.addInitScript(()=>{
   localStorage.setItem('atv.diagnostics.v1','performance');window.__cpuSamples=null;const raf=requestAnimationFrame;
   window.requestAnimationFrame=cb=>raf.call(window,t=>{if(cb.name==='render'&&window.__cpuSamples){const start=performance.now();cb(t);window.__cpuSamples.push(performance.now()-start);}else cb(t);});
  });
  await p.goto(`${side==='before'?before:after}/play.html?drive=1&environment=env01`);
  await p.waitForFunction(()=>window.atvTerrainStatus?.simulation&&window.atvTerrainStatus.minimap.ready,null,{timeout:90000});await p.bringToFront();await p.locator('#game').focus();await p.waitForTimeout(2000);
  await p.evaluate(()=>window.__cpuSamples=[]);await p.waitForTimeout(10000);
  const row=await p.evaluate(()=>{const a=window.__cpuSamples;window.__cpuSamples=null;const sorted=a.slice().sort((x,y)=>x-y);return{samples:a.length,mean:a.reduce((x,y)=>x+y,0)/a.length,p50:sorted[Math.floor(a.length*.5)],p95:sorted[Math.floor(a.length*.95)],diagnostics:window.atvTerrainStatus.performance};});rows.push({pair,side,...row});console.log(JSON.stringify(rows.at(-1)));
 }finally{await browser.close();}
}
await writeFile(`${out}/paired-live-render.json`,JSON.stringify({scope:'Caribou actual active loop, no draw-call instrumentation, one browser at a time, four alternating serial pairs, 2s warmup/10s sample each, same host Chromium Metal/390x844 DPR3. Callback wall elapsed does not isolate CPU from driver wait.',rows},null,2));
