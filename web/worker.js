const ready = WebAssembly.instantiateStreaming(fetch('atv_web.wasm'), {});
// Archives the page decodes member by member stay in Rust memory under a key,
// so each extraction sends only the key instead of copying the archive again.
self.onmessage = async ({ data }) => {
  const { id, op, index = 0, bytes, key, release } = data;
  try {
    const wasm = (await ready).instance.exports;
    if (release !== undefined) { wasm.resident_drop(release); return; }
    if (bytes) {
      if (!wasm.input_resize(bytes.byteLength)) throw new Error('File exceeds 128 MiB limit. Open the ISO to read individual files.');
      new Uint8Array(wasm.memory.buffer, wasm.input_ptr(), bytes.byteLength).set(bytes);
      if (key !== undefined) wasm.resident_store(key);
    }
    const started = performance.now();
    const ok = key === undefined ? wasm.run(op, index) : wasm.run_resident(op, index, key);
    const operationMs = performance.now() - started;
    if (ok === 2) { self.postMessage({ id, missing: true }); return; }
    const result = new Uint8Array(wasm.memory.buffer, wasm.output_ptr(), wasm.output_len()).slice();
    if (!ok) throw new Error(new TextDecoder().decode(result));
    self.postMessage({ id, bytes: result, operationMs, wasmBytes: wasm.memory.buffer.byteLength }, [result.buffer]);
  } catch (error) { if (release === undefined) self.postMessage({ id, error: error.message }); }
};
// The page instantiates the same compiled module for synchronous render helpers.
// A browser that cannot clone modules receives none and fetches its own copy.
ready.then(({ module }) => { try { self.postMessage({ ready: true, module }); } catch { self.postMessage({ ready: true }); } })
  .catch(error => self.postMessage({ startupError: error.message }));
