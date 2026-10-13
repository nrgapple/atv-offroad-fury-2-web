// Public URL contract for authored gameplay. Never accepts asset URLs or owner tokens.
export const ENVIRONMENTS = ['env01','env02','env03','env04','env05','env06','env07','env08','free05'];
const aliases = {
  map:['map','environment'], quad:['quad','vehicle','atv'], rider:['rider'], player:['player','name'],
  mode:['mode'], activity:['activity'], action:['action'], room:['room','lobby'], play:['play','drive'],
  laps:['laps'], cpu:['cpu'],
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
  if (mode==='single' && ('room' in values || 'action' in values)) errors.push('Room/action requires mode=multiplayer.');
  if (mode==='multiplayer') {
    if (!action) errors.push('Multiplayer needs room=CODE to join, or action=create.');
    if (action==='join' && !room) errors.push('Joining requires room=CODE.');
    if (quadId>26) errors.push('Multiplayer supports ATVs 1–26; motorcycles are Single Player only.');
    if (activity!=='freestyle') errors.push('Multiplayer supports Freeride only.');
  }
  if (activity==='race' && map!=='env01') errors.push('Race preview supports Caribou Peak (env01) only.');
  if (activity!=='race' && ('laps' in values || 'cpu' in values)) errors.push('Laps/cpu requires activity=race.');
  return {requested:true,errors,mode,activity,action,play,map:map??'env01',quadId,riderId,playerName,room,raceSettings:{laps,cpu,difficulty:1}};
}
export function buildRideUrl(origin, setup, {includePlayer = true} = {}) {
  const url = new URL('/play.html',origin);
  const entries = {mode:setup.mode??'single',activity:setup.activity??'freestyle',map:setup.map??'env01',quad:setup.quadId??1,rider:setup.riderId??0};
  if (entries.mode==='multiplayer') { entries.action='join'; entries.room=setup.room; }
  if (includePlayer && setup.playerName) entries.player=setup.playerName;
  if (entries.activity==='race') {entries.laps=setup.raceSettings?.laps??3;entries.cpu=setup.raceSettings?.cpu??3;}
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
