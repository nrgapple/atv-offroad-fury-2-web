// Opt-in original-content/relay gate: the host's Select Track picker shows every
// course preview, and hosting a chosen non-default track puts the host and an
// invited guest on that track.
import assert from 'node:assert/strict';
import {ENVIRONMENTS} from '../web/launch.js';
import {assertNoErrors,base,button,launch,newPage} from './browser/harness.mjs';
const track=process.env.ATV_LOBBY_TRACK||'sx01';
const browser=await launch();
const errors=[];
const host=await newPage(browser,{viewport:{width:1280,height:720}},errors);
const guest=await newPage(browser,{viewport:{width:390,height:844},hasTouch:true,isMobile:true},errors);
for(const page of [host,guest]) {
  await page.addInitScript(()=>{Object.defineProperty(document,'hasFocus',{value:()=>true});});
}
const riding=(page,environment,remotes)=>page.waitForFunction(([environment,remotes])=>{
  const s=window.atvTerrainStatus,m=s?.multiplayer;
  return s?.multiplayerError||s?.simulation&&document.querySelector('#frontend').hidden&&m?.state.environment===environment&&m.remote.length===remotes;
},[environment,remotes],{timeout:120000});
try {
  await host.goto(`${base}/play.html`);await button(host,'Press START').click({timeout:90000});
  await button(host,'Multiplayer').click();await button(host,'Host Game').click();await button(host,'Select Track').click();
  // Every course card carries its loaded preview; the hero follows the selection.
  await host.locator('.ms-course').first().waitFor({timeout:60000});
  await host.waitForFunction(()=>[...document.querySelectorAll('.ms-course img')].every(img=>img.complete),null,{timeout:60000});
  const cards=await host.locator('.ms-course').evaluateAll(nodes=>nodes.map(n=>({id:n.dataset.trackId,name:n.getAttribute('aria-label'),src:n.querySelector('img')?.getAttribute('src')??null,width:n.querySelector('img')?.naturalWidth??0})));
  assert.deepEqual(cards.map(c=>c.id).sort(),[...ENVIRONMENTS].sort(),'picker lists every course');
  assert.deepEqual(cards.filter(c=>!c.src||c.width===0).map(c=>c.id),[],'every course shows a loaded preview');
  const chosen=cards.find(c=>c.id===track);assert.ok(chosen,`picker offers ${track}`);
  await host.locator(`.ms-course[data-track-id=${track}]`).click();
  await host.waitForFunction(([id,src,name])=>document.querySelector(`.ms-course[data-track-id=${id}]`)?.getAttribute('aria-selected')==='true'&&document.querySelector('.ms-track-hero .ms-photo img')?.getAttribute('src')===src&&document.querySelector('.ms-track-hero .ms-track-name')?.textContent===name,[track,chosen.src,chosen.name],{timeout:15000});
  assert.equal(await host.locator('button.ms-ride').getAttribute('aria-label'),`Ride ${chosen.name}`);
  if(process.env.ATV_LOBBY_SCREENSHOTS)await host.screenshot({path:`${process.env.ATV_LOBBY_SCREENSHOTS}-picker.png`});
  console.log(JSON.stringify({courses:cards.length,previews:cards.filter(c=>c.width>0).length,chosen}));
  // Ride hosts the room on the chosen track; an invited guest lands on it too.
  await host.locator('button.ms-ride').click();await riding(host,track,0);
  const code=await host.evaluate(()=>window.atvTerrainStatus.multiplayer.state.code);
  await guest.goto(`${base}/play.html?room=${code}&player=Guest&quad=2`);
  await Promise.all([riding(host,track,1),riding(guest,track,1)]);
  const states=await Promise.all([host,guest].map(page=>page.evaluate(()=>{const s=window.atvTerrainStatus;return {error:s.multiplayerError,source:s.source,environment:s.multiplayer.state.environment,map:s.multiplayer.state.lobby.map,peer:s.multiplayer.state.peer,host:s.multiplayer.state.lobby.host};})));
  for(const s of states){assert.equal(s.error,undefined);assert.equal(s.environment,track);assert.equal(s.map,ENVIRONMENTS.indexOf(track)+1);assert.match(s.source,new RegExp(`${track}\\.tr2$`,'i'));}
  // Only the host changes the track.
  await button(guest,'Pause').click();await button(guest,'Leave Game').waitFor();assert.equal(await button(guest,'Select Track').count(),0,'Only the host changes the map');
  if(process.env.ATV_LOBBY_SCREENSHOTS)await guest.screenshot({path:`${process.env.ATV_LOBBY_SCREENSHOTS}-guest.png`});
  assertNoErrors(errors);
  console.log(JSON.stringify({code,track,clients:states}));
  console.log(`PASS: Select Track shows ${cards.length} course previews; hosting ${track} puts host and invited guest on it.`);
}catch(error){console.error(JSON.stringify({errors,states:await Promise.all([host,guest].map(page=>page.evaluate(()=>({page:document.querySelector('#frontend')?.dataset.page,state:window.atvTerrainStatus?.multiplayer?.state,notice:document.querySelector('#menu-notice')?.textContent})).catch(()=>null)))}));throw error;}finally{await browser.close();}
