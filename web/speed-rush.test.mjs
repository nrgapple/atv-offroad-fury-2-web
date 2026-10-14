import test from 'node:test';
import assert from 'node:assert/strict';
import {speedRushAmount,SpeedRush} from './speed-rush.js';
import {RideEffectsSettings} from './render-support.js';
const fps=mph=>mph*5280/3600;
test('speed onset is 40mph, smoothly increases and is bounded at all speeds',()=>{
 for(const mph of [0,10,39.99,40])assert.equal(speedRushAmount(fps(mph)),0);
 assert(speedRushAmount(fps(41))>0);assert(speedRushAmount(fps(41))<.002);
 for(let mph=41;mph<150;mph++)assert(speedRushAmount(fps(mph))>=speedRushAmount(fps(mph-1)));
 assert.equal(speedRushAmount(fps(75),'high'),1);assert.equal(speedRushAmount(1e6,'normal'),.75);
 assert.equal(speedRushAmount(NaN),0);assert.equal(speedRushAmount(Infinity),0);assert.equal(speedRushAmount(100,'off'),0);
});
test('off, low speed, crash and pause skip GPU entirely and clear active effect',()=>{
 const rush=new SpeedRush(new Proxy({},{get(){throw Error('GPU must not be touched');}}));
 for(const args of [{active:false,speed:120,intensity:'high'},{active:true,speed:120,intensity:'off'},{active:true,speed:20,intensity:'high'}]){
  rush.amount=1;rush.draw({...args,dt:1/60,width:1280,height:720});assert.equal(rush.amount,0);assert.equal(rush.draws,0);
 }
});
test('reduced motion defaults off, explicit speed choices persist, invalid/denied storage is safe',()=>{
 const values=new Map(),store={getItem:k=>values.get(k),setItem:(k,v)=>values.set(k,v)};
 assert.equal(new RideEffectsSettings(store,true).speed,'off');
 const setting=new RideEffectsSettings(store);assert.equal(setting.speed,'normal');
 setting.toggleSpeed();assert.equal(new RideEffectsSettings(store).speed,'high');setting.toggleSpeed();assert.equal(new RideEffectsSettings(store).speed,'off');
 values.set('atv-speed-rush','invalid');assert.equal(new RideEffectsSettings(store,true).speed,'off');
 const denied=new RideEffectsSettings({getItem(){throw Error();},setItem(){throw Error();}},true);denied.toggleSpeed();assert.equal(denied.speed,'low');
});
