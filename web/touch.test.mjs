import test from 'node:test';
import assert from 'node:assert/strict';
import { TouchInput, TouchLeanIntent, oneHandedFeedback, loadOneHandedPreference } from './touch.js';

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

test('one-handed contact holds throttle and brace; four directions only steer/lean', () => {
  const touch=new TouchInput();touch.setOneHanded(true);touch.begin(1,60,500);
  for(const [dx,dy,x,y] of [[0,0,0,0],[70,0,1,0],[-70,0,-1,0],[0,-70,0,-1],[0,70,0,1]]) {
    touch.move(1,60+dx,500+dy);const s=touch.sample();
    assert.equal(s.drive.axis,1);assert.equal(s.drive.held,true);assert.equal(s.drive.grounded_only,false);assert.equal(s.drive.forward_only,true);
    assert.equal(s.preload_axis,1);assert.equal(s.steer,x);assert.equal(s.lean,y);assert.equal(s.stunt_modifiers,0);
  }
  touch.end(1);let s=touch.sample();assert.equal(s.drive.axis,0);assert.equal(s.drive.held,false);assert.equal(s.preload_axis,-1);
  assert.equal(touch.sample().preload_axis,null);
});

test('one-handed controls and brace remain continuous through takeoff and landing', () => {
  const touch=new TouchInput();touch.setOneHanded(true);touch.begin(1,60,500);touch.move(1,130,570);
  const baseline=touch.sample();const generation=touch.generation;
  for(const airborne of [true,false,true]) {
    touch.setAirborne(airborne);assert.deepEqual(touch.sample(),baseline);assert.equal(touch.generation,generation);
  }
  touch.end(1);const released=touch.sample();assert.equal(released.drive.axis,0);assert.equal(released.preload_axis,-1);
});

test('changing mobile mode cancels old gesture, drive and pending releases', () => {
  const touch=new TouchInput();touch.begin(1,300,500);touch.sample();touch.setOneHanded(true);
  assert.equal(touch.sample().drive.axis,0);assert.equal(touch.sample().preload_axis,null);
  touch.begin(2,300,500,true);touch.move(2,300,430);assert.equal(touch.sample().stunt_modifiers,0);
  touch.setOneHanded(false);assert.equal(touch.sample().stunt_modifiers,0);assert(!touch.owns(2));
});



test('one-finger default respects saved mode and storage denial', () => {
  for (const value of [null, undefined, 'true', 'invalid']) assert.equal(loadOneHandedPreference(() => value), true);
  assert.equal(loadOneHandedPreference(() => 'false'), false);
  assert.equal(loadOneHandedPreference(() => { throw new Error('storage unavailable'); }), true);
});

test('compact feedback follows cardinal and diagonal commands without consuming releases', () => {
  const touch = new TouchInput(); touch.setOneHanded(true); touch.begin(1, 100, 500);
  for (const [dx, dy] of [[0,0], [70,0], [-70,0], [0,-70], [0,70], [35,-35], [-70,-70], [70,70], [-70,70]]) {
    touch.move(1, 100 + dx, 500 + dy);
    const state = oneHandedFeedback(touch);
    const command = touch.sample();
    assert.equal(state.steer, command.steer); assert.equal(state.lean, command.lean);
    assert.equal(state.cue, 'DRIVE · BRACE');
    touch.setAirborne(true); assert.deepEqual(oneHandedFeedback(touch), state); touch.setAirborne(false);
  }
  assert.equal(oneHandedFeedback(touch, { preload_armed: true }).cue, 'LIFT FOR PRELOAD');
  assert.equal(oneHandedFeedback(touch, { preload_active: true }).cue, 'PRELOADED');
  const applied_input = { touch_drive: { generation: touch.generation, held: true }, brake: 1 };
  assert.equal(oneHandedFeedback(touch, { applied_input }).cue, 'BRAKE · BRACE');
  applied_input.touch_drive.generation--;
  assert.equal(oneHandedFeedback(touch, { applied_input }).cue, 'DRIVE · BRACE');
  touch.end(1);
  assert.deepEqual(oneHandedFeedback(touch), { steer: 0, lean: 0, cue: '' });
  assert.equal(touch.sample().preload_axis, -1);
});
