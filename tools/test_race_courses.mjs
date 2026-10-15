// Host/public integration with original resources. Never runs in private-free CI.
// Starts the original race on every Environment, National and Supercross course
// (or ATV_RACE_COURSES=env02,sx07) and checks the start gate, field and racing.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {mkdirSync} from 'node:fs';
import {RACE_COURSES} from '../web/launch.js';
const {chromium}=createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE||'playwright');
const base=process.env.ATV_TEST_URL||'http://127.0.0.1:4184';
const output=process.env.ATV_RACE_SCREENSHOTS;
if(output)mkdirSync(output,{recursive:true});
const courses=process.env.ATV_RACE_COURSES?process.env.ATV_RACE_COURSES.split(','):RACE_COURSES;
const browser=await chromium.launch({headless:true});
const errors=[],results=[];
try {
 const page=await browser.newPage({viewport:{width:1280,height:720},hasTouch:true});
 page.on('pageerror',e=>errors.push(e.message));
 const screen=async name=>{await page.waitForFunction(name=>document.querySelector('#frontend').dataset.page===name||document.querySelector('#frontend').dataset.error,name,{timeout:90000});assert.equal(await page.locator('#frontend').getAttribute('data-error'),null,await page.locator('#menu-notice').textContent());};
 const state=()=>page.evaluate(()=>window.atvTerrainStatus);
 // Menu: the Course row cycles every published original course in both directions.
 await page.goto(base);await screen('title');
 await page.getByRole('button',{name:'Press START',exact:true}).click();await screen('main');
 await page.getByRole('button',{name:'Single Player',exact:true}).click();await screen('single');
 await page.getByRole('button',{name:'Race',exact:true}).click();await screen('race');
 const course=()=>page.locator('#menu-buttons button[aria-label^="Course: "]').getAttribute('aria-label');
 const first=await course();
 for(const [name,size] of [['desktop',{width:1280,height:720}],['portrait',{width:390,height:844}],['landscape',{width:844,height:390}]]){
  await page.setViewportSize(size);await page.waitForTimeout(300);
  const hits=await page.locator('#menu-buttons button').evaluateAll(buttons=>buttons.map(b=>({label:b.getAttribute('aria-label'),rect:b.getBoundingClientRect().toJSON()})));
  for(const hit of hits){assert(hit.rect.x>=0&&hit.rect.y>=0&&hit.rect.right<=size.width+1&&hit.rect.bottom<=size.height+1,JSON.stringify(hit));assert(hit.rect.height>=44,JSON.stringify(hit));}
  for(const [i,a] of hits.entries())for(const b of hits.slice(i+1))assert(a.rect.right<=b.rect.x||b.rect.right<=a.rect.x||a.rect.bottom<=b.rect.y||b.rect.bottom<=a.rect.y,`${a.label} overlaps ${b.label}`);
  if(output)await page.screenshot({path:`${output}/race-courses-setup-${name}.png`});
 }
 const changed=from=>page.waitForFunction(from=>document.querySelector('#menu-buttons button[aria-label^="Course: "]')?.getAttribute('aria-label')!==from,from);
 await page.locator('#menu-buttons button[aria-label^="Course: "]').click();await changed(first);
 const second=await course();
 await page.keyboard.press('ArrowLeft');await changed(second);assert.equal(await course(),first);
 for(const id of courses){
  await page.setViewportSize({width:1280,height:720});
  await page.goto(`${base}/play.html?mode=single&activity=race&map=${id}&cpu=4&laps=2&quad=1`);
  await page.waitForFunction(()=>window.atvTerrainStatus?.frame?.race?.phase==='countdown'||document.querySelector('#frontend')?.dataset.error,null,{timeout:180000});
  assert.equal(await page.locator('#frontend').getAttribute('data-error'),null,`${id}: ${await page.locator('#menu-notice').textContent()}`);
  let s=await state();const race=s.frame.race;
  assert.equal(race.field,5,id);assert.equal(race.laps,2,id);assert.equal(race.opponents.length,4,id);
  assert(race.checkpoints>=40,`${id} route`);assert(!race.wrong_way,`${id} grid faces the race direction`);
  // Riders line up across the original start gate, one RiderSpacing apart.
  const xs=race.opponents.map(o=>o.origin);const gap=Math.hypot(xs[1][0]-xs[2][0],xs[1][2]-xs[2][2]);
  assert(gap>=15&&gap<=31,`${id} grid gap ${gap}`);
  await page.keyboard.down('KeyW');
  await page.waitForFunction(()=>window.atvTerrainStatus.frame.race.elapsed>=3,null,{timeout:60000});
  await page.keyboard.up('KeyW');
  s=await state();
  assert.equal(s.frame.race.phase,'racing',id);assert.equal(s.glError,0,id);
  assert(s.frame.race.checkpoint>=1,`${id} player progressed`);
  assert(s.frame.race.opponents.filter(o=>o.distance>40).length>=3,`${id} CPUs drive`);
  assert(s.frame.race.opponents.every(o=>o.physical&&o.physical.quad_matrices.flat().every(Number.isFinite)),`${id} CPU physics`);
  assert(await page.locator('#race-overlay').isVisible());
  if(output)await page.screenshot({path:`${output}/race-course-${id}.png`});
  results.push({id,checkpoints:race.checkpoints,gap:Number(gap.toFixed(1)),player_checkpoint:s.frame.race.checkpoint,cpu:s.frame.race.opponents.map(o=>Math.round(o.distance))});
 }
 assert.deepEqual(errors,[]);
 console.log(JSON.stringify({gate:'race-courses',courses:results.length,results}));
 console.log(`PASS: ${results.length} original race courses start from their gates with four physical CPUs; course menu cycles in three layouts; no page or GL errors.`);
} finally { await browser.close(); }
