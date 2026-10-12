// Original-content water failure and buoyant wreck-rider integration gate.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {writeFile} from 'node:fs/promises';
import {installContactHarness,loadContactMap,findWaterRoute} from './runtime_contact_harness.mjs';
const {chromium}=createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE||'playwright');
const base=process.env.ATV_TEST_URL||'http://127.0.0.1:4197';
const environments=(process.env.ATV_WATER_MAPS||'env04,env05,env07,env08').split(',');
const browser=await chromium.launch({headless:true});
try {
  for(const environment of environments) for(const mobile of environment===environments[0]?[false,true]:[false]) {
    const page=await browser.newPage({viewport:mobile?{width:390,height:844}:{width:960,height:600}}),errors=[];
    page.on('pageerror',e=>errors.push(e.message)); await installContactHarness(page);
    try {
      await loadContactMap(page,base,environment); const route=await findWaterRoute(page);
      const drive=await page.evaluate(async route=>{
        await contactCommand(23,{origin:route.origin,yaw:route.yaw}); return contactSteps(450,{throttle:1});
      },route);
      assert(drive.every(f=>f.origin.every(Number.isFinite)&&f.linear_velocity.every(Number.isFinite)));
      const expired=drive.findIndex(f=>f.water_failure.remaining<=0);
      if(route.base-route.target[1]>6) assert(expired>=0,`${environment}: never exhausted deep-water clock`);
      if(expired>=0) {
        assert.equal(drive[expired].reset_count,0);
        assert.equal(drive[expired+1].reset_count,1,'Reset must consume late expiration on next update');
        assert(Math.hypot(drive[expired+1].origin[0]-route.origin[0],drive[expired+1].origin[2]-route.origin[2])<10,'Return to retained dry bank');
        assert(drive[expired+1].origin[1]>drive[expired+1].water_height,'Recovery must be above water');
        assert.equal(drive[expired+1].wreck.phase,'Riding');
      }
      const wet=drive.filter(f=>f.water_failure.submerged&&f.reset_count===0);
      assert(wet.length>0,'Original shoreline must exercise depth gate');
      if(expired>=0) assert(wet.length>=300,'Original five-second submerged interval must run');
      for(let i=1;i<(expired>=0?expired+1:drive.length);i++) {
        const expected=drive[i].origin[1]<Math.fround(drive[i-1].water_height-3);
        assert.equal(drive[i].water_failure.submerged,expected,'Use preceding cached height');
        const previous=drive[i-1];
        const remaining=expected?(previous.wreck.phase==='Riding'?Math.fround(previous.water_failure.remaining-Math.fround(1/60)):Math.fround(previous.water_failure.remaining)):5;
        assert.equal(Math.fround(drive[i].water_failure.remaining),remaining);
      }
      // A high test placement and actual lean input produce the source angle
      // wreck, rather than injecting a wreck or a buoyancy impulse.
      const rider=await page.evaluate(async route=>{
        await contactCommand(23,{origin:[route.target[0],route.base+80,route.target[2]],yaw:0});
        return contactSteps(450,{lean:1});
      },route);
      const waterOnly=rider.filter(f=>f.rider_wreck?.water_touching&&f.rider_wreck.contact_count===0);
      assert(waterOnly.length>10,'Real detached rider must contact water without solid support');
      assert(waterOnly.some(f=>f.rider_wreck.clip==='Swim'&&f.rider_wreck.phase==='water'));
      assert(waterOnly.some(f=>f.rider_wreck.linear_velocity[1]>0),'Buoyancy must reverse descent');
      assert(rider.some(f=>f.wreck.phase==='Recovering'));
      assert(rider.some(f=>f.reset_count>0&&f.wreck.phase==='Riding'&&!f.rider_wreck));
      assert(rider.every(f=>f.origin.every(Number.isFinite)&&(!f.rider_wreck||f.rider_wreck.origin.every(Number.isFinite))));
      const summary={environment,mobile,expired,reset:expired>=0?expired+1:null,waterOnlyFrames:waterOnly.length,buoyancyUpwardSpeed:Math.max(...waterOnly.map(f=>f.rider_wreck.linear_velocity[1])),errors};
      if(process.env.ATV_WATER_REPORT) await writeFile(`${process.env.ATV_WATER_REPORT}-${environment}${mobile?'-mobile':''}.json`,JSON.stringify({summary,route,drive,rider}));
      assert.deepEqual(errors,[]);assert.equal(await page.evaluate(()=>document.querySelector('#game').getContext('webgl2').getError()),0);
      console.log(JSON.stringify(summary));
    } finally {await page.close();}
  }
  console.log('PASS: original deep shoreline drowning/reset, shallow-water clock gates and water-only rider buoyancy, Swim and recovery. Test placements/input routes do not establish complete original gameplay parity.');
} finally {await browser.close();}
