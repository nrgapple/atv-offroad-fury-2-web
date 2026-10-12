// Original Vancouver asset/collision/selector gate. No fixture game data in CI.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {writeFile} from 'node:fs/promises';
import {installContactHarness,loadContactMap} from './runtime_contact_harness.mjs';
const {chromium}=createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE||'playwright');
const base=process.env.ATV_TEST_URL||'http://127.0.0.1:4290';
const browser=await chromium.launch({headless:true});
try {for(const mobile of [false,true]) {
 const page=await browser.newPage({viewport:mobile?{width:390,height:844}:{width:960,height:600}}),errors=[];page.on('pageerror',e=>errors.push(e.message));await installContactHarness(page);
 await loadContactMap(page,base,'free05');
 const result=await page.evaluate(async()=>{
  const status=window.atvTerrainStatus,{scene,placement}=contactAssets;
  const ski=scene.collision_models.find(p=>p.name==='skijump');
  if(!ski)throw Error('Original ski jump collision missing');
  const names=['draglight','l_post','skijump','skilodge1','skilodge2','skilodge3'];
  for(const name of names)if(!placement.placements.some(p=>p.name===name))throw Error(`Missing original solid ${name}`);
  for(const p of placement.placements) {
   const source=scene.collision_models.find(c=>c.name===p.name&&c.world_matrix.every((v,i)=>Math.fround(v)===Math.fround(p.world_matrix[i])));
   if(!source)throw Error('Collider placement differs from original scene');
  }
  const m=ski.world_matrix,world=local=>[0,1,2].map(i=>m[12+i]+m[i]*local[0]+m[4+i]*local[1]+m[8+i]*local[2]);
  // Upper source ramp deck: normal throttle, no forced velocity or jump impulse.
  await contactCommand(23,{origin:world([0,45.3,52.58]),yaw:Math.atan2(-m[8],-m[10])});
  const frames=await contactSteps(600,{throttle:1});
  return {placementCount:placement.placements.length,modelCount:scene.models.length,sceneInstances:status.sceneInstances,unsupported:status.unsupportedSceneMeshes,skippedMaterials:status.skippedSceneGroups,vegetation:status.vegetationPlacements,colliderNames:names,
   frames:frames.map(f=>({origin:f.origin,velocity:f.linear_velocity,grounded:f.grounded,support:f.wheel_obstacle_support,body:f.obstacle_contact_count,wreck:f.wreck.phase,reset:f.reset_count,selected:f.scene_query.selected_static}))};
 });
 assert.equal(result.modelCount,78);assert.equal(result.sceneInstances,75);assert.deepEqual(result.unsupported,[]);assert.equal(result.vegetation,37649);
 assert(result.frames.every(f=>f.origin.every(Number.isFinite)&&f.velocity.every(Number.isFinite)));
 assert(result.frames.some(f=>f.support.some(Boolean)),'Ski ramp must provide original wheel support');
 assert(result.frames.some(f=>f.body>0),'Original solid ramp must provide body contacts');
 const contact=result.frames.findIndex(f=>f.support.some(Boolean));const air=result.frames.findIndex((f,i)=>i>contact&&!f.grounded.some(Boolean));
 assert(air>contact,'Driving off the ski ramp must become airborne');assert(result.frames.slice(air+1).some(f=>f.grounded.some(Boolean)),'Ski-ramp flight must return to ground contact');
 if(process.env.ATV_VANCOUVER_REPORT)await writeFile(`${process.env.ATV_VANCOUVER_REPORT}${mobile?'-mobile':''}.json`,JSON.stringify(result));
 await page.getByRole('button',{name:'Resume',exact:true}).click();
 await page.locator('#game').focus();
 await page.waitForFunction(()=>window.atvTerrainStatus.sceneryTextureFrames?.draglight>0,null,{timeout:15000});
 const frame=await page.evaluate(()=>window.atvTerrainStatus.sceneryTextureFrames.draglight);assert(frame>=1);
 await page.keyboard.press('Escape');await page.waitForTimeout(150);const paused=await page.evaluate(()=>window.atvTerrainStatus.sceneryTextureFrames);await page.waitForTimeout(200);assert.deepEqual(await page.evaluate(()=>window.atvTerrainStatus.sceneryTextureFrames),paused);
 await page.getByRole('button',{name:'Resume',exact:true}).click();
 if(process.env.ATV_VANCOUVER_SCREENSHOTS)await page.screenshot({path:`${process.env.ATV_VANCOUVER_SCREENSHOTS}${mobile?'-mobile':''}.png`});
 assert.deepEqual(errors,[]);assert.equal(await page.evaluate(()=>document.querySelector('#game').getContext('webgl2').getError()),0);
 console.log(JSON.stringify({mobile,modelCount:result.modelCount,drawn:result.sceneInstances,colliders:result.placementCount,vegetation:result.vegetation,wheelFrames:result.frames.filter(f=>f.support.some(Boolean)).length,bodyFrames:result.frames.filter(f=>f.body>0).length,air,frame}));
 await page.goto(`${base}/play.html`);await page.waitForFunction(()=>document.querySelector('#frontend')?.dataset.page==='title');await page.keyboard.press('Enter');await page.getByRole('button',{name:'Single Player',exact:true}).click();await page.getByRole('button',{name:'Freestyle',exact:true}).click();
 for(let i=0;i<9 && await page.locator('#frontend').getAttribute('data-track')!=='free05';i++) {
  const previous=await page.locator('#frontend').getAttribute('data-track');
  await page.getByRole('button',{name:'Next environment',exact:true}).click({force:true});
  await page.waitForFunction(previous=>document.querySelector('#frontend').dataset.track!==previous,previous);
 }
 assert.equal(await page.locator('#frontend').getAttribute('data-track'),'free05');
 await page.getByRole('button',{name:'Ride Vancouver Ridge',exact:true}).click({force:true});
 await page.waitForFunction(()=>window.atvTerrainStatus?.ready&&/free05.tr2$/i.test(window.atvTerrainStatus.source),null,{timeout:120000});
 assert(await page.locator('#disc').isHidden());assert.deepEqual(errors,[]);
 await page.close();
}console.log('PASS: Vancouver original ski-ramp support/body/air/landing, six solid asset types, animation and unlocked menu route on desktop/390px. Full hardware/render parity remains incomplete.');}finally{await browser.close();}
