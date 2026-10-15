// Real menu selections and browser refreshes using hosted original content.
import assert from 'node:assert/strict';
import {mkdirSync} from 'node:fs';
import {assertNoErrors,base,button,launch,newPage,waitForGame} from './browser/harness.mjs';
const browser=await launch();
const output=process.env.ATV_MENU_PREFERENCES_OUTPUT || 'work/verification/menu-preferences';mkdirSync(output,{recursive:true});
const errors=[];
try {
 const page=await newPage(browser,{viewport:{width:390,height:844},hasTouch:true,isMobile:true},errors);
 const screen=async name=>{await page.waitForFunction(name=>document.querySelector('#frontend').dataset.page===name&&!document.querySelector('#frontend').hidden&&document.querySelector('#menu-buttons').childElementCount>0,name,{timeout:90000});};
 const single=async()=>{await screen('title');await page.keyboard.press('Enter');await screen('main');await button(page,'Single Player').tap();await screen('single');};
 const garage=async key=>{await button(page,'Quad and Rider').tap();await screen('garage');await page.waitForFunction(key=>document.querySelector('.model-view canvas')?.dataset.selection===key,key,{timeout:90000});};
 const saved=()=>page.evaluate(()=>JSON.parse(localStorage.getItem('atv-player-selection')));
 await page.goto(new URL('/play.html',base).href);await single();await garage('1:0');
 await page.locator('[data-action=quadnext]').tap();await page.locator('[data-action=riderchange]').tap();
 await page.waitForFunction(()=>JSON.parse(localStorage.getItem('atv-player-selection')).quadId===2&&JSON.parse(localStorage.getItem('atv-player-selection')).riderId===1);
 await page.reload();await single();await garage('2:1');await page.screenshot({path:`${output}/restored-atv-rider.png`});
 await button(page,'Done').tap();await screen('single');
 await button(page,'Freestyle').tap();await screen('tracks');
 for(let i=0;i<9&&await page.locator('#frontend').getAttribute('data-track')!=='env02';i++){const before=await page.locator('#frontend').getAttribute('data-track');await page.keyboard.press('ArrowRight');await page.waitForFunction(before=>document.querySelector('#frontend').dataset.track!==before,before);}
 assert.equal(await page.locator('#frontend').getAttribute('data-track'),'env02');
 await page.getByRole('button',{name:/^Ride /}).tap();
 await waitForGame(page,{timeout:90000,until:()=>window.atvTerrainStatus.simulation&&document.querySelector('#frontend').hidden});
 assert.equal(await page.evaluate(()=>window.atvTerrainStatus.playerModel.quad_id),2);
 assert.equal((await saved()).freestyleEnvironment,'env02');
 await page.reload();await single();await button(page,'Freestyle').tap();await screen('tracks');
 assert.equal(await page.locator('#frontend').getAttribute('data-track'),'env02');await page.screenshot({path:`${output}/restored-course.png`});
 // Browsing a different preview is not playing it and must not overwrite the last ride.
 await page.keyboard.press('ArrowRight');await page.waitForFunction(()=>document.querySelector('#frontend').dataset.track!=='env02');assert.equal((await saved()).freestyleEnvironment,'env02');
 await page.reload();await single();await garage('2:1');
 // Wrap backward to the final motorcycle choice through actual garage buttons.
 await page.locator('[data-action=quadprev]').tap();await page.locator('[data-action=quadprev]').tap();
 await page.waitForFunction(()=>JSON.parse(localStorage.getItem('atv-player-selection')).quadId===46);
 await page.reload();await single();await garage('46:1');await page.screenshot({path:`${output}/restored-motorcycle.png`});
 await button(page,'Done').tap();await screen('single');
 await button(page,'Race').tap();await screen('race');
 await page.locator('[data-action=racestart]').tap();
 await waitForGame(page,{timeout:90000,until:()=>window.atvTerrainStatus.simulation&&document.querySelector('#frontend').hidden});
 assert.equal(await page.evaluate(()=>window.atvTerrainStatus.playerModel.quad_id),46);
 assert.equal(await page.evaluate(()=>window.atvTerrainStatus.playerModel.motorcycle),true);
 assert.equal((await saved()).quadId,46);assert.equal((await saved()).riderId,1);assert.equal((await saved()).freestyleEnvironment,'env02','Race must preserve last Freestyle course');
 await page.reload();await single();await button(page,'Freestyle').tap();await screen('tracks');assert.equal(await page.locator('#frontend').getAttribute('data-track'),'env02');
 assertNoErrors(errors);console.log(JSON.stringify({gate:'menu-preferences',atvRiderRefresh:true,motorcycleRefresh:true,lastPlayedCourse:true,browsingDoesNotOverwrite:true,raceSelection:true,errors}));
} finally {await browser.close();}
