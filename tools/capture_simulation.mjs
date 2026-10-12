// Observe every delivered WASM step, including intermediate frames in a batch.
// This wraps the test browser's worker transport without changing game inputs.
export async function captureSimulation(page, name) {
  await page.addInitScript(({ name }) => {
    const frames = window[name] = [];
    const pending = new WeakMap();
    const post = Worker.prototype.postMessage;
    Worker.prototype.postMessage = function (message, ...rest) {
      if (!pending.has(this)) {
        const requests = new Set();
        pending.set(this, requests);
        this.addEventListener('message', ({ data }) => {
          if (!requests.delete(data.id) || data.error || !data.bytes) return;
          const batch = JSON.parse(new TextDecoder().decode(data.bytes));
          for (const { frame, camera } of batch) {
            frames.push({ step: frames.length + 1, frame, camera, input: frame.applied_input });
          }
        });
      }
      if (message.op === 50) pending.get(this).add(message.id);
      return post.call(this, message, ...rest);
    };
  }, { name });
}
