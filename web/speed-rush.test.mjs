import test from 'node:test';
import assert from 'node:assert/strict';
import {speedRushAmount,speedRushFocalScale,speedRushTaps,speedRushFocus,speedRushBounds,SpeedRush} from './speed-rush.js';
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
test('FOV kick, tap tiers and travel focus stay bounded',()=>{
 assert.equal(speedRushFocalScale(0),1);assert(Math.abs(speedRushFocalScale(1)-.91)<1e-9);assert.equal(speedRushFocalScale(9),speedRushFocalScale(1));
 assert.equal(speedRushTaps(false),8);assert.equal(speedRushTaps(true),5);assert(speedRushTaps(true,true)>=3);
 const identity=[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1];
 assert.equal(speedRushFocus(identity,[0,0,0],[0,0,0]),null);
 const [x,y]=speedRushFocus(identity,[0,0,0],[100,100,0]);assert.equal(x,.8);assert.equal(y,.8);
 const [cx,cy,hx,hy]=speedRushBounds(identity,[0,0,0],[.2,.2,.2]);
 // The box spans one foot below the origin up to its height.
 assert(Math.abs(cx-.5)<1e-9&&Math.abs(hx-.1)<1e-9&&Math.abs(cy-.3)<1e-9&&Math.abs(hy-.3)<1e-9);
});
test('effect amount drives the FOV kick and fully resets when inactive',()=>{
 const rush=new SpeedRush(null);
 for(let i=0;i<120;i++)rush.update({speed:fps(100),intensity:'high',active:true,dt:1/60});
 assert(rush.amount>.95&&rush.focalScale<.92);
 rush.update({speed:fps(100),intensity:'high',active:false,dt:1/60});assert.equal(rush.amount,0);assert.equal(rush.focalScale,1);
 for(let i=0;i<120;i++)rush.update({speed:fps(100),intensity:'normal',active:true,dt:1/10});
 assert.equal(rush.taps,speedRushTaps(false,true));
});
