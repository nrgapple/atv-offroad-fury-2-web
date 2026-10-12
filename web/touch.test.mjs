import test from 'node:test';
import assert from 'node:assert/strict';
import { TouchInput, TouchLeanIntent } from './touch.js';

test('primary right up drives, down brakes, hold preloads and release drops drive', () => {
  for(const airborne of [false,true]) {
    const touch=new TouchInput();touch.begin(1,300,500,airborne);
    assert.equal(touch.sample().preload_axis,1);
    touch.move(1,350,430);assert.equal(touch.sample().drive.axis,1);
    touch.move(1,250,570);assert.equal(touch.sample().drive.axis,-1);
    assert.equal(touch.sample().stunt_modifiers,0,'Primary airborne drag is still drive');
    touch.end(1);const released=touch.sample();
    assert.equal(released.drive.held,false);assert.equal(released.drive.axis,0);assert.equal(released.preload_axis,-1);
    assert.equal(touch.sample().preload_axis,null);
  }
});

test('right cancel, blur/reset and unseen tap never emit preload release', () => {
  for(const reason of ['cancel','clear','tap']) {
    const touch=new TouchInput();touch.begin(1);const generation=touch.generation;
    if(reason!=='tap')touch.sample();
    if(reason==='clear')touch.clear();else touch.end(1,reason==='cancel');
    const sample=touch.sample();assert.equal(sample.preload_axis,null);assert.equal(sample.drive.axis,0);
    if(reason!=='tap')assert(touch.generation>generation);
  }
});

test('extra grounded right finger cannot steal throttle or release preload', () => {
  const touch=new TouchInput();touch.begin(1,300,500);touch.move(1,300,430);touch.sample();
  assert(!touch.begin(2,350,500));touch.move(2,350,570);touch.end(2);
  assert.equal(touch.sample().drive.axis,1);assert.equal(touch.sample().preload_axis,1);
});

test('left thumb always steers and leans with exclusive ownership and no drive channel', () => {
  const left=new TouchLeanIntent();assert(left.begin(1,60,500));assert(!left.begin(2,70,510));
  left.move(1,130,570);assert.equal(left.steer,1);assert.equal(left.lean,1);
  left.move(2,0,0);left.end(2);assert.equal(left.lean,1);
  left.move(1,60,430);assert.equal(left.lean,-1);assert.equal(left.steer,0);
  left.end(1);assert.equal(left.lean,0);assert.equal(left.steer,0);
  left.begin(3,50,400);left.clear();assert.equal(left.lean,0);
});

test('secondary airborne right swipes select original tricks independently of throttle', () => {
  for(const [dx,dy,x,y] of [[0,-70,0,-1],[0,70,0,1],[-70,0,-1,0],[70,0,1,0]]) {
    const touch=new TouchInput();touch.begin(1,300,500,true);touch.move(1,300,430);
    assert(touch.begin(2,350,500,true));assert(!touch.begin(3,350,500,true));
    touch.move(2,350+dx,500+dy);let sample=touch.sample();
    assert.equal(sample.drive.axis,1);assert.equal(sample.stunt_modifiers,1);assert.equal(sample.stunt_x,x);assert.equal(sample.stunt_y,y);
    touch.end(2);sample=touch.sample();assert.equal(sample.drive.axis,1);assert.equal(sample.stunt_modifiers,0);assert.equal(sample.preload_axis,1);
  }
});

test('releasing primary right while secondary stunt held drops throttle without dropping trick', () => {
  const touch=new TouchInput();touch.begin(1,300,500,true);touch.move(1,300,430);touch.sample();
  touch.begin(2,350,500,true);touch.move(2,350,430);touch.end(1);
  const sample=touch.sample();assert.equal(sample.drive.held,false);assert.equal(sample.drive.axis,0);assert.equal(sample.preload_axis,-1);assert.equal(sample.stunt_modifiers,1);
  touch.clear();assert.equal(touch.sample().stunt_modifiers,0);
});
