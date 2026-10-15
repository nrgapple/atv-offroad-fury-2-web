// Opt-in original-content integration harness. All coordinates come from loaded
// resources; test placement does not inject impulses, velocities or wreck states.
import { button, waitForGame } from './browser/harness.mjs';
export async function installContactHarness(page) {
  await page.addInitScript(() => {
    const NativeWorker = Worker; let id = -2000;
    window.contactAssets = {};
    window.Worker = class extends NativeWorker {
      constructor(...args) {
        super(...args); window.contactWorker = this; this.ops = new Map(); this.pending = new Map();
        this.addEventListener('message', ({data}) => {
          const request = this.ops.get(data.id); this.ops.delete(data.id);
          if (request && data.bytes && !data.error) {
            const parsed = () => JSON.parse(new TextDecoder().decode(data.bytes));
            if (request.op === 9) contactAssets.info = parsed();
            if (request.op === 10) contactAssets.heights = new Float32Array(data.bytes.buffer, data.bytes.byteOffset, data.bytes.byteLength / 4).slice();
            if (request.op === 60) contactAssets.scene = parsed();
            if (request.op === 64) contactAssets.water = parsed();
          }
          const pending = this.pending.get(data.id);
          if (pending) { this.pending.delete(data.id); data.error ? pending.reject(Error(data.error)) : pending.resolve(JSON.parse(new TextDecoder().decode(data.bytes))); }
        });
      }
      postMessage(data, ...args) {
        if (data.op === 49) contactAssets.placement = JSON.parse(new TextDecoder().decode(data.bytes));
        this.ops.set(data.id, {op:data.op}); super.postMessage(data, ...args);
      }
    };
    window.contactCommand = (op, value) => new Promise((resolve,reject) => {
      const key=id--; contactWorker.pending.set(key,{resolve,reject});
      contactWorker.postMessage({id:key,op,index:0,bytes:new TextEncoder().encode(value == null ? '' : JSON.stringify(value))});
    });
    window.contactSteps = async (count, input={}) => {
      const frames=[];
      for(let i=0;i<count;i+=4) frames.push(...await contactCommand(50,{count:Math.min(4,count-i),input:{throttle:0,brake:0,steer:0,lean:0,reverse:false,...input},orbit:0}));
      return frames.map(row=>row.frame);
    };
  });
}

export async function loadContactMap(page, base, environment) {
  await page.goto(`${base}/play.html?drive=1&environment=${environment}`);
  await waitForGame(page);
  await page.keyboard.press('Escape');
  await button(page,'Resume').waitFor();
  // Time-based on purpose: lets a simulation batch already sent to the worker
  // finish before direct contact commands run against the paused state.
  await page.waitForTimeout(100);
}

// Find a short original shoreline segment with a dry bank and deep water.
// This inspects the original height grid; it does not create a water footprint.
export async function findWaterRoute(page, minimumDepth = 4) {
  return page.evaluate(minimumDepth => {
    const {info, heights, water, scene} = contactAssets;
    const width=info.width_cells+1, base=water.base_height; let best;
    for (let z=8;z<info.depth_cells-8;z+=2) for(let x=8;x<info.width_cells-8;x+=2) {
      const height=heights[z*width+x]; if(height>base-minimumDepth) continue;
      for(const [dx,dz] of [[1,0],[-1,0],[0,1],[0,-1]]) for(let d=2;d<=15;d++) {
        const sx=x+dx*d,sz=z+dz*d;
        if(sx<3||sz<3||sx>info.width_cells-3||sz>info.depth_cells-3) continue;
        const dry=heights[sz*width+sx]; if(dry<base+1||dry>base+8) continue;
        const origin=[sx*info.spacing,dry+1,sz*info.spacing], target=[x*info.spacing,height,z*info.spacing];
        const score=Math.hypot(origin[0]-scene.start_position[0],origin[2]-scene.start_position[2]);
        if(!best||score<best.score) best={origin,target,yaw:Math.atan2(-dx,-dz),score,base};
        break;
      }
    }
    if(!best) throw Error('No original dry-bank/deep-water route found.');
    return best;
  }, minimumDepth);
}
