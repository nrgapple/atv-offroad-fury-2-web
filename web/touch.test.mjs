import test from 'node:test';
import assert from 'node:assert/strict';
import { TouchInput, TouchLeanIntent } from './touch.js';

test('ground right press drives, back drag brakes, release requests preload once', () => {
  const touch=new TouchInput();touch.begin(1,300,500);
  assert.equal(touch.sample().preload_axis,1);assert.equal(touch.sample().drive.axis,1);
  touch.move(1,305,507);assert.equal(touch.sample().drive.axis,1);
  touch.move(1,300,570);assert.equal(touch.sample().drive.axis,-1);
  touch.end(1);assert.equal(touch.sample().preload_axis,-1);assert.equal(touch.sample().preload_axis,null);
});

test('cancel, clear and unseen ground tap never release preload', () => {
  for(const reason of ['cancel','clear','tap']) {
    const touch=new TouchInput();touch.begin(1);const generation=touch.generation;
    if(reason!=='tap')touch.sample();
    if(reason==='clear')touch.clear();else touch.end(1,reason==='cancel');
    const sample=touch.sample();assert.equal(sample.preload_axis,null);assert.equal(sample.drive.axis,0);
    if(reason!=='tap')assert(touch.generation>generation);
  }
});

test('right thumb ownership is exclusive in both modes', () => {
  for (const airborne of [false,true]) {
    const touch=new TouchInput();assert(touch.begin(1,300,500,airborne));assert(!touch.begin(2,350,500,airborne));
    touch.move(2,350,570);touch.end(2);assert(touch.owns(1));
  }
});

test('left thumb steers and leans with exclusive ownership', () => {
  const left=new TouchLeanIntent();assert(left.begin(1,60,500));assert(!left.begin(2,70,510));
  left.move(1,130,570);assert.equal(left.steer,1);assert.equal(left.lean,1);
  left.move(2,0,0);left.end(2);assert.equal(left.lean,1);
  left.move(1,60,430);assert.equal(left.lean,-1);assert.equal(left.steer,0);
  left.end(1);assert.equal(left.lean,0);assert.equal(left.steer,0);
});

test('single airborne thumb swipes and holds eight original selections without drive', () => {
  for(const [dx,dy,modifier,x,y] of [[0,-70,1,0,-1],[0,70,1,0,1],[-70,0,1,-1,0],[70,0,1,1,0],[-70,-70,2,0,-1],[70,-70,2,1,0],[-70,70,2,-1,0],[70,70,2,0,1]]) {
    const touch=new TouchInput();touch.begin(1,300,500,true);touch.move(1,300+dx,500+dy,0);
    assert.equal(touch.sample(100).stunt_modifiers,0,'Wait for swipe to settle so circles can be recognized');
    const sample=touch.sample(120);assert.equal(sample.drive.axis,0);assert.equal(sample.preload_axis,null);
    assert.equal(sample.stunt_modifiers,modifier);assert.equal(sample.stunt_x,x);assert.equal(sample.stunt_y,y);
    assert.equal(touch.sample(1000).stunt_modifiers,modifier,'Hold retains selection');
    touch.end(1);assert.equal(touch.sample().stunt_modifiers,0);assert.equal(touch.sample().preload_axis,null);
  }
});

test('clockwise and counterclockwise closed circles choose Cat Nac and Cordova', () => {
  for (const direction of [1,-1]) {
    const touch=new TouchInput();touch.begin(1,340,500,true);
    for(let i=1;i<=24;i++) {
      const angle=direction*i*Math.PI*2/24;
      touch.move(1,300+40*Math.cos(angle),500+40*Math.sin(angle),i*10);
      if(i<21)assert.equal(touch.sample(i*10).stunt_modifiers,0);
    }
    const sample=touch.sample(240);assert.equal(sample.stunt_modifiers,3);assert.equal(sample.stunt_x,0);assert.equal(sample.stunt_y,direction>0?-1:1);
    touch.end(1);assert.equal(touch.sample().stunt_modifiers,0);
  }
});

