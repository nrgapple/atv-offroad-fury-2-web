const ready = WebAssembly.instantiateStreaming(fetch('atv_web.wasm'), {}).then(result => result.instance.exports);
self.onmessage = async ({ data: { id, op, index = 0, bytes } }) => {
  try {
    const wasm = await ready;
    if (!wasm.input_resize(bytes.byteLength)) throw new Error('File exceeds 128 MiB limit. Open the ISO to read individual files.');
    new Uint8Array(wasm.memory.buffer, wasm.input_ptr(), bytes.byteLength).set(bytes);
    const started = performance.now();
    const ok = wasm.run(op, index);
    const operationMs = performance.now() - started;
    const result = new Uint8Array(wasm.memory.buffer, wasm.output_ptr(), wasm.output_len()).slice();
    if (!ok) throw new Error(new TextDecoder().decode(result));
    self.postMessage({ id, bytes: result, operationMs, wasmBytes: wasm.memory.buffer.byteLength }, [result.buffer]);
  } catch (error) { self.postMessage({ id, error: error.message }); }
};
ready.then(() => self.postMessage({ ready: true })).catch(error => self.postMessage({ startupError: error.message }));
