// Opt-in real-content pause gate. Optional local assets use an existing host relay.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {existsSync} from 'node:fs';
import {resolve,basename} from 'node:path';
const {chromium}=createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || 'playwright');
const base=process.env.ATV_TEST_URL || 'http://127.0.0.1:4193';
const multiplayer=process.env.ATV_PAUSE_MULTIPLAYER==='1';
const browser=await chromium.launch({headless:true,args:['--enable-gpu','--use-angle=metal']});
const errors=[];
const button=(p,name)=>p.getByRole('button',{name,exact:true});
const state=p=>p.evaluate(()=>window.atvTerrainStatus);
const screen=async(p,name)=>{
  await p.waitForFunction(name=>document.querySelector('#frontend').dataset.page===name&&!document.querySelector('#frontend').hidden&&!document.querySelector('#frontend').dataset.error,name,{timeout:90000});
  await p.waitForFunction(()=>!document.getAnimations().some(a=>a.effect?.target?.id==='menu-stage'));
};
async function page() {
  const p=await browser.newPage({viewport:{width:1280,height:896},hasTouch:true,deviceScaleFactor:2});
  p.on('pageerror',e=>errors.push(e.message));
  await p.addInitScript(()=>Object.defineProperty(document,'hasFocus',{value:()=>true}));
  if(process.env.ATV_TEST_ASSET_ROOT) await p.route(`${base}/**`,async route=>{
    const name=new URL(route.request().url()).pathname.slice(1),path=resolve(process.env.ATV_TEST_ASSET_ROOT,name);
    if(name===basename(name)&&existsSync(path))await route.fulfill({path});else await route.continue();
  });
  await p.goto(base+'/play.html');await button(p,'Press START').click({timeout:90000});return p;
}
async function openPause(p) {await button(p,'Pause').click();await screen(p,'pause');}
async function running(p){await p.waitForFunction(()=>window.atvTerrainStatus.simulation&&document.querySelector('#frontend').hidden,null,{timeout:120000});}
async function shot(p,name){if(process.env.ATV_PAUSE_SCREENSHOTS)await p.screenshot({path:`${process.env.ATV_PAUSE_SCREENSHOTS}-${name}.png`});}
try {
  console.log('START',base,multiplayer);const a=await page();console.log('MAIN');
  if(!multiplayer) {
    await button(a,'Single Player').click();await button(a,'Freestyle').click();await button(a,'Ride Caribou Peak').click();await running(a);
    await a.keyboard.down('KeyW');await a.waitForTimeout(700);await openPause(a);await a.keyboard.up('KeyW');
    await a.waitForTimeout(200);const frozen=(await state(a)).simulationSteps;
    await a.waitForTimeout(400);assert.equal((await state(a)).simulationSteps,frozen,'single-player must remain paused');
    for(const size of [{width:1280,height:896},{width:390,height:844},{width:320,height:568},{width:844,height:390},{width:667,height:375}]) {
      await a.setViewportSize(size);await a.waitForTimeout(350);
      const panel=a.locator('.pause-panel');const box=await panel.boundingBox();
      assert(box.x>=0&&box.y>=0&&box.x+box.width<=size.width&&box.y+box.height<=size.height);
      assert.deepEqual(await panel.locator('h2').allTextContents(),['Ride','Preferences','Session']);
      const boxes=await panel.locator('button').evaluateAll(nodes=>nodes.map(n=>{const r=n.getBoundingClientRect();return {x:r.x,y:r.y,w:r.width,h:r.height};}));
      for(const b of boxes)assert(b.h>=44&&b.x>=box.x&&b.x+b.w<=box.x+box.width);
      for(let i=0;i<boxes.length;i++)for(const b of boxes.slice(i+1)){const c=boxes[i];assert(c.x+c.w<=b.x+1||b.x+b.w<=c.x+1||c.y+c.h<=b.y+1||b.y+b.h<=c.y+1,'buttons overlap');}
      await shot(a,`${size.width}x${size.height}`);
      await button(a,'Quit to Main Menu').scrollIntoViewIfNeeded();
      const quit=await button(a,'Quit to Main Menu').boundingBox();assert(quit.y>=box.y&&quit.y+quit.height<=box.y+box.height);
      await button(a,'Resume').scrollIntoViewIfNeeded();
    }
    await a.setViewportSize({width:390,height:844});
    await button(a,'Controls').click();await screen(a,'controls');await shot(a,'controls');await a.keyboard.press('Escape');await screen(a,'pause');
    await button(a,'HUD settings').click();await button(a,'Minimap configuration').click();await screen(a,'minimap-config');await shot(a,'minimap');
    const slider=a.getByRole('slider',{name:'Minimap size',exact:true});await slider.focus();await a.keyboard.press('ArrowRight');await a.waitForTimeout(100);assert.equal(await slider.inputValue(),'168');
    await a.keyboard.press('Escape');await screen(a,'hud-settings');await a.keyboard.press('Escape');await screen(a,'pause');
    await button(a,'Resume').focus();await a.keyboard.press('ArrowDown');await a.waitForTimeout(100);
    assert.equal(await a.locator('.pause-panel [aria-current=true]').getAttribute('data-action'),'reset');
    await a.keyboard.press('ArrowDown');await a.waitForTimeout(100);assert.equal(await a.locator('.pause-panel [aria-current=true]').getAttribute('data-action'),'controls');
    await a.keyboard.press('Enter');await screen(a,'controls');await a.keyboard.press('Escape');await screen(a,'pause');
    // Native Tab focus and Enter must activate the same highlighted control.
    await button(a,'Resume').focus();await a.keyboard.press('Tab');await a.waitForTimeout(100);assert.equal(await a.evaluate(()=>document.activeElement.dataset.action),'reset');
    await button(a,'Resume').click();await running(a);await a.waitForTimeout(200);assert.equal((await state(a)).frame.applied_input.throttle,0);
    console.log('PASS responsive layout, sharp DOM labels, scroll access, keyboard/Tab, settings sliders, single-player freeze and neutral resume');
  } else {
    await button(a,'Multiplayer').click();await button(a,'Create lobby').click();await a.getByRole('textbox',{name:'Rider name',exact:true}).fill('Pause Host');await button(a,'Create lobby').click();
    await a.waitForFunction(()=>window.atvTerrainStatus.multiplayer.state.code);const code=(await state(a)).multiplayer.state.code;console.log('ROOM',code);
    const b=await page();await button(b,'Multiplayer').click();await button(b,'Join lobby').click();await b.getByRole('textbox',{name:'Rider name',exact:true}).fill('Live Guest');await b.getByRole('textbox',{name:'Lobby code',exact:true}).fill(code);await button(b,'Join lobby').click();
    await a.waitForFunction(()=>window.atvTerrainStatus.multiplayer.state.lobby.players.length===2);
    await button(a,'Ride together').click();await Promise.all([running(a),running(b)]);console.log('BOTH RIDING');
    for(const p of [a,b])await p.waitForFunction(()=>window.atvTerrainStatus.multiplayerDraws>0,null,{timeout:60000});
    for(const [menu,rider] of [[a,b],[b,a]]) {
      await menu.bringToFront();await menu.evaluate(()=>window.dispatchEvent(new Event('focus')));
      await menu.keyboard.down('KeyW');await menu.waitForFunction(()=>window.atvTerrainStatus.frame.applied_input.throttle>.5);await openPause(menu);await menu.keyboard.up('KeyW');
      assert.equal(await menu.locator('#pause-title').textContent(),'Ride menu');
      const before=(await state(menu)).simulationSteps;
      await menu.waitForFunction(before=>window.atvTerrainStatus.simulationSteps>before+15,before,{timeout:10000});
      assert.equal((await state(menu)).frame.applied_input.throttle,0,'menu releases local driving');
      const remoteBefore=(await state(menu)).multiplayer.remote[0].position;
      await rider.bringToFront();await rider.evaluate(()=>window.dispatchEvent(new Event('focus')));await rider.keyboard.down('KeyW');
      await menu.waitForFunction(before=>{const s=window.atvTerrainStatus,p=s.multiplayer.remote[0].position;return Math.hypot(...p.map((v,i)=>v-before[i]))>3;},remoteBefore,{timeout:15000});
      await rider.keyboard.up('KeyW');const s=await state(menu);assert(s.multiplayerDraws>0);assert.equal(s.glError,0);
      await menu.bringToFront();await menu.evaluate(()=>window.dispatchEvent(new Event('focus')));await shot(menu,menu===a?'online-host':'online-guest');
      await button(menu,'Controls').click();await screen(menu,'controls');const settingsSteps=(await state(menu)).simulationSteps;
      await menu.waitForFunction(n=>window.atvTerrainStatus.simulationSteps>n+10,settingsSteps,{timeout:10000});
      await menu.keyboard.press('Escape');await screen(menu,'pause');await button(menu,'Players').click();await screen(menu,'players');
      const rosterSteps=(await state(menu)).simulationSteps;await menu.waitForFunction(n=>window.atvTerrainStatus.simulationSteps>n+10,rosterSteps,{timeout:10000});
      await menu.keyboard.press('Escape');await button(menu,'Resume').click();await running(menu);assert.equal((await state(menu)).frame.applied_input.throttle,0);
      console.log('LIVE_MENU',JSON.stringify({role:menu===a?'host':'guest',steps:s.simulationSteps-before,remoteMovement:Math.hypot(...s.multiplayer.remote[0].position.map((v,i)=>v-remoteBefore[i]))}));
    }
    await openPause(b);await a.close();await b.waitForFunction(()=>!window.atvTerrainStatus.multiplayer.state.code,null,{timeout:15000});assert.equal((await state(b)).multiplayer.remote.length,0);
    console.log('PASS both host/guest pause menus and settings keep physics/peers rendering, neutral input, and host departure cleanup');
  }
  assert.deepEqual(errors,[]);
}finally{await browser.close();}
