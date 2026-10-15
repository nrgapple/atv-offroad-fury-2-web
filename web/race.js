// Presentation only: race clocks, rank, checkpoints and CPU poses come from Rust.
export const raceTime = value => value == null ? '--:--.--' : `${Math.floor(value/60)}:${(value%60).toFixed(2).padStart(5,'0')}`;
export class RaceOverlay {
  constructor({restart,quit}) {
    this.restart=restart;this.quit=quit;this.course=null;this.resultShown=false;this.padArmed=false;this.busy=false;
    this.root=document.createElement('section');this.root.id='race-overlay';this.root.hidden=true;
    this.root.innerHTML=`<div class="race-strip"><strong class="race-place"></strong><span class="race-lap"></span><span class="race-total"></span></div><div class="race-times"><span class="race-current"></span><span class="race-best"></span></div><div class="race-guide"><canvas width="140" height="110" aria-label="Race course radar"></canvas><div><strong class="race-direction"></strong><ol class="race-order"></ol></div></div><div class="race-countdown" role="status" aria-live="polite"></div><div class="race-results" hidden><h2>Race Results</h2><p class="race-summary"></p><ol></ol><button type="button" data-race-action="retry">Race Again</button><button type="button" data-race-action="quit">Single Player</button></div>`;
    document.body.append(this.root);this.map=this.root.querySelector('canvas');this.ctx=this.map.getContext('2d');this.results=this.root.querySelector('.race-results');
    this.root.querySelector('[data-race-action=retry]').onclick=()=>void this.act('retry');
    this.root.querySelector('[data-race-action=quit]').onclick=()=>void this.act('quit');
    window.addEventListener('keydown',e=>{if(this.root.hidden||this.results.hidden||e.repeat)return;if(e.code==='Escape'||(e.code==='Enter'&&e.target===document.getElementById('game'))){e.preventDefault();e.stopImmediatePropagation();void this.act(e.code==='Escape'?'quit':'retry');}},true);
  }
  async act(action) {if(this.busy)return;this.busy=true;try{await (action==='retry'?this.restart():this.quit());}catch(error){this.results.hidden=false;this.root.querySelector('.race-summary').textContent=error.message;}finally{this.busy=false;}}
  setCourse(course) {this.course=course;this.resultShown=false;this.padArmed=false;this.root.hidden=true;this.results.hidden=true;this.orderKey='';}
  draw(race,player,visible) {
    this.root.hidden=!race||!visible;
    if(this.root.hidden)return;
    this.root.dataset.phase=race.phase;
    const set=(s,t)=>{const e=this.root.querySelector(s);if(e.textContent!==t)e.textContent=t;};
    set('.race-place',`PLACE ${race.position} / ${race.field}`);set('.race-lap',`LAP ${race.lap} / ${race.laps}`);set('.race-total',`TOTAL ${raceTime(race.elapsed)}`);
    set('.race-current',`LAP TIME ${raceTime(race.lap_time)}`);set('.race-best',`BEST ${raceTime(race.best)}`);
    const [w,x,y,z]=player.orientation,yaw=Math.atan2(2*(w*y+x*z),1-2*(y*y+x*x));
    const dx=race.target[0]-player.origin[0],dz=race.target[2]-player.origin[2];
    const angle=Math.atan2(Math.sin(Math.atan2(dx,dz)-yaw),Math.cos(Math.atan2(dx,dz)-yaw));
    const direction=race.wrong_way?'WRONG WAY':Math.abs(angle)<.25?'↑ STRAIGHT':angle>0?'→ RIGHT':'← LEFT';
    // Off the track polygon progress pauses; point back to the racing line.
    const tx=(race.track_point?.[0]??player.origin[0])-player.origin[0],tz=(race.track_point?.[2]??player.origin[2])-player.origin[2];
    const back=Math.atan2(Math.sin(Math.atan2(tx,tz)-yaw),Math.cos(Math.atan2(tx,tz)-yaw));
    const offTrack=race.off_track?`OFF TRACK ${Math.abs(back)<.4?'↑':back>0?'→':'←'} ${Math.round(Math.hypot(tx,tz))} ft${race.off_track_time>3?' · Reset to return':''}`:'';
    this.root.dataset.offTrack=String(Boolean(race.off_track));
    set('.race-direction',offTrack||`${direction} · ${Math.round(Math.hypot(dx,dz))} ft`);
    set('.race-countdown',race.phase==='countdown'?String(race.countdown):race.elapsed<1?'GO!':'');
    const key=JSON.stringify(race.standings.map(r=>r.name));
    if(key!==this.orderKey){this.orderKey=key;const list=this.root.querySelector('.race-order');list.replaceChildren(...race.standings.map(r=>{const li=document.createElement('li');li.textContent=r.name;li.classList.toggle('is-player',r.name==='YOU');return li;}));}
    this.radar(race,player);
    if(race.phase==='finished') {
      this.results.hidden=false;set('.race-summary',`Place ${race.position} of ${race.field} · ${raceTime(race.elapsed)} · Best ${raceTime(race.best)}`);
      if(!this.resultShown){this.resultShown=true;this.results.querySelector('ol').replaceChildren(...race.standings.map(r=>{const li=document.createElement('li');li.textContent=`${r.name} — ${r.time==null?`${r.laps} laps`:raceTime(r.time)}`;return li;}));this.results.querySelector('button').focus({preventScroll:true});}
      let pad;try{pad=[...navigator.getGamepads()].find(p=>p?.connected!==false&&p);}catch{}
      const held=pad?.buttons[0]?.pressed||pad?.buttons[1]?.pressed;
      if(!held)this.padArmed=true;
      else if(this.padArmed){this.padArmed=false;void this.act(pad.buttons[1]?.pressed?'quit':'retry');}
    } else this.results.hidden=true;
  }
  radar(race,player) {
    if(!this.course)return;
    // Course bounds and the track path are built once per course; each frame
    // only blits them and draws the rider dots (at most 20 Hz).
    const now=performance.now();if(this.radarCourse===this.course&&now-(this.radarTime??0)<50)return;this.radarTime=now;
    if(this.radarCourse!==this.course){
      const points=this.course.points;let minX=Infinity,maxX=-Infinity,minZ=Infinity,maxZ=-Infinity;
      for(const p of points){minX=Math.min(minX,p[0]);maxX=Math.max(maxX,p[0]);minZ=Math.min(minZ,p[2]);maxZ=Math.max(maxZ,p[2]);}
      const scale=Math.min(120/(maxX-minX),90/(maxZ-minZ)),cx=(minX+maxX)/2,cz=(minZ+maxZ)/2;
      this.radarProject=p=>[70+(p[0]-cx)*scale,55-(p[2]-cz)*scale];
      const path=new Path2D();points.forEach((p,i)=>{const [x,y]=this.radarProject(p);i?path.lineTo(x,y):path.moveTo(x,y);});
      // Close onto the lap circuit; a National/Supercross start straight stays open.
      const [lx,ly]=this.radarProject(points[this.course.loop_start??0]);path.lineTo(lx,ly);
      this.radarPath=path;this.radarCourse=this.course;
    }
    const c=this.ctx,project=this.radarProject;
    c.clearRect(0,0,140,110);c.strokeStyle='#c9c9c9';c.lineWidth=3;c.stroke(this.radarPath);
    const dot=(p,color,r)=>{const [x,y]=project(p);c.beginPath();c.arc(x,y,r,0,Math.PI*2);c.fillStyle=color;c.fill();};
    dot(race.target,'#7dd9ff',4);for(const cpu of race.opponents)dot(cpu.origin,'#f67862',3);dot(player.origin,'#ffe375',4);
  }
}
