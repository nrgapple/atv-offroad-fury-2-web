// Opt-in original-content regression for Training's natural start and boundaries.
import assert from 'node:assert/strict';
import {installContactHarness,loadContactMap} from './runtime_contact_harness.mjs';
import {assertNoErrors,base,launch,newPage,waitForGame,waitForSteps} from './browser/harness.mjs';
const browser=await launch();
const errors=[],page=await newPage(browser,{viewport:{width:390,height:844},hasTouch:true,isMobile:true},errors);
await installContactHarness(page);
try {
  await page.goto(`${base}/play.html?drive=1&environment=training`);
  await waitForGame(page,{until:()=>window.atvTerrainStatus.simulation});
  await page.bringToFront(); await page.locator('#game').focus();
  await waitForSteps(page,180,{timeout:120000});
  const idle=await page.evaluate(()=>window.atvTerrainStatus.frame);
  assert.equal(idle.boundary_launch.active,false,'The unmodified Training start must not launch without input');
  assert(Math.abs(idle.speed)<2,'Training must settle naturally at rest');
  assert(idle.grounded.some(Boolean),'Training idle wheels must contact its terrain');
  if(process.env.ATV_TRAINING_SCREENSHOT) await page.screenshot({path:process.env.ATV_TRAINING_SCREENSHOT});
  const cdp=await page.context().newCDPSession(page);
  await cdp.send('Input.dispatchTouchEvent',{type:'touchStart',touchPoints:[{id:1,x:240,y:570,radiusX:6,radiusY:6,force:1}]});
  await page.waitForFunction(start=>Math.hypot(...window.atvTerrainStatus.frame.origin.map((v,i)=>v-start[i]))>5,idle.origin,{timeout:30000});
  await cdp.send('Input.dispatchTouchEvent',{type:'touchCancel',touchPoints:[]});
  await page.keyboard.press('Backspace');
  await waitForSteps(page,180,{timeout:120000});
  const reset=await page.evaluate(()=>window.atvTerrainStatus.frame);
  assert.equal(reset.boundary_launch.active,false,'Reset must preserve Training boundary policy');
  assert(Math.abs(reset.speed)<2);
  await loadContactMap(page,base,'training');
  const result=await page.evaluate(async()=>{
    const source=contactAssets.scene.start_position;
    const config=contactAssets.placement.boundary_launch;
    const hay=contactAssets.scene.collision_models.find(m=>m.name.toLowerCase().includes('hay_ctr'));
    if(!hay) throw Error('Original Training hay collider missing');
    const m=hay.world_matrix, contacts=[];
    for(const distance of [8,14,24]) for(const direction of [-1,1]) {
      const origin=[0,1,2].map(i=>m[12+i]+m[4+i]*2+m[8+i]*distance*direction);
      await contactCommand(23,{origin,yaw:Math.atan2(-m[8]*direction,-m[10]*direction)});
      const frames=await contactSteps(240,{throttle:1});
      contacts.push({support:frames.filter(f=>f.wheel_obstacle_support.some(Boolean)).length,body:frames.filter(f=>f.obstacle_contact_count>0).length});
    }
    const maxX=config.width_scale*(256*config.terrain_width+1000);
    await contactCommand(23,{origin:[maxX+1,source[1],source[2]],yaw:0});
    const boundary=await contactSteps(1);
    return {source,config,crowd:contactAssets.placement.crowd_present,contacts,boundary:boundary[0].boundary_launch};
  });
  console.log(JSON.stringify({idle:{origin:idle.origin,speed:idle.speed},reset:{origin:reset.origin,speed:reset.speed},...result}));
  assert.deepEqual(result.source,[124.07,89.75,636.57]);
  assert.equal(result.crowd,false,'Training crowd audio metadata stays original');
  assert(result.contacts.some(c=>c.support>0&&c.body>0),'Original Training props must still collide');
  assert.equal(result.boundary.active,true,'Training extended boundary remains active outside its native limits');
  assert.equal(await page.evaluate(()=>window.atvTerrainStatus.glError),0);
  assertNoErrors(errors);
  console.log('PASS: natural Training idle180steps,390px touch driving/reset, source prop contacts and active extended boundary.');
} finally {await browser.close();}
