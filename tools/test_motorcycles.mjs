// Actual UI/WASM/resource gate. Uses ordinary keyboard inputs and read-only
// telemetry. No pose, velocity, contact, crash or solver state is injected.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {mkdir,writeFile} from 'node:fs/promises';
import {captureCrashIsolation,verifyCrashIsolation} from './crash_isolation.mjs';
const {chromium}=createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE||'playwright');
const browser=await chromium.launch({headless:true,args:['--use-angle=metal']});
const page=await browser.newPage({viewport:{width:1280,height:720},hasTouch:true});
const base=process.env.ATV_TEST_URL||'http://127.0.0.1:4397';
const errors=[];page.on('pageerror',e=>errors.push(e.message));
await captureCrashIsolation(page);
const button=name=>page.getByRole('button',{name,exact:true});
const state=()=>page.evaluate(()=>window.atvTerrainStatus);
let selected=1;
const ids=process.env.ATV_MOTORCYCLE_IDS?.split(',').map(Number)??Array.from({length:20},(_,i)=>27+i);
const report={models:[],errors};
try {
 await page.goto(new URL('/play.html',base).href);
 await button('Press START').click({timeout:120000});
 for (const id of ids) {
  console.log(`Motorcycle ${id}: selection and riding`);
  await button('Single Player').click();await button('Quad and Rider').click();
  const next=(id-selected+46)%46,previous=(selected-id+46)%46;
  for(let i=0;i<Math.min(next,previous);i++)await button(next<=previous?'Next quad':'Previous quad').click();
  selected=id;
  await page.waitForFunction(id=>[...document.querySelectorAll('.model-view canvas')].every(c=>c.dataset.selection===`${id}:0`),id);
  assert.equal(await page.locator('#menu-notice').textContent(),'');
  await button('Done').click();await button('Freestyle').click();await button('Ride Caribou Peak').click();
  await page.waitForFunction(id=>(document.querySelector('#frontend').hidden && window.atvTerrainStatus?.simulation && window.atvTerrainStatus.playerModel?.quad_id===id)||document.querySelector('#frontend').dataset.error,id,{timeout:120000});
  let initial=await state();assert(initial.simulation,await page.locator('#menu-notice').textContent());
  assert.equal(initial.playerModel.quad_id,id);assert.equal(initial.frame.vehicle_kind,'motorcycle');assert.equal(initial.frame.grounded.length,2);assert.equal(initial.audio.ready,true);
  await page.locator('#game').focus();await page.keyboard.down('KeyW');
  await page.waitForFunction(()=>window.atvTerrainStatus.frame.speed>12,null,{timeout:20000});
  await page.waitForTimeout(500);
  const driven=await state();assert(Math.hypot(...driven.frame.origin.map((v,i)=>v-initial.frame.origin[i]))>5);
  assert(driven.frame.wheel_angles.every(a=>Math.abs(a)>1));
  assert(driven.frame.wheel_particles.total_emitted.some(n=>n>0));assert.equal(driven.frame.wreck.phase,'Riding');
  await page.keyboard.down('KeyA');await page.waitForFunction(()=>Math.abs(window.atvTerrainStatus.frame.steering_angle)>.05);await page.waitForTimeout(250);await page.keyboard.up('KeyA');
  const turned=await state();assert.notDeepEqual(turned.frame.orientation,driven.frame.orientation);
  assert(turned.frame.rider_grip_errors.every(v=>v<.08),`Grip errors ${turned.frame.rider_grip_errors}`);
  await page.keyboard.down('KeyV');await page.waitForFunction(()=>window.atvTerrainStatus.frame.clutch_active);await page.keyboard.up('KeyV');await page.waitForFunction(()=>window.atvTerrainStatus.frame.clutch_boost>0);await page.keyboard.up('KeyW');
  const clothed=await state();assert(clothed.vehicleTextures>0 && clothed.riderTextures>0);
  if(process.env.ATV_MOTORCYCLE_SCREENSHOTS){await page.mouse.move(900,300);await page.mouse.down();await page.mouse.move(1200,300);await page.mouse.up();await page.waitForTimeout(100);await page.screenshot({path:`${process.env.ATV_MOTORCYCLE_SCREENSHOTS}-${id}.png`});}
  const snapshot=await state();assert.equal(snapshot.glError,0);assert.equal(snapshot.audioError,undefined);
  report.models.push({id,label:snapshot.playerModel.label,speed:driven.frame.speed,origin:driven.frame.origin,steering:turned.frame.steering_angle,particles:snapshot.frame.wheel_particles.total_emitted,samples:snapshot.audio.samples,wheels:snapshot.frame.grounded.length,bikeNodes:snapshot.frame.quad_node_matrices.length,riderNodes:snapshot.frame.rider_node_matrices.length});
  await page.keyboard.press('Escape');await button('Reset ATV').click();await page.waitForFunction(n=>window.atvTerrainStatus.frame.reset_count>n,snapshot.frame.reset_count);await page.keyboard.press('Escape');await button('Quit to Main Menu').click();
 }
 report.isolation=await verifyCrashIsolation(page);assert.deepEqual(errors,[]);report.passed=true;
 console.log(JSON.stringify(report));
 if(process.env.ATV_MOTORCYCLE_REPORT){await mkdir('work/verification',{recursive:true});await writeFile(process.env.ATV_MOTORCYCLE_REPORT,JSON.stringify(report,null,2));}
} finally {await browser.close();}
