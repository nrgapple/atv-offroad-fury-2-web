// Gameplay mappings. Gesture adaptation stays in touch.js; native force gates stay in Rust.
export const ACTIONS = [
  ['throttle','Throttle'], ['brake','Brake'], ['steerLeft','Steer left'], ['steerRight','Steer right'],
  ['leanForward','Lean forward'], ['leanBack','Lean back'], ['reverse','Reverse'],
  ['stuntCircle','Stunt Circle'], ['stuntTriangle','Stunt Triangle'], ['stuntL1','Stunt L1'],
  ['preloadHold','Preload hold / release'], ['camera','Change camera'],
  ['cameraLeft','Camera orbit left'], ['cameraRight','Camera orbit right'], ['reset','Reset ATV'], ['pause','Pause'],
].map(([id,label])=>Object.freeze({id,label}));
export const STORAGE_KEY = 'atv2-gameplay-bindings-v1';
const key = key => ({key}), button = button => ({button}), axis = (axis,sign) => ({axis,sign});
const DEFAULTS = {
  keyboard: { throttle:[key('KeyW')], brake:[key('KeyS')], steerLeft:[key('KeyA'),key('ArrowLeft')], steerRight:[key('KeyD'),key('ArrowRight')], leanForward:[key('KeyQ'),key('ArrowUp')], leanBack:[key('KeyE'),key('ArrowDown')], reverse:[key('KeyR')], stuntCircle:[key('KeyJ')], stuntTriangle:[key('KeyK')], stuntL1:[key('KeyL')], preloadHold:[key('Space')], camera:[key('KeyC')], cameraLeft:[],cameraRight:[],reset:[key('Backspace')],pause:[key('Escape')] },
  controller: {throttle:[button(0),button(7)],brake:[button(2),button(6)],steerLeft:[axis(0,-1),button(14)],steerRight:[axis(0,1),button(15)],leanForward:[axis(1,-1),button(12)],leanBack:[axis(1,1),button(13)],reverse:[{chord:[button(0),button(2)]}],stuntCircle:[button(1)],stuntTriangle:[button(3)],stuntL1:[button(4)],preloadHold:[],camera:[button(11)],cameraLeft:[axis(2,-1)],cameraRight:[axis(2,1)],reset:[{chord:[button(4),button(5)]}],pause:[button(9)]},
};
const clone = value => JSON.parse(JSON.stringify(value));
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
function amount(binding,keys,pad) {
  if(binding.chord) return Math.min(...binding.chord.map(b=>amount(b,keys,pad)));
  if(binding.key) return Number(keys?.has(binding.key)??false);
  if('button' in binding) return clamp(pad?.buttons?.[binding.button]?.value ?? Number(Boolean(pad?.buttons?.[binding.button]?.pressed)));
  const value=clamp((pad?.axes?.[binding.axis]??0)*binding.sign);
  return value<=.15?0:(value-.15)/.85;
}
export function bindingLabel(bindings) {
  const atom=b=>b.key ? b.key.replace(/^Key/,'').replace(/^Digit/,'') : 'button' in b ? `Button ${b.button}` : `Axis ${b.axis} ${b.sign<0?'-':'+'}`;
  return bindings.length ? bindings.map(b=>b.chord?b.chord.map(atom).join(' + '):atom(b)).join(' / ') : 'Unbound';
}
export class GameplayBindings {
  constructor(storage) {
    if(storage===undefined) {try{storage=globalThis.localStorage;}catch{storage=null;}}
    this.storage=storage;this.profile=clone(DEFAULTS);this.previous=new Set();this.blocked=new Set();this.preloadHeld=false;this.storageError=null;this.listeners=new Set();this.controllerIdentity=undefined;this.generation=0;this.keyboardHeld=new Set();this.queuedActions=new Set();
    try {
      const saved=JSON.parse(storage?.getItem(STORAGE_KEY)??'null');
      if(saved) for(const device of ['keyboard','controller']) for(const {id} of ACTIONS) this.profile[device][id]=normalize(device,saved[device][id]);
    } catch { this.profile=clone(DEFAULTS); }
  }
  save() { try {this.storage?.setItem(STORAGE_KEY,JSON.stringify(this.profile));this.storageError=null;}catch{this.storageError='Settings work for this session; browser storage is unavailable.';} }
  subscribe(listener) {this.listeners.add(listener);return()=>this.listeners.delete(listener);}
  changed() {this.cancel();this.save();for(const listener of this.listeners)listener();}
  rows(device) { if(!this.profile[device])throw Error('Unknown input device');return ACTIONS.map(a=>({...a,binding:clone(this.profile[device][a.id]),labelBinding:bindingLabel(this.profile[device][a.id])})); }
  setBinding(device,id,binding) { if(!ids.has(id))throw Error('Unknown control');const next=normalize(device,binding);this.profile[device][id]=next;this.changed(); }
  reset(device) {if(!this.profile[device])throw Error('Unknown input device');this.profile[device]=clone(DEFAULTS[device]);this.changed();}
  value(device,id,keys,pad) {return Math.max(0,...(this.profile[device]?.[id]??[]).map(b=>amount(b,keys,pad)));}
  usesKey(code) {return Object.values(this.profile.keyboard).flat().some(b=>(b.chord??[b]).some(a=>a.key===code));}
  cancel() {this.generation++;this.previous.clear();this.keyboardHeld.clear();this.queuedActions.clear();this.preloadHeld=false;this.blocked=new Set(['camera','reset','pause','preloadHold']);}
  // Keyboard events can begin and end between physics batches. Queue only the
  // discrete actions; sampling here must never consume a preload release.
  observeKeys(keys, pressedCode = null) {
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
    let brake=0;
    let reverseChord=false;
    for(const device of ['keyboard','controller']) {
      const consumed=this.profile[device].reverse.filter(b=>b.chord&&amount(b,keys,pad)>.5).flatMap(b=>b.chord.map(a=>JSON.stringify(a)));
      reverseChord ||= consumed.length>0;
      for(const binding of this.profile[device].brake) if(!(binding.chord??[binding]).every(a=>consumed.includes(JSON.stringify(a)))) brake=Math.max(brake,amount(binding,keys,pad));
    }
    const x=values.steerRight-values.steerLeft,y=values.leanBack-values.leanForward;
    const input={throttle:values.throttle,brake,steer:x,lean:y,reverse:values.reverse>.5,stunt_modifiers:Number(values.stuntCircle>.5)|(Number(values.stuntTriangle>.5)<<1)|(Number(values.stuntL1>.5)<<2),stunt_x:x,stunt_y:y};
    if(reverseChord)input.reverse_chord_brake=values.brake;
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
