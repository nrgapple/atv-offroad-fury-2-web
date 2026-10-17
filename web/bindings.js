// Gameplay mappings. Gesture adaptation stays in touch.js; native force gates stay in Rust.
import { bindingText } from './pad-glyphs.js';
import { controllerSettings } from './gamepads.js';
export const ACTIONS = [
  ['throttle','Gas'], ['brake','Brake'], ['steerLeft','Steer left'], ['steerRight','Steer right'],
  ['leanForward','Lean forward'], ['leanBack','Lean back'], ['reverse','Reverse'],
  ['stuntCircle','Stunt Circle'], ['stuntTriangle','Stunt Triangle'], ['stuntL1','Stunt L1'],
  ['clutch','Motorcycle clutch'], ['preloadHold','Preload hold / release'], ['camera','Change camera'],
  ['cameraLeft','Camera orbit left'], ['cameraRight','Camera orbit right'], ['reset','Reset ATV'], ['pause','Pause'],
].map(([id,label])=>Object.freeze({id,label}));
// Keyboard brake resolves through Rust's drive adapter: brake while rolling
// forward, then reverse from a stop. The reverse row is an optional extra key.
const KEYBOARD_LABELS = {brake:'Brake / reverse', reverse:'Reverse (extra key)', stuntCircle:'Trick ○ (hold + direction)', stuntTriangle:'Trick △ (hold + direction)', stuntL1:'Trick L1 (combine)'};
export const STORAGE_KEY = 'atv2-gameplay-bindings-v1';
const PROFILE_VERSION = 2;
// A held direction pressed before the trick key waits this long for a fresh
// direction, so holding W for gas does not force every trick to Up.
export const TRICK_DIRECTION_GRACE_MS = 150;
const key = key => ({key}), button = button => ({button}), axis = (axis,sign) => ({axis,sign});
const DEFAULTS = {
  keyboard: { throttle:[key('KeyW'),key('ArrowUp')], brake:[key('KeyS'),key('ArrowDown')], steerLeft:[key('KeyA'),key('ArrowLeft')], steerRight:[key('KeyD'),key('ArrowRight')], leanForward:[key('KeyQ')], leanBack:[key('KeyE'),key('ShiftLeft')], reverse:[], stuntCircle:[key('KeyJ')], stuntTriangle:[key('KeyK')], stuntL1:[key('KeyL')], clutch:[key('KeyV')], preloadHold:[key('Space')], camera:[key('KeyC')], cameraLeft:[key('KeyZ')],cameraRight:[key('KeyX')],reset:[key('KeyR'),key('Backspace')],pause:[key('Escape'),key('KeyP')] },
  controller: {throttle:[button(0),button(7)],brake:[button(2),button(6)],steerLeft:[axis(0,-1),button(14)],steerRight:[axis(0,1),button(15)],leanForward:[axis(1,-1),button(12)],leanBack:[axis(1,1),button(13)],reverse:[{chord:[button(0),button(2)]}],stuntCircle:[button(1)],stuntTriangle:[button(3)],stuntL1:[button(4)],clutch:[button(10)],preloadHold:[],camera:[button(11)],cameraLeft:[axis(2,-1)],cameraRight:[axis(2,1)],reset:[{chord:[button(4),button(5)]}],pause:[button(9)]},
};
// Controller presets. Fury Remastered is the port default. The others follow
// each original game's default layout: ATV Offroad Fury 2's Controller
// Settings Preset 1 (PAGE17) and MX vs. ATV Unleashed's Preset 1
// (presets_driving_1 / presets_stunt_1 / presets_camera_1). Their unported
// cameras (Look Back, Thrill, Dolly, Zoom) have no action here; see CONTROLS.md.
const PAD_COMMON = {steerLeft:[axis(0,-1),button(14)],steerRight:[axis(0,1),button(15)],leanForward:[axis(1,-1),button(12)],leanBack:[axis(1,1),button(13)],
  throttle:[button(0)],brake:[button(2)],reverse:[{chord:[button(2),button(0)]}],stuntCircle:[button(1)],stuntTriangle:[button(3)],preloadHold:[],
  camera:[button(8)],cameraLeft:[axis(2,-1)],cameraRight:[axis(2,1)],pause:[button(9)]};
