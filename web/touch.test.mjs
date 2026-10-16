import test from 'node:test';
import assert from 'node:assert/strict';
import { TouchInput, TouchLeanIntent, oneHandedFeedback, loadOneHandedPreference, loadSteeringSensitivity, normalizeSteeringSensitivity, DEFAULT_STEERING_SENSITIVITY, TRICK_WHEEL, trickWheelSelection, flipZone } from './touch.js';

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
  assert.deepEqual(oneHandedFeedback(touch), { steer: 0, lean: 0, cue: '', flipZone: null, flipProgress: 0, flipsReady: false });
  assert.equal(touch.sample().preload_axis, -1);
});


test('mobile steering preference has a gentler default and safe persistent bounds', () => {
  assert.equal(DEFAULT_STEERING_SENSITIVITY, 85);
  for (const saved of [null, undefined, '', 'junk', '0', '151']) assert.equal(loadSteeringSensitivity(() => saved), 85);
  assert.equal(loadSteeringSensitivity(() => { throw new Error('denied'); }), 85);
  for (const value of [50, 85, 100, 150]) assert.equal(loadSteeringSensitivity(() => String(value)), value);
  assert.equal(normalizeSteeringSensitivity(1), 50); assert.equal(normalizeSteeringSensitivity(300), 150);
  assert.equal(normalizeSteeringSensitivity(NaN), 85); assert.equal(normalizeSteeringSensitivity(102), 100);
});

test('sensitivity softens both touch modes without capping full lock or changing lean/brace', () => {
  const one = new TouchInput(); one.setOneHanded(true); one.steeringSensitivity = 85; one.begin(1, 100, 500); one.move(1, 135, 535);
  assert.equal(one.sample().steer, .425); assert.equal(one.sample().lean, .5); assert.equal(one.sample().preload_axis, 1);
  assert.equal(oneHandedFeedback(one).steer, .425);
  one.move(1, 200, 535); assert.equal(one.sample().steer, 1, 'larger drag still reaches full lock');
  let sensitivity = 85;
  const two = new TouchLeanIntent(() => sensitivity); two.begin(1, 100, 500); two.move(1, 135, 535);
  assert.equal(two.steer, .425); assert.equal(two.lean, .5);
  sensitivity = 50; assert.equal(two.steer, .25, 'setting applies to existing displacement');
  two.move(1, -40, 535); assert.equal(two.steer, -1, 'low sensitivity still reaches full left lock');
  sensitivity = 150; two.move(1, 135, 535); assert.equal(two.steer, .75); assert.equal(two.lean, .5);
  two.end(1); assert.equal(two.steer, 0); assert.equal(two.lean, 0);
});


test('radial preview is reversible, commits all ten stunts only on release, and preserves primary axes', () => {
  for (const [index, trick] of TRICK_WHEEL.entries()) {
    const t = new TouchInput(); t.setOneHanded(true); t.begin(1, 300, 500, true); t.move(1, 335, 535);
    const baseline = t.sample(); assert(t.begin(2, 100, 500, true)); assert(!t.begin(3, 200, 500, true));
    const { angle } = trick, radius = trick.outer ? 143 : 90;
    t.move(2, 100 + Math.cos(angle) * radius, 500 + Math.sin(angle) * radius);
    assert.equal(t.secondary.pointer.selection.name, trick.name);
    assert.equal(t.sample(10000).stunt_modifiers, 0, 'preview never starts a stunt');
    t.move(2, 100, 500); assert.equal(t.secondary.pointer.selection, null, 'center cancels preview');
    t.move(2, 190, 500); assert.notEqual(t.secondary.pointer.selection, null, 'can change mind');
    t.move(2, 100 + Math.cos(angle) * radius, 500 + Math.sin(angle) * radius);
    t.end(2); const committed = t.sample();
    assert.equal(committed.stunt_modifiers, trick.modifiers); assert.equal(committed.stunt_x, trick.axes[0]); assert.equal(committed.stunt_y, trick.axes[1]);
    assert.equal(committed.steer, baseline.steer); assert.equal(committed.lean, baseline.lean);
    assert.equal(committed.preload_axis, 1); assert.equal(committed.drive.axis, 1); assert(!committed.drive.reverse_held);
    assert.deepEqual(t.sample(), baseline, 'release selection is delivered once'); assert(t.owns(1));
  }
});

test('radial sectors include a neutral center and wrap cleanly at twelve oclock', () => {
  assert.equal(trickWheelSelection(0, 0), null); assert.equal(trickWheelSelection(27, 0), null);
  assert.equal(trickWheelSelection(NaN, 70), null); assert.equal(trickWheelSelection(0, Infinity), null);
  assert.equal(trickWheelSelection(-1, -90).name, 'Nac Nac'); assert.equal(trickWheelSelection(1, -90).name, 'Nac Nac');
});

