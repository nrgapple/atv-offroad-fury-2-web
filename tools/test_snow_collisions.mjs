// Caribou's original ramp and all five authored ski obstacle placements.
import assert from 'node:assert/strict';
import {writeFile} from 'node:fs/promises';
import {installContactHarness,loadContactMap} from './runtime_contact_harness.mjs';
import {assertNoErrors,base,launch,newPage} from './browser/harness.mjs';
const browser=await launch();
try {for(const mobile of [false,true]) {
  const errors=[],page=await newPage(browser,{viewport:mobile?{width:390,height:844}:{width:960,height:600}},errors);await installContactHarness(page);
  try {
    await loadContactMap(page,base,'env01');
    const rows=await page.evaluate(async()=>{
      const models=contactAssets.scene.models.filter(p=>['e01ramp','snowski1'].includes(p.name));
      if(models.length!==6)throw Error('Expected original ramp and five ski placements');
      // Source visual and COL setters use different orientation rules. Match
      // the original collision placement, not an orthogonalized visual basis.
      for(const p of models) {
        const source=contactAssets.scene.collision_models.find(c=>c.name===p.name&&c.world_matrix.slice(12,15).every((v,i)=>Math.fround(v)===Math.fround(p.world_matrix[12+i])));
        if(!source||!contactAssets.placement.placements.some(c=>c.name===p.name&&c.world_matrix.every((v,i)=>Math.fround(v)===Math.fround(source.world_matrix[i])))) throw Error(`Missing loaded collider for ${p.name}`);
      }
      const rows=[];
      for(const p of models) for(const direction of [1,-1]) {
        const m=p.world_matrix, distance=p.name==='e01ramp'?40:12, local=[0,2,direction*distance];
        const origin=[0,1,2].map(i=>m[12+i]+m[i]*local[0]+m[4+i]*local[1]+m[8+i]*local[2]);
        await contactCommand(23,{origin,yaw:Math.atan2(-m[8]*direction,-m[10]*direction)});
        const frames=await contactSteps(420,{throttle:1});
        rows.push({name:p.name,world:m,direction,frames:frames.map(f=>({origin:f.origin,velocity:f.linear_velocity,grounded:f.grounded,support:f.wheel_obstacle_support,body:f.obstacle_contact_count,wreck:f.wreck.phase,reset:f.reset_count,selected:f.scene_query.selected_static}))});
      }
      return rows;
    });
    for(const row of rows) {
      assert(row.frames.every(f=>f.origin.every(Number.isFinite)&&f.velocity.every(Number.isFinite)));
      assert(row.frames.some(f=>f.selected>0),'Original static cache must admit props');
      assert(row.frames.some(f=>f.support.some(Boolean)),`${row.name}: no wheel support`);
      assert(row.frames.some(f=>f.body>0),`${row.name}: no body response`);
      if(row.name==='e01ramp'&&row.direction===-1) {
        const contact=row.frames.findIndex(f=>f.support.some(Boolean));
        const air=row.frames.findIndex((f,i)=>i>contact&&!f.grounded.some(Boolean));
        assert(air>contact);assert(row.frames.slice(air+1).some(f=>f.grounded.some(Boolean)));
        assert(row.frames.every(f=>f.wreck==='Riding'),'Ramp approach must launch/land cleanly');
        assert(Math.max(...row.frames.map(f=>f.velocity[1]))>20);
      }
    }
    if(process.env.ATV_SNOW_REPORT)await writeFile(`${process.env.ATV_SNOW_REPORT}${mobile?'-mobile':''}.json`,JSON.stringify(rows));
    assertNoErrors(errors);assert.equal(await page.evaluate(()=>document.querySelector('#game').getContext('webgl2').getError()),0);
    console.log(JSON.stringify({mobile,routes:rows.map(r=>({name:r.name,direction:r.direction,supportFrames:r.frames.filter(f=>f.support.some(Boolean)).length,bodyFrames:r.frames.filter(f=>f.body>0).length,upwardSpeed:Math.max(...r.frames.map(f=>f.velocity[1]))}))}));
  } finally {await page.close();}
}console.log('PASS: original Caribou ramp and all five ski placements supply wheel/body contacts from both directions; ramp launch/landing passes desktop and390px. This is bounded collision coverage.');}finally{await browser.close();}
