// Presentation only: race clocks, rank, checkpoints and CPU poses come from Rust.
export const raceTime = value => value == null ? '--:--.--' : `${Math.floor(value/60)}:${(value%60).toFixed(2).padStart(5,'0')}`;
export class RaceOverlay {
  constructor({restart,quit,finish}) {
    this.restart=restart;this.quit=quit;this.finish=finish;this.course=null;this.resultShown=false;this.padArmed=false;this.busy=false;
    this.root=document.createElement('section');this.root.id='race-overlay';this.root.hidden=true;
    this.root.innerHTML=`<div class="race-strip"><strong class="race-place"></strong><span class="race-lap"></span><span class="race-total"></span></div><div class="race-times"><span class="race-current"></span><span class="race-best"></span></div><div class="race-guide"><canvas width="140" height="110" aria-label="Race course radar"></canvas><div><strong class="race-direction"></strong><ol class="race-order"></ol></div></div><div class="race-countdown" role="status" aria-live="polite"></div><div class="race-results" hidden><h2>Race Results</h2><p class="race-summary"></p><table><thead><tr><th scope="col">Rider</th><th scope="col">Total Time</th><th scope="col">Best Lap</th></tr></thead><tbody></tbody></table><div class="race-menu" data-menu="results"><button type="button" data-race-action="continue">Continue</button><button type="button" data-race-action="hide">Hide Menu</button></div><div class="race-menu" data-menu="quit" hidden><button type="button" data-race-action="retry">Restart</button><button type="button" data-race-action="quit">Quit Event</button></div></div><button type="button" class="race-show" data-race-action="show" hidden>Show Results</button><div class="race-fade" aria-hidden="true"></div>`;
    document.body.append(this.root);this.map=this.root.querySelector('canvas');this.ctx=this.map.getContext('2d');this.results=this.root.querySelector('.race-results');
    this.show=this.root.querySelector('.race-show');
    for(const b of this.root.querySelectorAll('[data-race-action]'))b.onclick=()=>void this.act(b.dataset.raceAction);
    // Original race-over menu: Continue (X) opens Restart/Quit Event, and
    // Hide Menu (Square) toggles the results to watch the riders drive on.
    window.addEventListener('keydown',e=>{if(this.root.hidden||!this.resultShown||e.repeat)return;const game=e.target===document.getElementById('game');
      const action=e.code==='Escape'?(this.menu==='quit'?'back':this.hidden?'show':'hide'):e.code==='Enter'&&(game||this.hidden)?(this.hidden?'show':this.menu==='quit'?'retry':'continue'):null;
      if(action){e.preventDefault();e.stopImmediatePropagation();void this.act(action);}},true);
  }
  async act(action) {
    if(action==='continue'||action==='back'){this.setMenu(action==='continue'?'quit':'results');return;}
    if(action==='hide'||action==='show'){this.hidden=action==='hide';this.results.hidden=this.hidden;this.show.hidden=!this.hidden;(this.hidden?this.show:this.results.querySelector(`[data-menu=${this.menu}] button`)).focus({preventScroll:true});return;}
    if(this.busy)return;this.busy=true;try{await (action==='retry'?this.restart():this.quit());}catch(error){this.results.hidden=false;this.root.querySelector('.race-summary').textContent=error.message;}finally{this.busy=false;}
  }
  setMenu(menu) {this.menu=menu;for(const m of this.root.querySelectorAll('.race-menu'))m.hidden=m.dataset.menu!==menu;this.results.querySelector(`[data-menu=${menu}] button`).focus({preventScroll:true});}
  setCourse(course) {this.course=course;this.faded=false;this.resultShown=false;this.padArmed=false;this.hidden=false;this.root.hidden=true;this.results.hidden=true;this.show.hidden=true;this.orderKey='';}
  draw(race,player,visible) {
    this.root.hidden=!race||!visible;
    if(this.root.hidden)return;
    // Race load fades in from black over 500ms (`00210980`) as the
    // pre-race camera starts; its shots play without the race HUD.
    if(race.phase==='intro'&&!this.faded){this.faded=true;const fade=this.root.querySelector('.race-fade');fade.classList.remove('is-fading');void fade.offsetWidth;fade.classList.add('is-fading');}
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
    // The original lamp art replaces the digits when the course carries it.
    set('.race-countdown',this.lamps?'':race.phase==='countdown'?String(race.countdown):race.elapsed<1?'GO!':'');
    const key=JSON.stringify(race.standings.map(r=>r.name));
    if(key!==this.orderKey){this.orderKey=key;const list=this.root.querySelector('.race-order');list.replaceChildren(...race.standings.map(r=>{const li=document.createElement('li');li.textContent=r.name;li.classList.toggle('is-player',r.name==='YOU');return li;}));}
    this.radar(race,player);
    if(race.phase==='finished') {
      set('.race-summary',`Place ${race.position} of ${race.field} · ${raceTime(race.elapsed)} · Best ${raceTime(race.best)}`);
      // Race over settles every time at once (unfinished CPUs are estimated
      // as in the original); the riders keep driving behind the menu.
      if(!this.resultShown){
        this.resultShown=true;this.hidden=false;this.results.hidden=false;this.show.hidden=true;this.finish?.(race);
        this.results.querySelector('tbody').replaceChildren(...race.standings.map(r=>{const tr=document.createElement('tr');tr.classList.toggle('is-player',r.name==='YOU');
          for(const text of [`${r.position}. ${r.name}`,raceTime(r.time),raceTime(r.best)]){const td=document.createElement('td');td.textContent=text;tr.append(td);}return tr;}));
        this.setMenu('results');
      }
      let pad;try{pad=[...navigator.getGamepads()].find(p=>p?.connected!==false&&p);}catch{}
      const pressed=i=>Boolean(pad?.buttons[i]?.pressed),held=pressed(0)||pressed(1)||pressed(2);
      if(!held)this.padArmed=true;
      else if(this.padArmed){this.padArmed=false;void this.act(this.hidden?'show':pressed(2)?'hide':pressed(1)?(this.menu==='quit'?'back':'hide'):this.menu==='quit'?'retry':'continue');}
    } else {this.results.hidden=true;this.show.hidden=true;}
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