test('inner sectors match the standard swipe directions; circle stunts sit in the outer band above', () => {
  const at = (degrees, radius = 90) => trickWheelSelection(Math.cos(degrees * Math.PI / 180) * radius, Math.sin(degrees * Math.PI / 180) * radius)?.name;
  for (const [degrees, name] of [[-90, 'Nac Nac'], [-45, 'Point Back'], [0, 'Bar Hop'], [45, 'Cliff Hanger'],
    [90, 'No Footed Can Can'], [135, 'Nothing'], [180, 'Suicide No Hander'], [-135, 'Heel Clicker']]) assert.equal(at(degrees), name);
  assert.equal(at(-60, 145), 'Cat Nac', 'clockwise circle on the clockwise side'); assert.equal(at(-120, 145), 'Cordova');
  assert.equal(at(-90, 145), 'Nac Nac', 'straight up stays Nac Nac past the band gap');
  assert.equal(at(90, 145), 'No Footed Can Can', 'the lower band is empty and falls through');
  assert.equal(at(-60, 131), 'Point Back', 'inside the band stays an inner trick');
  assert.equal(new Set(TRICK_WHEEL.map(t => t.label.length <= 12)).size, 1, 'labels fit one line');
});

test('second ground contact requests reverse without altering brace or primary origin', () => {
  const t=new TouchInput();t.setOneHanded(true);t.begin(1,80,500);t.move(1,115,535);const before=t.sample(),gen=t.generation;
  t.begin(2,300,500);t.move(2,370,570,0);let s=t.sample(120);assert(s.drive.reverse_held);assert.equal(s.stunt_modifiers,0);
  assert.equal(oneHandedFeedback(t).cue,'REVERSE · BRACE');
  t.setAirborne(true);s=t.sample(500);assert.equal(s.stunt_modifiers,0,'ground drag cannot become an air trick');assert.equal(s.steer,before.steer);assert.equal(s.lean,before.lean);
  assert.equal(oneHandedFeedback(t).cue,'DRIVE · BRACE');
  t.move(2,370,500,500);assert.equal(t.secondary.pointer.selection.name,'Nac Nac');assert.equal(t.sample(620).stunt_modifiers,0);
  t.setAirborne(false);s=t.sample(1000);assert.equal(s.stunt_modifiers,0);assert(s.drive.reverse_held);
  t.end(2,true);assert.deepEqual(t.sample(),before);assert.equal(t.generation,gen,'secondary cancel cannot reset primary lean/preload history');
});

test('radial tap, center release, cancellation, primary release and mode changes discard selections', () => {
  const t=new TouchInput();t.setOneHanded(true);t.begin(1,80,500,true);t.sample();t.begin(2,300,500,true);
  t.move(2,302,503,0);assert.equal(t.sample(1000).stunt_modifiers,0);t.end(2);
  t.begin(2,300,500,true);t.move(2,300,430);t.move(2,300,500);t.end(2);assert.equal(t.sample().stunt_modifiers,0);
  t.begin(2,300,500,true);t.move(2,300,430);t.end(2,true);assert.equal(t.sample().stunt_modifiers,0);assert(t.owns(1));
  t.begin(2,300,500,true);t.move(2,300,430);t.end(1);assert(!t.owns(2));let s=t.sample();assert.equal(s.stunt_modifiers,0);assert(!s.drive.held);assert(!s.drive.reverse_held);assert.equal(s.preload_axis,-1);
  t.begin(3,80,500,true);t.begin(4,300,500,true);t.setOneHanded(false);assert(!t.owns(3));assert(!t.owns(4));assert.equal(t.sample().stunt_modifiers,0);
});


test('landing discards both radial preview and unconsumed release, takeoff reanchors held second touch', () => {
  for (const released of [false, true]) {
    const t = new TouchInput(); t.setOneHanded(true); t.begin(1, 80, 500, true); t.begin(2, 300, 500, true);
    t.move(2, 300, 430); if (released) t.end(2);
    t.setAirborne(false); assert.equal(t.sample().stunt_modifiers, 0);
    if (!released) { t.setAirborne(true); assert.equal(t.secondary.pointer.selection, null); t.end(2); assert.equal(t.sample().stunt_modifiers, 0); }
  }
});

test('optional flips: the flip zone lies past the driving range and needs a straight pull', () => {
  assert.equal(flipZone(0, 110).flip, 'backflip'); assert.equal(flipZone(-110, 10).flip, 'left'); assert.equal(flipZone(110, -20).flip, 'right');
  assert.equal(flipZone(0, 80), null, 'full lean back is still just leaning');
  assert.equal(flipZone(-95, 0), null, 'full steering lock is still just steering');
  assert.equal(flipZone(-150, 0, 50), null, 'low sensitivity moves the zone past its later full lock');
  assert.equal(flipZone(-190, 0, 50).flip, 'left');
  assert.equal(flipZone(90, 90), null, 'diagonals never flip'); assert.equal(flipZone(0, -140), null, 'no front flip');
  assert.equal(flipZone(NaN, 140), null);
});

