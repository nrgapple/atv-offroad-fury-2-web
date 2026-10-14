// Private-content integration test: every venue is loaded through its actual URL.
import assert from 'node:assert/strict';import {createRequire} from 'node:module';import fs from 'node:fs/promises';import {ENVIRONMENTS} from '../web/launch.js';
const {chromium}=createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE||'playwright');const base=process.env.ATV_TEST_URL||'http://127.0.0.1:4201';const browser=await chromium.launch({headless:true});const reports=[];
const declared={env02:3,env03:2,env04:2,env05:5,env06:2,env07:5,env08:28,free04:3,free06:15,nat04:7,nat06:1,nat07:1,sx06:1,sx07:1,training:0,game01:4};
try{
 for(const environment of process.env.ATV_TEST_MAPS?.split(',')??ENVIRONMENTS){
  const page=await browser.newPage({viewport:{width:390,height:844}}),errors=[];page.on('pageerror',e=>errors.push(e.message));
  try{
   await page.goto(`${base}/play.html?drive=1&environment=${environment}`);await page.waitForFunction(()=>window.atvTerrainStatus?.ready||document.querySelector('#status.error'),null,{timeout:120000});assert((await page.evaluate(()=>window.atvTerrainStatus)).ready,await page.locator('#status').textContent());await page.locator('#game').focus();
   const steps=await page.evaluate(()=>window.atvTerrainStatus.simulationSteps);await page.waitForFunction(n=>window.atvTerrainStatus.simulationSteps>n+80,steps,{timeout:60000});
   const state=await page.evaluate(()=>({fx:window.atvTerrainStatus.ambientEffects,steps:window.atvTerrainStatus.simulationSteps,glError:document.querySelector('#game').getContext('webgl2').getError()}));
   assert.equal(state.fx?.plan.declared??0,declared[environment]??0,'source common/mode declarations appended exactly once');assert.equal(state.glError,0);assert.deepEqual(errors,[]);
   if(state.fx){assert(state.fx.frame.particles.length<=600);assert(state.fx.draws<=2);assert(state.fx.frame.particles.every(p=>[...p.position,p.size,p.alpha].every(Number.isFinite)));if(state.fx.active)assert(state.fx.frame.total_emitted>0||state.fx.unsupported.length>0);}
   const report={environment,declared:state.fx?.plan.declared??0,placements:state.fx?.plan.placements.length??0,active:state.fx?.active??0,unresolved:state.fx?.plan.unresolved??[],unsupported:state.fx?.unsupported??[],births:state.fx?.frame.total_emitted??0,dropped:state.fx?.frame.dropped??0,particles:state.fx?.frame.particles.length??0,draws:state.fx?.draws??0};reports.push(report);console.log(JSON.stringify(report));
  }catch(error){console.error(JSON.stringify({environment,errors,status:await page.locator('#status').textContent()}));throw error;}finally{await page.close();}
 }
 if(process.env.ATV_AMBIENT_CATALOG_REPORT)await fs.writeFile(process.env.ATV_AMBIENT_CATALOG_REPORT,JSON.stringify(reports,null,2));
 console.log('PASS: all requested original venues load source ambient declarations exactly once with finite bounded frames; unresolved types/actors remain explicit.');
}finally{await browser.close();}