export const CONTROLLER_PRESETS = Object.freeze({
  remastered: {label:'Fury Remastered', profile:DEFAULTS.controller},
  // Original: L1 is the third stunt modifier and L1 + R1 resets; no clutch, so L3 keeps the motorcycle clutch.
  atv2: {label:'ATV Offroad Fury 2', profile:{...PAD_COMMON,stuntL1:[button(4)],clutch:[button(10)],reset:[{chord:[button(4),button(5)]}]}},
  // Unleashed: R1 is the stunt modifier and L1 the clutch; it shows no reset button.
  unleashed: {label:'MX vs. ATV Unleashed', profile:{...PAD_COMMON,stuntL1:[button(5)],clutch:[button(4)],reset:[]}},
});
const sameProfile = (a,b) => ACTIONS.every(({id})=>JSON.stringify(a[id])===JSON.stringify(b[id]));
// Version 1 keyboard defaults. Saved rows still equal to these adopt the new
// defaults; rows a player customized are kept exactly.
const V1_KEYBOARD = { throttle:[key('KeyW')], brake:[key('KeyS')], steerLeft:[key('KeyA'),key('ArrowLeft')], steerRight:[key('KeyD'),key('ArrowRight')], leanForward:[key('KeyQ'),key('ArrowUp')], leanBack:[key('KeyE'),key('ArrowDown')], reverse:[key('KeyR')], stuntCircle:[key('KeyJ')], stuntTriangle:[key('KeyK')], stuntL1:[key('KeyL')], clutch:[key('KeyV')], preloadHold:[key('Space')], camera:[key('KeyC')], cameraLeft:[],cameraRight:[],reset:[key('Backspace')],pause:[key('Escape')] };
const clone = value => JSON.parse(JSON.stringify(value));
const atoms = binding => binding.chord ?? [binding];
function migrateKeyboard(saved) {
  const profile={}, adopted=[];
  for(const {id} of ACTIONS) {
    const row=normalize('keyboard',saved?.[id] ?? DEFAULTS.keyboard[id]);
    if(JSON.stringify(row)===JSON.stringify(V1_KEYBOARD[id])) {profile[id]=clone(DEFAULTS.keyboard[id]);adopted.push(id);}
    else profile[id]=row;
  }
  // A customized row keeps its key; a newly adopted default drops that key.
  const custom=new Set(ACTIONS.filter(({id})=>!adopted.includes(id)).flatMap(({id})=>profile[id].flatMap(atoms).map(a=>a.key)));
  for(const id of adopted) profile[id]=profile[id].filter(b=>atoms(b).every(a=>!custom.has(a.key)));
  return profile;
}
const ids = new Set(ACTIONS.map(a=>a.id));
const clamp = value => Number.isFinite(value) ? Math.max(0,Math.min(1,value)) : 0;
function validAtom(device, atom) {
  if(!atom || typeof atom!=='object' || Array.isArray(atom)) return false;
  const fields=Object.keys(atom);
  if(device==='keyboard') return fields.length===1 && typeof atom.key==='string' && /^[A-Za-z][A-Za-z0-9]{0,39}$/.test(atom.key);
  if(fields.length===1 && Number.isInteger(atom.button)) return atom.button>=0&&atom.button<64;
  return fields.length===2 && Number.isInteger(atom.axis)&&atom.axis>=0&&atom.axis<16&&(atom.sign===-1||atom.sign===1);
}
function validBinding(device,binding) {
  return validAtom(device,binding) || (binding && Object.keys(binding).length===1 && Array.isArray(binding.chord) && binding.chord.length>=2&&binding.chord.length<=4&&binding.chord.every(a=>validAtom(device,a)) && new Set(binding.chord.map(a=>JSON.stringify(a))).size===binding.chord.length);
}
function normalize(device,bindings) {
  if(!['keyboard','controller'].includes(device)) throw Error('Unknown input device');
  const values=Array.isArray(bindings)?bindings:[bindings];
  if(values.length>4||!values.every(b=>validBinding(device,b))) throw Error('Invalid input binding');
  return clone(values);
}
function amount(binding,keys,pad,zone=.15) {
  if(binding.chord) return Math.min(...binding.chord.map(b=>amount(b,keys,pad,zone)));
  if(binding.key) return Number(keys?.has(binding.key)??false);
  if('button' in binding) return clamp(pad?.buttons?.[binding.button]?.value ?? Number(Boolean(pad?.buttons?.[binding.button]?.pressed)));
  const value=clamp((pad?.axes?.[binding.axis]??0)*binding.sign);
  return value<=zone?0:(value-zone)/(1-zone);
}
export function bindingLabel(bindings) {
  const atom=b=>b.key ? b.key.replace(/^Key/,'').replace(/^Digit/,'') : 'button' in b ? `Button ${b.button}` : `Axis ${b.axis} ${b.sign<0?'-':'+'}`;
  return bindings.length ? bindings.map(b=>b.chord?b.chord.map(atom).join(' + '):atom(b)).join(' / ') : 'Unbound';
}
export class GameplayBindings {
  constructor(storage) {
    if(storage===undefined) {try{storage=globalThis.localStorage;}catch{storage=null;}}
    this.storage=storage;this.profile=clone(DEFAULTS);this.customController=null;this.previous=new Set();this.blocked=new Set();this.preloadHeld=false;this.storageError=null;this.listeners=new Set();this.controllerIdentity=undefined;this.generation=0;this.keyboardHeld=new Set();this.queuedActions=new Set();this.pressed=new Map();this.pressCount=0;this.clock=()=>globalThis.performance?.now?.()??Date.now();
    try {
      const saved=JSON.parse(storage?.getItem(STORAGE_KEY)??'null');
      if(saved) {
        for(const device of ['keyboard','controller']) for(const {id} of ACTIONS) this.profile[device][id]=normalize(device,saved[device][id] ?? DEFAULTS[device][id]);
        if(saved.customController) try {this.customController=Object.fromEntries(ACTIONS.map(({id})=>[id,normalize('controller',saved.customController[id]??[])]));} catch {this.customController=null;}
        if(saved.version!==PROFILE_VERSION) {this.profile.keyboard=migrateKeyboard(saved.keyboard);this.save();}
      }
    } catch { this.profile=clone(DEFAULTS); }
  }
  save() { try {this.storage?.setItem(STORAGE_KEY,JSON.stringify({...this.profile,...(this.customController?{customController:this.customController}:{}),version:PROFILE_VERSION}));this.storageError=null;}catch{this.storageError='Settings work for this session; browser storage is unavailable.';} }
  subscribe(listener) {this.listeners.add(listener);return()=>this.listeners.delete(listener);}
  changed() {this.cancel();this.save();for(const listener of this.listeners)listener();}
  // Controller rows use standard-layout names (Cross, R2, Left stick left);
  // pass standard=false for a pad that reports a non-standard mapping.
  rows(device,standard=true) { if(!this.profile[device])throw Error('Unknown input device');return ACTIONS.map(a=>({...a,label:(device==='keyboard'&&KEYBOARD_LABELS[a.id])||a.label,binding:clone(this.profile[device][a.id]),labelBinding:device==='controller'?bindingText(this.profile[device][a.id],standard):bindingLabel(this.profile[device][a.id])})); }
  setBinding(device,id,binding) { if(!ids.has(id))throw Error('Unknown control');const next=normalize(device,binding);this.profile[device][id]=next;this.changed(); }
  reset(device) {if(!this.profile[device])throw Error('Unknown input device');if(device==='controller')return this.applyControllerPreset('remastered');this.profile[device]=clone(DEFAULTS[device]);this.changed();}
  /** 'remastered', 'atv2', 'unleashed', or 'custom' for edited bindings. */
  controllerPreset() {return Object.keys(CONTROLLER_PRESETS).find(id=>sameProfile(this.profile.controller,CONTROLLER_PRESETS[id].profile))??'custom';}
  controllerPresetLabel() {const id=this.controllerPreset();return id==='custom'?'Custom':CONTROLLER_PRESETS[id].label;}
  // Choosing a preset keeps edited bindings as "Custom" so they can be chosen again.
  applyControllerPreset(id) {
    if(id!=='custom'&&!CONTROLLER_PRESETS[id])throw Error('Unknown controller preset');
    if(id==='custom'&&!this.customController)return;
    if(this.controllerPreset()==='custom')this.customController=clone(this.profile.controller);
    this.profile.controller=clone(id==='custom'?this.customController:CONTROLLER_PRESETS[id].profile);this.changed();
  }
  cycleControllerPreset(direction=1) {
    const options=[...Object.keys(CONTROLLER_PRESETS),...(this.customController||this.controllerPreset()==='custom'?['custom']:[])];
    const at=Math.max(0,options.indexOf(this.controllerPreset()));
    this.applyControllerPreset(options[(at+direction+options.length)%options.length]);
  }
  // Controller values use the Controller Settings dead zone and response
  // curves (identity at the defaults); keyboard keys are unaffected.
  value(device,id,keys,pad) {
    if(device!=='controller'||!this.tuning)return Math.max(0,...(this.profile[device]?.[id]??[]).map(b=>amount(b,keys,pad)));
    return this.tuning.shape(id,Math.max(0,...(this.profile.controller[id]??[]).map(b=>amount(b,keys,pad,this.tuning.zone))));
  }
  controllerAmount(binding,keys,pad) {return amount(binding,keys,pad,this.tuning?.zone);}
  usesKey(code) {return Object.values(this.profile.keyboard).flat().some(b=>(b.chord??[b]).some(a=>a.key===code));}
  cancel() {this.generation++;this.previous.clear();this.keyboardHeld.clear();this.queuedActions.clear();this.preloadHeld=false;this.blocked=new Set(['camera','reset','pause','preloadHold']);}
  // Keyboard events can begin and end between physics batches. Queue only the
  // discrete actions; sampling here must never consume a preload release.
  observeKeys(keys, pressedCode = null) {
    if(pressedCode)this.pressed.set(pressedCode,{order:++this.pressCount,time:this.clock()});
    for(const id of ['camera','reset','pause']) {
      const held=this.value('keyboard',id,keys,null)>.5;
      const fresh=pressedCode&&this.profile.keyboard[id].some(b=>(b.chord??[b]).some(a=>a.key===pressedCode));
      // A fresh keyboard keydown is known to be a new press even if it arrives
      // immediately after Resume, before a neutral physics sample. Controller
      // buttons still need that neutral sample because they have no key events.
      if(fresh&&held&&!this.keyboardHeld.has(id))this.queuedActions.add(id);
      if(held)this.keyboardHeld.add(id);else this.keyboardHeld.delete(id);
    }
    if(pressedCode&&this.profile.keyboard.preloadHold.some(b=>(b.chord??[b]).some(a=>a.key===pressedCode))&&this.value('keyboard','preloadHold',keys,null)>.5)this.blocked.delete('preloadHold');
  }
  // Keyboard tricks: hold a trick key, then tap a direction. Gas/brake keys
  // count as Up/Down here. The most recently pressed held direction wins.
  keyboardTrickAxes(keys) {
    const held=id=>this.profile.keyboard[id].flatMap(atoms).filter(a=>keys?.has(a.key)).map(a=>this.pressed.get(a.key)??{order:-1,time:-Infinity});
    const modifiers=['stuntCircle','stuntTriangle','stuntL1'].flatMap(held);
    if(!modifiers.length)return null;
    let best=null;
    for(const [id,dx,dy] of [['leanForward',0,-1],['throttle',0,-1],['leanBack',0,1],['brake',0,1],['reverse',0,1],['steerLeft',-1,0],['steerRight',1,0]])
      for(const press of held(id)) if(!best||press.order>best.press.order) best={press,axes:[dx,dy]};
    if(!best)return null;
    const modifierAt=Math.max(...modifiers.map(p=>p.time));
    return best.press.time<modifierAt&&this.clock()-modifierAt<TRICK_DIRECTION_GRACE_MS?[0,0]:best.axes;
  }
  read(keys,pad) {
    if(pad?.connected===false)pad=null;
    const controllerIdentity=pad?`${pad.index??0}:${pad.id??''}`:null;
    // Losing or switching a controller is cancellation, never a preload flick.
    // Wait for held edge controls to return to neutral before rearming them.
    if(this.controllerIdentity!==undefined&&this.controllerIdentity!==controllerIdentity)this.cancel();
    this.controllerIdentity=controllerIdentity;
    const values=Object.fromEntries(ACTIONS.map(({id})=>[id,Math.max(this.value('keyboard',id,keys,pad),this.value('controller',id,keys,pad))]));
    // Preserve the chord's full brake for Rust's native speed gate. Other
    // brake alternatives (e.g. trigger or keyboard) remain independent.
    // Controller drive keeps the native face-button chord. Keyboard gas and
    // brake/reverse go to Rust's keyboard adapter, which knows the fixed-step
    // speed: S brakes while rolling forward and reverses once stopped.
    const pd=id=>this.value('controller',id,keys,pad),kb=id=>this.value('keyboard',id,keys,null);
    let brake=0;
    const consumed=this.profile.controller.reverse.filter(b=>b.chord&&this.controllerAmount(b,keys,pad)>.5).flatMap(b=>b.chord.map(a=>JSON.stringify(a)));
    for(const binding of this.profile.controller.brake) if(!atoms(binding).every(a=>consumed.includes(JSON.stringify(a)))) brake=Math.max(brake,this.tuning?this.tuning.shape('brake',this.controllerAmount(binding,keys,pad)):amount(binding,keys,pad));
    const kbGas=kb('throttle')>.5,kbBack=Math.max(kb('brake'),kb('reverse'))>.5;
    const x=values.steerRight-values.steerLeft,y=values.leanBack-values.leanForward;
    const mask=Number(values.stuntCircle>.5)|(Number(values.stuntTriangle>.5)<<1)|(Number(values.stuntL1>.5)<<2);
    const [stuntX,stuntY]=this.keyboardTrickAxes(keys)??[x,y];
    const input={clutch:values.clutch>.5,throttle:pd('throttle'),brake,steer:x,lean:y,reverse:pd('reverse')>.5,stunt_modifiers:mask,stunt_x:stuntX,stunt_y:stuntY,
      keyboard_drive:{generation:this.generation,held:kbGas||kbBack,axis:kbBack?-1:kbGas?1:0}};
    if(consumed.length)input.reverse_chord_brake=pd('brake');
    const actions={};
    for(const id of ['camera','reset','pause']) {
      const held=values[id]>.5;
      if(!held)this.blocked.delete(id);
      actions[id]=this.queuedActions.delete(id)||(held&&!this.previous.has(id)&&!this.blocked.has(id));
      if(held)this.previous.add(id);else this.previous.delete(id);
    }
    const preload=values.preloadHold>.5;
    if(!preload)this.blocked.delete('preloadHold');
    if(!this.blocked.has('preloadHold')) {
      if(preload)input.preload_axis=1;else if(this.preloadHeld)input.preload_axis=-1;
      this.preloadHeld=preload;
    }
    return {input,actions,orbit:values.cameraRight-values.cameraLeft};
  }
}
export const gameplayBindings = new GameplayBindings();
gameplayBindings.tuning = controllerSettings;
