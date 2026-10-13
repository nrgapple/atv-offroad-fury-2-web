// Real menu selections and browser refreshes using hosted original content.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {mkdirSync} from 'node:fs';
const {chromium}=createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || 'playwright');
const browser=await chromium.launch({headless:true,args:['--enable-gpu','--use-angle=metal']});
const base=process.env.ATV_TEST_URL || 'http://127.0.0.1:4327';
const output=process.env.ATV_MENU_PREFERENCES_OUTPUT || 'work/verification/menu-preferences';mkdirSync(output,{recursive:true});
const errors=[];
try {
 const page=await browser.newPage({viewport:{width:390,height:844},hasTouch:true,isMobile:true});page.on('pageerror',e=>errors.push(e.message));
 const screen=async name=>{await page.waitForFunction(name=>document.querySelector('#frontend').dataset.page===name&&!document.querySelector('#frontend').hidden&&document.querySelector('#menu-buttons').childElementCount>0,name,{timeout:90000});};
 const single=async()=>{await screen('title');await page.keyboard.press('Enter');await screen('main');await page.getByRole('button',{name:'Single Player',exact:true}).tap();await screen('single');};
 const garage=async key=>{await page.getByRole('button',{name:'Quad and Rider',exact:true}).tap();await screen('garage');await page.waitForFunction(key=>document.querySelector('.model-view canvas')?.dataset.selection===key,key,{timeout:90000});};
 const saved=()=>page.evaluate(()=>JSON.parse(localStorage.getItem('atv-player-selection')));
 await page.goto(new URL('/play.html',base).href);await single();await garage('1:0');
 await page.locator('[data-action=quadnext]').tap();await page.locator('[data-action=riderchange]').tap();
 await page.waitForFunction(()=>JSON.parse(localStorage.getItem('atv-player-selection')).quadId===2&&JSON.parse(localStorage.getItem('atv-player-selection')).riderId===1);
 await page.reload();await single();await garage('2:1');await page.screenshot({path:`${output}/restored-atv-rider.png`});
 await page.getByRole('button',{name:'Done',exact:true}).tap();await screen('single');
 await page.getByRole('button',{name:'Freestyle',exact:true}).tap();await screen('tracks');
 for(let i=0;i<9&&await page.locator('#frontend').getAttribute('data-track')!=='env02';i++){const before=await page.locator('#frontend').getAttribute('data-track');await page.keyboard.press('ArrowRight');await page.waitForFunction(before=>document.querySelector('#frontend').dataset.track!==before,before);}
 assert.equal(await page.locator('#frontend').getAttribute('data-track'),'env02');
 await page.getByRole('button',{name:/^Ride /}).tap();
 await page.waitForFunction(()=>window.atvTerrainStatus.simulation&&document.querySelector('#frontend').hidden,null,{timeout:90000});
 assert.equal(await page.evaluate(()=>window.atvTerrainStatus.playerModel.quad_id),2);
 assert.equal((await saved()).freestyleEnvironment,'env02');
 await page.reload();await single();await page.getByRole('button',{name:'Freestyle',exact:true}).tap();await screen('tracks');
 assert.equal(await page.locator('#frontend').getAttribute('data-track'),'env02');await page.screenshot({path:`${output}/restored-course.png`});
 // Browsing a different preview is not playing it and must not overwrite the last ride.
 await page.keyboard.press('ArrowRight');await page.waitForFunction(()=>document.querySelector('#frontend').dataset.track!=='env02');assert.equal((await saved()).freestyleEnvironment,'env02');
 await page.reload();await single();await garage('2:1');
 // Wrap backward to the final motorcycle choice through actual garage buttons.
 await page.locator('[data-action=quadprev]').tap();await page.locator('[data-action=quadprev]').tap();
 await page.waitForFunction(()=>JSON.parse(localStorage.getItem('atv-player-selection')).quadId===46);
 await page.reload();await single();await garage('46:1');await page.screenshot({path:`${output}/restored-motorcycle.png`});
 await page.getByRole('button',{name:'Done',exact:true}).tap();await screen('single');
 await page.getByRole('button',{name:'Race',exact:true}).tap();await screen('race');
 await page.locator('[data-action=racestart]').tap();
 await page.waitForFunction(()=>window.atvTerrainStatus.simulation&&document.querySelector('#frontend').hidden,null,{timeout:90000});
 assert.equal(await page.evaluate(()=>window.atvTerrainStatus.playerModel.quad_id),46);
 assert.equal(await page.evaluate(()=>window.atvTerrainStatus.playerModel.motorcycle),true);
 assert.equal((await saved()).quadId,46);assert.equal((await saved()).riderId,1);assert.equal((await saved()).freestyleEnvironment,'env02','Race must preserve last Freestyle course');
 await page.reload();await single();await page.getByRole('button',{name:'Freestyle',exact:true}).tap();await screen('tracks');assert.equal(await page.locator('#frontend').getAttribute('data-track'),'env02');
 assert.deepEqual(errors,[]);console.log(JSON.stringify({gate:'menu-preferences',atvRiderRefresh:true,motorcycleRefresh:true,lastPlayedCourse:true,browsingDoesNotOverwrite:true,raceSelection:true,errors}));
} finally {await browser.close();}
