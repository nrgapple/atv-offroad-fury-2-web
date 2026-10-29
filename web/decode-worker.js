// Load-time decode worker: runs the engine's own WebAssembly for pure decode
// jobs (course textures, sky tiles) so they use the phone's other cores and
// leave the page's thread free (crates/atv-game/src/offload.rs). The page
// sends its compiled module first, then jobs; each answer carries the job id.
import { initSync, offload_job } from './atv_app.js';

onmessage = ({ data }) => {
  if (data.module) {
    initSync({ module: data.module });
    return;
  }
  const { id, kind, arg, bytes } = data;
  try {
    const out = offload_job(kind, arg, bytes);
    postMessage({ id, out }, [out.buffer]);
  } catch (error) {
    postMessage({ id, error: String(error?.message ?? error) });
  }
};
