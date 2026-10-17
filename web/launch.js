// Public URL contract for authored gameplay. Never accepts asset URLs or owner tokens.
// Preserve the first nine lobby IDs for existing rooms and append every venue.
export const ENVIRONMENTS = ['env01','env02','env03','env04','env05','env06','env07','env08','free05',
  'free01','free02','free03','free04','free06',
  ...Array.from({length:8},(_,i)=>`nat${String(i+1).padStart(2,'0')}`),
  ...Array.from({length:16},(_,i)=>`sx${String(i+1).padStart(2,'0')}`),'training','game01'];
// Every Environment, National and Supercross venue ships an original *_NAT.RES race.
export const RACE_COURSES = ENVIRONMENTS.filter(id=>/^(env|nat|sx)\d\d$/.test(id));
export const COURSE_PREVIEWS = ['ANAT','PSTNT','PNAT','ASX','PSX','HOCK'].map(id=>`/UI/RESOURCE/TP/TP_${id}.RES`);
// Original per-course loading-screen action shots (UI/LOADICON, frames 02-07).
// Training ships only the progress-bar art, so it keeps the course photograph.
export function loadingShotsResource(environment) {
  if (!ENVIRONMENTS.includes(environment)) return null;
  const n = environment.slice(-2), family = environment.replace(/\d+$/, '');
  return {env:`/UI/LOADICON/ENV${'AAABBBCC'[Number(n) - 1]}/E${n}_OB.LSI`, free:`/UI/LOADICON/FREE/F${n}_OB.LSI`, nat:`/UI/LOADICON/NAT/N${n}_NAT.LSI`,
    sx:`/UI/LOADICON/SX/S${n}_NAT.LSI`, game:`/UI/LOADICON/GAME/G${n}_SO.LSI`}[family] ?? null;
}
export function courseResource(environment, racing = false) {
  if (!ENVIRONMENTS.includes(environment) || (racing && !RACE_COURSES.includes(environment))) throw new Error('Unsupported course.');
  const number=environment.slice(-2), family=environment.replace(/\d+$/,'');
  const prefix={env:'E',free:'F',nat:'N',sx:'S',game:'G'}[family];
  const group=family==='sx'||family==='training'?'B':'A';
  const suffix=racing?'NAT':family==='training'?'TR':family==='game'?'SO':'FR';
  const stem=family==='training'?'T00':`${prefix}${number}`;
  const sceneStem=family==='env'&&!racing?`${environment}fr`:environment;
  return {archive:`/TRACKS/TRACKS${group}/${environment.toUpperCase()}/${stem}_${suffix}.RES`,
    sceneStem,modeStem:racing?`${environment}nat`:sceneStem,
    placements:family==='env'?`/TRACKS/TRACKSA/ENV${number}/E${number}_ENDL.RES`:null};
}
const aliases = {
  map:['map','environment'], quad:['quad','vehicle','atv'], rider:['rider'], player:['player','name'],
  mode:['mode'], activity:['activity'], action:['action'], room:['room','lobby'], play:['play','drive'],
  laps:['laps'], cpu:['cpu'], difficulty:['difficulty'], cheats:['cheats','cheat'],
};
const modeAliases = {single:'single', 'single-player':'single', singleplayer:'single', sp:'single', multiplayer:'multiplayer', multi:'multiplayer', mp:'multiplayer'};
const activityAliases = {freestyle:'freestyle',freeride:'freestyle',race:'race'};
const slug = (text = '') => text.trim().toLowerCase().replace(/[^a-z0-9]+/g,'-').replace(/^-|-$/g,'');
export function parseLaunch(search, tracks = ENVIRONMENTS.map(id=>({id,name:id}))) {
  const params = new URLSearchParams(search), errors = [], values = {};
  for (const [field,keys] of Object.entries(aliases)) {
    const entries = keys.flatMap(key=>params.getAll(key));
    if (entries.length > 1) errors.push(`Use ${keys[0]} only once (including aliases).`);
    if (entries.length) values[field] = entries[0].trim();
  }
  if (!Object.keys(values).length) return {requested:false,errors};
  const pick = (field, table, fallback) => {
    if (!(field in values)) return fallback;
    const value = table[values[field].toLowerCase()];
    if (value == null) errors.push(`Unsupported ${field}: ${values[field] || '(empty)'}.`);
    return value ?? fallback;
  };
  const integer = (field, allowed, fallback) => {
    if (!(field in values)) return fallback;
    const value = /^\d+$/.test(values[field]) ? Number(values[field]) : NaN;
    if (!allowed(value)) { errors.push(`Invalid ${field}: ${values[field] || '(empty)'}.`); return fallback; }
    return value;
  };
  const mode = pick('mode',modeAliases,('room' in values || 'action' in values)?'multiplayer':'single');
  const activity = pick('activity',activityAliases,'freestyle');
  const action = pick('action',{join:'join',create:'create',host:'create'},'room' in values?'join':null);
  const play = pick('play',{'1':true,true:true,'0':false,false:false},true);
  const mapValue = values.map?.toLowerCase();
  const map = mapValue == null ? 'env01' : tracks.find(t=>t.id===mapValue || slug(t.name)===slug(mapValue))?.id;
  if (!map || !ENVIRONMENTS.includes(map)) errors.push(`Unsupported map: ${values.map || '(empty)'}.`);
  const quadId = integer('quad',v=>v>=1&&v<=46,1);
  const riderId = pick('rider',{'0':0,male:0,'1':1,female:1},0);
  const playerName = values.player ?? 'Rider';
  if (!playerName || new TextEncoder().encode(playerName).length>32 || /[\u0000-\u001f\u007f-\u009f]/.test(playerName)) errors.push('Player name must contain 1–32 UTF-8 bytes without control characters.');
  const room = (values.room ?? '').toUpperCase();
  if ('room' in values && !/^[A-Z0-9]{8}$/.test(room)) errors.push('Room must contain exactly 8 letters or numbers.');
  const laps = integer('laps',v=>v>=1&&v<=5,3), cpu = integer('cpu',v=>[0,3,4].includes(v),3);
  const difficulty = pick('difficulty',{normal:1,'1':1,expert:2,'2':2},1);
  // The original EATDIRT (Aggressive AI) and BRINGIT (All Out AI) race cheats.
  const cheats = {};
  for (const code of ('cheats' in values ? values.cheats.toLowerCase().split(',') : [])) {
    const key = {eatdirt:'aggressive',bringit:'all_out'}[code.trim()];
    if (key) cheats[key] = true; else errors.push(`Unsupported cheat: ${code.trim() || '(empty)'}.`);
  }
  if (mode==='single' && ('room' in values || 'action' in values)) errors.push('Room/action requires mode=multiplayer.');
  if (mode==='multiplayer') {
    if (!action) errors.push('Multiplayer needs room=CODE to join, or action=create.');
    if (action==='join' && !room) errors.push('Joining requires room=CODE.');
    if (quadId>26) errors.push('Multiplayer supports ATVs 1–26; motorcycles are Single Player only.');
    if (activity!=='freestyle') errors.push('Multiplayer supports Freeride only.');
  }
  if (activity==='race' && map && !RACE_COURSES.includes(map)) errors.push('Races use the original Environment, National and Supercross courses.');
  if (activity!=='race' && ['laps','cpu','difficulty','cheats'].some(key=>key in values)) errors.push('Laps/cpu/difficulty/cheats requires activity=race.');
  return {requested:true,errors,mode,activity,action,play,map:map??'env01',quadId,riderId,playerName,room,raceSettings:{laps,cpu,difficulty,course:map??'env01',...cheats}};
}
// App shortcuts (manifest "shortcuts") open a menu with the saved rider, quad
// and race setup. Unlike ride links, they never replace the player's selection.
const MENU_SHORTCUTS = {freeride:'tracks', race:'race'};
export function menuShortcut(search) {
  const params = new URLSearchParams(search), keys = [...params.keys()];
  if (keys.length !== 1 || keys[0] !== 'menu') return null;
  return MENU_SHORTCUTS[params.get('menu').toLowerCase()] ?? null;
}
export function buildRideUrl(origin, setup, {includePlayer = true} = {}) {
  const url = new URL('/play.html',origin);
  const entries = {mode:setup.mode??'single',activity:setup.activity??'freestyle',map:setup.map??'env01',quad:setup.quadId??1,rider:setup.riderId??0};
  if (entries.mode==='multiplayer') { entries.action='join'; entries.room=setup.room; }
  if (includePlayer && setup.playerName) entries.player=setup.playerName;
  if (entries.activity==='race') {
    entries.laps=setup.raceSettings?.laps??3;entries.cpu=setup.raceSettings?.cpu??3;
    if (setup.raceSettings?.difficulty===2) entries.difficulty='expert';
    const cheats=[setup.raceSettings?.aggressive&&'eatdirt',setup.raceSettings?.all_out&&'bringit'].filter(Boolean);
    if (cheats.length) entries.cheats=cheats.join(',');
  }
  entries.play=1;
  for(const [key,value] of Object.entries(entries)) if(value!=null)url.searchParams.set(key,String(value));
  // Share only valid supported setups, never the current page's unrelated query/hash.
  const parsed=parseLaunch(url.search);
  if(parsed.errors.length)throw Error(parsed.errors.join(' '));
  return url.href;
}
export async function copyRideUrl(url, notice, label = 'Ride link') {
  try { await navigator.clipboard.writeText(url); notice.textContent=`${label} copied.`; }
  catch { notice.textContent=url; }
}