test('optional flips: lifting a thumb held in the flip zone launches once, with hold strength', () => {
  const lift = (flips, dx, dy, heldMs, sensitivity = 100) => {
    const t = new TouchInput(); t.setOneHanded(true); t.flips = flips; t.steeringSensitivity = sensitivity; t.begin(1, 200, 600);
    t.move(1, 200 + dx, 600 + dy, 1000); t.sample(); t.end(1, false, 1000 + heldMs); return [t.sample(), t.sample()];
  };
  let [first, after] = lift(true, 0, 115, 500);
  assert.equal(first.flip_request, 'backflip'); assert.equal(first.flip_launch, true); assert.equal(first.flip_hold, 30);
  assert.equal(first.preload_axis, -1, 'the same lift is still the jump');
  assert.equal(after.flip_request, null, 'delivered once'); assert.equal(after.flip_launch, false);
  assert.equal(lift(true, 0, 115, 4000)[0].flip_hold, 60, 'strength caps at one second');
  assert.equal(lift(true, -115, 0, 300)[0].flip_request, 'left'); assert.equal(lift(true, 115, 10, 300)[0].flip_request, 'right');
  assert.equal(lift(true, 0, 115, 150)[0].flip_request, null, 'sliding into the zone while lifting is not a flip');
  assert.equal(lift(true, 0, 65, 500)[0].flip_request, null, 'a lean-back lift is an ordinary jump');
  assert.equal(lift(true, -90, 0, 2000)[0].flip_request, null, 'lifting out of a hard turn is an ordinary jump');
  assert.equal(lift(false, 0, 115, 500)[0].flip_request, null, 'Flips off');
  const rolled = leaveAfter => {
    const t = new TouchInput(); t.setOneHanded(true); t.flips = true; t.begin(1, 200, 600);
    t.move(1, 200, 715, 1000); t.move(1, 200, 640, 1300); t.end(1, false, 1300 + leaveAfter); return t.sample().flip_request;
  };
  assert.equal(rolled(60), 'backflip', 'a thumb rolling out as it lifts keeps the armed flip');
  assert.equal(rolled(250), null, 'returning toward the centre cancels the flip');
  const cancelled = new TouchInput(); cancelled.setOneHanded(true); cancelled.flips = true; cancelled.begin(1, 200, 600);
  cancelled.move(1, 200, 720, 0); cancelled.end(1, true, 1000); assert.equal(cancelled.sample().flip_request, null, 'cancel never flips');
});

test('optional flips: the thumb cue promises a flip only once armed and when lifting would start one', () => {
  const t = new TouchInput(); t.setOneHanded(true); t.flips = true; t.begin(1, 200, 600); t.move(1, 200, 720, 0);
  const arming = oneHandedFeedback(t, { preload_armed: true }, 110);
  assert.equal(arming.cue, 'LIFT FOR PRELOAD', 'no promise during the dwell'); assert.equal(arming.flipZone, 'backflip'); assert.equal(arming.flipProgress, .5);
  assert.equal(oneHandedFeedback(t, { preload_armed: true }, 300).cue, 'LIFT → BACKFLIP');
  assert.equal(oneHandedFeedback(t, { flip: { launch_window: true } }, 300).cue, 'LIFT → BACKFLIP');
  const flat = oneHandedFeedback(t, { flip: { launch_window: false } }, 300);
  assert.notEqual(flat.cue, 'LIFT → BACKFLIP', 'late air or flat ground'); assert.equal(flat.flipsReady, false); assert.equal(flat.flipZone, null);
  t.move(1, 315, 600, 400); assert.equal(oneHandedFeedback(t, { preload_armed: true }, 700).cue, 'LIFT → 360 RIGHT');
  t.flips = false; assert.equal(oneHandedFeedback(t, { preload_armed: true }, 700).cue, 'LIFT FOR PRELOAD');
  const pad = new TouchLeanIntent(() => 100); pad.begin(1, 100, 500); pad.move(1, 100, 610, 0);
  assert.equal(pad.launchFlip(100), null, 'the pad needs the same dwell');
  assert.deepEqual(pad.launchFlip(250), { flip: 'backflip', hold: 15 }); pad.move(1, 100, 520, 300); assert.equal(pad.launchFlip(450), null);
  const lean = new TouchLeanIntent(() => 100); lean.begin(1, 100, 500); lean.move(1, 100, 575, 0);
  assert.equal(lean.launchFlip(2000), null, 'a full lean on the pad is not a flip');
});

test('optional flips: two-finger long swipes flip, short swipes and up keep the original tricks', () => {
  const swipe = (flips, dx, dy) => {
    const t = new TouchInput(); t.flips = flips; t.begin(1, 300, 500, true);
    for (let i = 1; i <= 10; i++) t.move(1, 300 + dx * i / 10, 500 + dy * i / 10, i * 10);
    return t.sample(1000);
  };
  assert.equal(swipe(true, 0, 130).flip_request, 'backflip');
  assert.equal(swipe(true, -130, 0).flip_request, 'left'); assert.equal(swipe(true, 130, 0).flip_request, 'right');
  const up = swipe(true, 0, -130); assert.equal(up.flip_request, null); assert.equal(up.stunt_modifiers, 1);
  const short = swipe(true, 0, 60); assert.equal(short.flip_request, null); assert.deepEqual([short.stunt_modifiers, short.stunt_y], [1, 1]);
  const off = swipe(false, 0, 130); assert.equal(off.flip_request, null); assert.equal(off.stunt_modifiers, 1);
});
