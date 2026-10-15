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
  await p.addInitScript(()=>{
    Object.defineProperty(document,'hasFocus',{value:()=>true});
    const pads=navigator.getGamepads.bind(navigator);Object.defineProperty(navigator,'getGamepads',{value:()=>window.pausePad ? [window.pausePad] : pads()});
    window.testSuspends=0;const suspend=AudioContext.prototype.suspend;
    AudioContext.prototype.suspend=function(...args){window.testSuspends++;return suspend.apply(this,args);};
  });
  if(process.env.ATV_TEST_ASSET_ROOT) await p.route(`${base}/**`,async route=>{
    const name=new URL(route.request().url()).pathname.slice(1),path=resolve(process.env.ATV_TEST_ASSET_ROOT,name);
    if(name===basename(name)&&existsSync(path))await route.fulfill({path});else await route.continue();
  });
  await p.goto(base+'/play.html');await button(p,'Press START').click({timeout:90000});return p;
}
async function musicBefore(p) {
  await p.waitForFunction(()=>window.atvTerrainStatus.audio.music.voices>0&&window.atvTerrainStatus.audio.context==='running',null,{timeout:30000});
  return p.evaluate(()=>({audio:window.atvTerrainStatus.audio,suspends:window.testSuspends}));
}
async function musicContinues(p,before,paused) {
  await p.waitForTimeout(700);
  const after=await p.evaluate(()=>({audio:window.atvTerrainStatus.audio,suspends:window.testSuspends}));
  assert.equal(after.suspends,before.suspends,'menu transition must not suspend the audio context');
  assert.equal(after.audio.context,'running');assert.equal(after.audio.musicPreview,paused);
  assert.equal(after.audio.music.playingIndex,before.audio.music.playingIndex,'pause keeps the current song');
  assert(after.audio.music.positionSeconds>before.audio.music.positionSeconds+.3,'soundtrack advances through pause');
  assert.equal(after.audio.menuMusic.voices,0,'ride menu must not start title music');
  if(paused)assert.equal(after.audio.spatial.active,false,'game sounds stay muted');
  console.log('MUSIC_CONTINUES',JSON.stringify({paused,seconds:after.audio.music.positionSeconds-before.audio.music.positionSeconds,suspends:after.suspends-before.suspends}));
}
async function openPause(p) {await button(p,'Pause').click();await screen(p,'pause');}
async function running(p){await p.waitForFunction(()=>window.atvTerrainStatus.simulation&&document.querySelector('#frontend').hidden,null,{timeout:120000});}
async function shot(p,name){if(process.env.ATV_PAUSE_SCREENSHOTS)await p.screenshot({path:`${process.env.ATV_PAUSE_SCREENSHOTS}-${name}.png`});}
try {
  console.log('START',base,multiplayer);const a=await page();console.log('MAIN');
  if(!multiplayer) {
    await button(a,'Single Player').click();await button(a,'Freestyle').click();await button(a,'Ride Caribou Peak').click();await running(a);
    const music=await musicBefore(a);await a.keyboard.down('KeyW');await a.waitForTimeout(700);await openPause(a);await musicContinues(a,music,true);await a.keyboard.up('KeyW');
    await a.waitForTimeout(200);const frozen=(await state(a)).simulationSteps;
    await a.waitForTimeout(400);assert.equal((await state(a)).simulationSteps,frozen,'single-player must remain paused');
    for(const size of [{width:1280,height:896},{width:390,height:844},{width:320,height:568},{width:844,height:390},{width:667,height:375}]) {
      await a.setViewportSize(size);await a.waitForTimeout(350);
      const panel=a.locator('.pause-panel');const box=await panel.boundingBox();
      assert(box.x>=0&&box.y>=0&&box.x+box.width<=size.width&&box.y+box.height<=size.height);
      assert.deepEqual(await panel.locator('button').evaluateAll(nodes=>nodes.map(n=>n.dataset.action)),['resume','reset','settings','copyride','devlog','main']);
      const boxes=await panel.locator('button').evaluateAll(nodes=>nodes.map(n=>{const r=n.getBoundingClientRect();return {x:r.x,y:r.y,w:r.width,h:r.height};}));
      for(const b of boxes)assert(b.h>=44&&b.x>=box.x&&b.x+b.w<=box.x+box.width);
      for(let i=0;i<boxes.length;i++)for(const b of boxes.slice(i+1)){const c=boxes[i];assert(c.x+c.w<=b.x+1||b.x+b.w<=c.x+1||c.y+c.h<=b.y+1||b.y+b.h<=c.y+1,'buttons overlap');}
      await shot(a,`${size.width}x${size.height}`);
      await button(a,'Quit to Main Menu').scrollIntoViewIfNeeded();
      const quit=await button(a,'Quit to Main Menu').boundingBox();assert(quit.y>=box.y&&quit.y+quit.height<=box.y+box.height);
      await button(a,'Resume').scrollIntoViewIfNeeded();
    }
    await a.setViewportSize({width:390,height:844});
    const sizes=[{width:1280,height:896},{width:390,height:844},{width:320,height:568},{width:844,height:390},{width:667,height:375}];
    async function submenuLayout(name) {
      await screen(a,name);
      for(const size of sizes) {
        await a.setViewportSize(size);await a.waitForTimeout(120);
        const panel=a.locator('.pause-panel'),box=await panel.boundingBox();
        assert(box.x>=0&&box.y>=0&&box.x+box.width<=size.width&&box.y+box.height<=size.height);
        assert.equal(await a.locator('#menu-canvas').isVisible(),false,'submenu uses sharp DOM text');
        assert.equal(await panel.evaluate(n=>n.scrollWidth<=n.clientWidth),true,'no horizontal clipping');
        for(const control of await panel.locator('button,input').all()) {
          await control.scrollIntoViewIfNeeded();const b=await control.boundingBox();
          assert(b.height>=44&&b.x>=box.x&&b.x+b.width<=box.x+box.width,`${name} ${size.width}: accessible control`);
          assert(b.y>=box.y-1&&b.y+b.height<=box.y+box.height+1,`${name} ${size.width}x${size.height}: reachable scrolling ${JSON.stringify({control:await control.getAttribute('aria-label')??await control.getAttribute('data-setting'),b,box})}`);
        }
        await panel.evaluate(n=>n.scrollTop=0);
        if(size.width===390||size.width===1280)await shot(a,`${name}-${size.width}`);
      }
      await a.setViewportSize({width:390,height:844});
    }
    // Pause reaches Controls, Display and Audio through the Settings hub (PR #73).
    await button(a,'Settings').click();await submenuLayout('settings');
    await button(a,'Controls').click();await submenuLayout('controls');await musicContinues(a,music,true);await shot(a,'controls');
    const steering=a.getByRole('slider',{name:'Mobile steering sensitivity',exact:true});
    await steering.evaluate(n=>n.dataset.identity='kept');await steering.focus();await a.keyboard.press('ArrowRight');
    await a.waitForFunction(()=>document.querySelector('.pause-panel output').textContent==='90%');
    assert.equal(await steering.getAttribute('data-identity'),'kept','range node survives value update');
    await a.keyboard.press('Tab');await a.waitForTimeout(100);assert.equal(await a.evaluate(()=>document.activeElement.dataset.action),'keyboard');
    await a.keyboard.press('Enter');await submenuLayout('bindings');
    const firstBinding=a.locator('.pause-panel button[data-action^="bind:"]').first();
    const original=await firstBinding.getAttribute('aria-label');await firstBinding.click();
    await a.waitForSelector('.pause-capture');await shot(a,'capture');
    await a.keyboard.press('Escape');await a.waitForSelector('.pause-capture',{state:'detached'});assert.equal(await firstBinding.getAttribute('aria-label'),original);
    await firstBinding.click();await a.waitForSelector('.pause-capture');await a.keyboard.press('F7');await a.waitForSelector('.pause-capture',{state:'detached'});
    assert.match(await firstBinding.getAttribute('aria-label'),/F7/);
    await button(a,'Defaults').click();await a.waitForFunction(original=>document.querySelector('.pause-panel button[data-action^="bind:"]').getAttribute('aria-label')===original,original);
    await button(a,'Next').click();await a.waitForFunction(()=>document.querySelector('.pause-page-count').textContent.startsWith('Page 2'));
    await a.keyboard.press('Escape');await screen(a,'controls');await button(a,'Controller bindings').click();await submenuLayout('bindings');await shot(a,'controller');
    await a.evaluate(()=>window.pausePad={connected:true,axes:[0,0,0,0],buttons:Array.from({length:17},()=>({pressed:false,value:0}))});
    const controllerRow=a.locator('.pause-panel button[data-action^="bind:"]').first();await controllerRow.click();await a.waitForSelector('.pause-capture');
    await a.waitForTimeout(100);await a.evaluate(()=>window.pausePad.buttons[6]={pressed:true,value:1});await a.waitForTimeout(100);
    await a.evaluate(()=>window.pausePad.buttons[6]={pressed:false,value:0});await a.waitForSelector('.pause-capture',{state:'detached'});
    assert.match(await controllerRow.getAttribute('aria-label'),/6|trigger/i);await button(a,'Defaults').click();await controllerRow.focus();await a.waitForTimeout(100);
    await a.evaluate(()=>window.pausePad.buttons[13]={pressed:true,value:1});await a.waitForTimeout(100);
    assert.equal(await a.evaluate(()=>document.activeElement.dataset.action),await a.locator('.pause-panel button[data-action^="bind:"]').nth(1).getAttribute('data-action'));
    await a.evaluate(()=>window.pausePad=null);
    await a.keyboard.press('Escape');await screen(a,'controls');await a.keyboard.press('Escape');await screen(a,'settings');
    await button(a,'Audio').click();await submenuLayout('music');
    await button(a,'Next').click();await a.waitForFunction(()=>document.querySelector('.pause-page-count').textContent.startsWith('Page 2'));
    await a.locator('.pause-panel button[data-action^="song:"]').first().click();await a.waitForFunction(()=>document.querySelector('.pause-panel button[data-playing=true]'));
    await button(a,'Music off').click();await button(a,'Music on').click();await button(a,'Volume').click();
    await a.keyboard.press('Escape');await screen(a,'settings');await a.keyboard.press('Escape');await screen(a,'pause');
    assert.equal(await a.locator('.pause-panel').evaluate(n=>n.scrollTop),0,'reopening starts at Resume');
    await button(a,'Settings').click();await screen(a,'settings');await button(a,'Display').click();await submenuLayout('hud-settings');await button(a,'Minimap configuration').click();await submenuLayout('minimap-config');await shot(a,'minimap');
    const slider=a.getByRole('slider',{name:'Minimap size',exact:true});await slider.focus();await a.keyboard.press('ArrowRight');await a.waitForTimeout(100);assert.equal(await slider.inputValue(),'168');
    await a.keyboard.press('Escape');await screen(a,'hud-settings');await a.keyboard.press('Escape');await screen(a,'settings');await a.keyboard.press('Escape');await screen(a,'pause');
    await button(a,'Resume').focus();await a.keyboard.press('ArrowDown');await a.waitForTimeout(100);
    assert.equal(await a.locator('.pause-panel [aria-current=true]').getAttribute('data-action'),'reset');
    await a.keyboard.press('ArrowDown');await a.waitForTimeout(100);assert.equal(await a.locator('.pause-panel [aria-current=true]').getAttribute('data-action'),'settings');
    await a.keyboard.press('Enter');await screen(a,'settings');await a.keyboard.press('Escape');await screen(a,'pause');
    // Native Tab focus and Enter must activate the same highlighted control.
    await button(a,'Resume').focus();await a.keyboard.press('Tab');await a.waitForTimeout(100);assert.equal(await a.evaluate(()=>document.activeElement.dataset.action),'reset');
    const resumeMusic=await musicBefore(a);await button(a,'Resume').click();await running(a);await musicContinues(a,resumeMusic,false);await a.waitForTimeout(200);assert.equal((await state(a)).frame.applied_input.throttle,0);
    console.log('PASS settings hub and all six submenu routes at five viewports, remap/cancel/defaults/pagination/music, stable ranges, responsive layout, sharp DOM labels, scroll access, keyboard/Tab, settings sliders, single-player freeze and neutral resume');
  } else {
    // Host Game → Select Track → Ride starts riding; the guest joins the live ride.
    await button(a,'Multiplayer').click();await button(a,'Host Game').click();await a.getByRole('textbox',{name:'Rider name',exact:true}).fill('Pause Host');await button(a,'Select Track').click();
    const card=a.locator('.ms-course[data-track-id=env01]');await card.waitFor({timeout:60000});const selected=await card.getAttribute('aria-selected')==='true';await card.click();if(!selected)await a.locator('button.ms-ride').click();
    await running(a);const code=(await state(a)).multiplayer.state.code;assert.match(code,/^[A-Z0-9]{8}$/);console.log('ROOM',code);
    const b=await page();await button(b,'Multiplayer').click();await button(b,'Join Game').click();await b.getByRole('textbox',{name:'Rider name',exact:true}).fill('Live Guest');await b.getByRole('textbox',{name:'Lobby code',exact:true}).fill(code);await button(b,'Join Game').click();
    await a.waitForFunction(()=>window.atvTerrainStatus.multiplayer.state.lobby.players.length===2);
    await Promise.all([running(a),running(b)]);console.log('BOTH RIDING');
    for(const p of [a,b])await p.waitForFunction(()=>window.atvTerrainStatus.multiplayerDraws>0,null,{timeout:60000});
    for(const [menu,rider] of [[a,b],[b,a]]) {
      await menu.bringToFront();await menu.evaluate(()=>window.dispatchEvent(new Event('focus')));
      const music=await musicBefore(menu);await menu.keyboard.down('KeyW');await menu.waitForFunction(()=>window.atvTerrainStatus.frame.applied_input.throttle>.5);await openPause(menu);await menu.keyboard.up('KeyW');await musicContinues(menu,music,true);
      assert.equal(await menu.locator('#pause-title').textContent(),'Pause');
      // Online rows: only the host may change the track for everyone.
      assert.deepEqual(await menu.locator('.pause-panel button').evaluateAll(nodes=>nodes.map(n=>n.dataset.action)),['resume','reset','players','changeride',...(menu===a?['hosttrack']:[]),'settings','copyride','devlog','leave']);
      assert.equal(await button(menu,'Copy invite link').count(),1);assert.equal(await button(menu,'Leave Game').count(),1);
      const before=(await state(menu)).simulationSteps;
      await menu.waitForFunction(before=>window.atvTerrainStatus.simulationSteps>before+15,before,{timeout:10000});
      assert.equal((await state(menu)).frame.applied_input.throttle,0,'menu releases local driving');
      const remoteBefore=(await state(menu)).multiplayer.remote[0].position;
      await rider.bringToFront();await rider.evaluate(()=>window.dispatchEvent(new Event('focus')));await rider.keyboard.down('KeyW');
      await menu.waitForFunction(before=>{const s=window.atvTerrainStatus,p=s.multiplayer.remote[0].position;return Math.hypot(...p.map((v,i)=>v-before[i]))>3;},remoteBefore,{timeout:15000});
      await rider.keyboard.up('KeyW');const s=await state(menu);assert(s.multiplayerDraws>0);assert.equal(s.glError,0);
      await menu.bringToFront();await menu.evaluate(()=>window.dispatchEvent(new Event('focus')));await shot(menu,menu===a?'online-host':'online-guest');
      await button(menu,'Settings').click();await screen(menu,'settings');await button(menu,'Controls').click();await screen(menu,'controls');const settingsSteps=(await state(menu)).simulationSteps;
      await menu.waitForFunction(n=>window.atvTerrainStatus.simulationSteps>n+10,settingsSteps,{timeout:10000});
      await menu.keyboard.press('Escape');await screen(menu,'settings');await menu.keyboard.press('Escape');await screen(menu,'pause');await button(menu,'Players').click();await screen(menu,'players');await button(menu,`Spawn beside ${menu===a?'Live Guest':'Pause Host'}`).waitFor();await shot(menu,'players');
      const rosterSteps=(await state(menu)).simulationSteps;await menu.waitForFunction(n=>window.atvTerrainStatus.simulationSteps>n+10,rosterSteps,{timeout:10000});
      await menu.keyboard.press('Escape');await button(menu,'Resume').click();await running(menu);assert.equal((await state(menu)).frame.applied_input.throttle,0);
      console.log('LIVE_MENU',JSON.stringify({role:menu===a?'host':'guest',steps:s.simulationSteps-before,remoteMovement:Math.hypot(...s.multiplayer.remote[0].position.map((v,i)=>v-remoteBefore[i]))}));
    }
    // Host leaves from Pause: the paused guest keeps the room and becomes host,
    // and its pause menu gains Select Track.
    await openPause(b);await openPause(a);await button(a,'Leave Game').click();await a.waitForFunction(()=>!window.atvTerrainStatus.multiplayer.state.code&&document.querySelector('#frontend').dataset.page==='multiplayer');
    await b.waitForFunction(code=>{const m=window.atvTerrainStatus.multiplayer;return m.state.code===code&&m.state.lobby?.host===m.state.peer&&m.state.lobby.players.length===1&&m.remote.length===0;},code,{timeout:15000});await a.close();
    await button(b,'Select Track').waitFor({timeout:15000});const guestSteps=(await state(b)).simulationSteps;await b.waitForFunction(n=>window.atvTerrainStatus.simulationSteps>n+10,guestSteps,{timeout:10000});await shot(b,'handoff');
    console.log('PASS both host/guest pause menus and settings keep physics/peers rendering, neutral input, online rows, and host handoff on departure');
  }
  assert.deepEqual(errors,[]);
}finally{await browser.close();}