test('takeoff reanchors held thumb and landing immediately resumes drive', () => {
  const touch=new TouchInput();touch.begin(1,300,500);touch.move(1,300,430);touch.sample();
  touch.setAirborne(true);let sample=touch.sample();assert.equal(sample.drive.axis,0);assert.equal(sample.preload_axis,null);assert.equal(sample.stunt_modifiers,0);
  touch.move(1,300,360,0);assert.equal(touch.sample(120).stunt_modifiers,1);
  touch.setAirborne(false);sample=touch.sample();assert.equal(sample.drive.axis,1);assert.equal(sample.stunt_modifiers,0);
  touch.move(1,300,360);assert.equal(touch.sample().drive.axis,1);
  touch.end(1);assert.equal(touch.sample().preload_axis,-1);touch.begin(2,300,500);assert.equal(touch.sample().drive.axis,1);
});

test('quick released swipe is delivered once; cancellation discards it', () => {
  const touch=new TouchInput();touch.begin(1,300,500,true);touch.move(1,300,430);touch.end(1);
  assert.equal(touch.sample().stunt_modifiers,1);assert.equal(touch.sample().stunt_modifiers,0);
  touch.begin(2,300,500,true);touch.move(2,300,430);touch.end(2,true);assert.equal(touch.sample().stunt_modifiers,0);
});

test('small jitter and incomplete tiny circles never select tricks', () => {
  const touch=new TouchInput();touch.begin(1,310,500,true);
  for(let i=1;i<=24;i++)touch.move(1,300+10*Math.cos(i*Math.PI/12),500+10*Math.sin(i*Math.PI/12),i*10);
  assert.equal(touch.sample(1000).stunt_modifiers,0);
});

test('one-handed ground contact braces; forward drives, back brakes, horizontal steers', () => {
  const touch=new TouchInput();touch.setOneHanded(true);touch.begin(1,60,500);
  let s=touch.sample();assert.equal(s.drive.axis,0);assert.equal(s.preload_axis,1);
  touch.move(1,130,430);s=touch.sample();assert.equal(s.drive.axis,1);assert.equal(s.steer,1);assert.equal(s.lean,-1);assert.equal(s.stunt_modifiers,0);
  touch.move(1,-10,570);s=touch.sample();assert.equal(s.drive.axis,-1);assert.equal(s.steer,-1);assert.equal(s.lean,1);
  touch.end(1);assert.equal(touch.sample().preload_axis,-1);
});

test('one-handed air queues drive and retains lean; landing activates without lifting', () => {
  const touch=new TouchInput();touch.setOneHanded(true);touch.begin(1,60,500);touch.move(1,60,430);touch.sample();touch.setAirborne(true);
  let s=touch.sample();assert.equal(s.lean,-1);assert.equal(s.preload_axis,null);assert.equal(s.drive.axis,1);assert.equal(s.drive.grounded_only,true);
  touch.move(1,130,570,0);s=touch.sample(500);assert.equal(s.steer,1);assert.equal(s.lean,1);assert.equal(s.stunt_modifiers,0);
  touch.move(1,60,430);touch.setAirborne(false);s=touch.sample();assert.equal(s.drive.axis,1);assert.equal(s.steer,0);assert.equal(s.lean,-1);
  touch.end(1);assert.equal(touch.sample().preload_axis,-1);
});

test('changing mobile mode cancels old gesture, drive and pending releases', () => {
  const touch=new TouchInput();touch.begin(1,300,500);touch.sample();touch.setOneHanded(true);
  assert.equal(touch.sample().drive.axis,0);assert.equal(touch.sample().preload_axis,null);
  touch.begin(2,300,500,true);touch.move(2,300,430);assert.equal(touch.sample().stunt_modifiers,0);
  touch.setOneHanded(false);assert.equal(touch.sample().stunt_modifiers,0);assert(!touch.owns(2));
});
