//! Load-time decodes handed to other CPU cores.
//!
//! The PS2 decoded its course textures on one CPU while the loader played.
//! A phone has several cores, and on the web the engine's own thread is also
//! the page's: a long decode there freezes the loader and every input. Pure
//! decodes (bytes in, pixels out) therefore run as jobs:
//!
//! - web: a small pool of Web Workers (`web/decode-worker.js`) running this
//!   same WebAssembly module, sharing the page's compiled module; the page
//!   thread only copies bytes in and pixels out;
//! - desktop and Android: scoped threads.
//!
//! Every job runs the same Rust function ([`run`]) wherever it runs, so the
//! output is byte-identical to decoding inline; a pool that cannot start (no
//! worker support, an old page shell, a worker error) decodes inline instead.
use crate::Result;

/// Job kinds ([`run`]'s `kind`).
pub const IPUM: u32 = 1;
/// [`IPUM`] flag: also produce the deblocked/smoothed tile (Enhanced sky).
pub const SMOOTH: u32 = 1;

/// One decoded IPUM image: its RGBA and, when asked, the smoothed RGBA.
#[derive(Clone, Debug, PartialEq)]
pub struct Decoded {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub smooth: Option<Vec<u8>>,
}

/// Runs one job on this thread: the function every worker runs.
/// Output layout for [`IPUM`]: width and height (u32 LE), the RGBA, then
/// the smoothed RGBA when [`SMOOTH`] is set.
pub fn run(kind: u32, arg: u32, bytes: &[u8]) -> Result<Vec<u8>> {
    match kind {
        IPUM => {
            let image = atv_core::ipu::decode_ipum(bytes)?;
            let smooth = (arg & SMOOTH != 0).then(|| image.smoothed_rgba());
            let mut out = Vec::with_capacity(
                8 + image.rgba.len() + smooth.as_ref().map_or(0, Vec::len),
            );
            out.extend_from_slice(&image.info.width.to_le_bytes());
            out.extend_from_slice(&image.info.height.to_le_bytes());
            out.extend_from_slice(&image.rgba);
            if let Some(smooth) = smooth {
                out.extend_from_slice(&smooth);
            }
            Ok(out)
        }
        _ => Err(format!("unknown decode job {kind}")),
    }
}

fn unpack_ipum(out: Vec<u8>, smooth: bool) -> Result<Decoded> {
    let word = |at: usize| -> Result<u32> {
        Ok(u32::from_le_bytes(
            out.get(at..at + 4)
                .ok_or("short decode result")?
                .try_into()
                .map_err(|_| "short decode result")?,
        ))
    };
    let (width, height) = (word(0)?, word(4)?);
    let size = width as usize * height as usize * 4;
    let expected = 8 + size * if smooth { 2 } else { 1 };
    if out.len() != expected {
        return Err("decode result size mismatch".into());
    }
    let rgba = out[8..8 + size].to_vec();
    let smooth = smooth.then(|| out[8 + size..].to_vec());
    Ok(Decoded {
        width,
        height,
        rgba,
        smooth,
    })
}

/// Decodes IPUM images (terrain layers, sky tiles) in parallel, in order.
pub async fn ipums(inputs: &[&[u8]], smooth: bool) -> Result<Vec<Decoded>> {
    let arg = if smooth { SMOOTH } else { 0 };
    let outputs = many(IPUM, arg, inputs).await?;
    outputs
        .into_iter()
        .map(|out| unpack_ipum(out, smooth))
        .collect()
}

/// Starts the web pool ahead of the first job (a ride load), so its workers
/// instantiate while the course downloads. No-op elsewhere.
pub fn warm() {
    #[cfg(target_arch = "wasm32")]
    web::warm();
}

/// Runs `inputs` as `kind` jobs and returns their outputs in order.
pub async fn many(kind: u32, arg: u32, inputs: &[&[u8]]) -> Result<Vec<Vec<u8>>> {
    if inputs.is_empty() {
        return Ok(Vec::new());
    }
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(outputs) = web::many(kind, arg, inputs).await {
            return outputs;
        }
        // No pool: inline, giving the browser the thread between jobs.
        let mut outputs = Vec::with_capacity(inputs.len());
        for input in inputs {
            outputs.push(run(kind, arg, input)?);
            crate::yield_frame().await;
        }
        Ok(outputs)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let threads = std::thread::available_parallelism()
            .map_or(1, |n| n.get())
            .clamp(1, 8)
            .min(inputs.len());
        let per = inputs.len().div_ceil(threads);
        let results: Vec<Result<Vec<Vec<u8>>>> = std::thread::scope(|scope| {
            let handles: Vec<_> = inputs
                .chunks(per)
                .map(|group| {
                    scope.spawn(move || {
                        group
                            .iter()
                            .map(|input| run(kind, arg, input))
                            .collect::<Result<Vec<_>>>()
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().unwrap_or_else(|_| Err("decode thread panicked".into())))
                .collect()
        });
        let mut outputs = Vec::with_capacity(inputs.len());
        for group in results {
            outputs.extend(group?);
        }
        crate::yield_frame().await;
        Ok(outputs)
    }
}

#[cfg(target_arch = "wasm32")]
mod web {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::JsFuture;

    /// Idle pools are torn down after this long, returning their memory.
    const IDLE_MS: i32 = 20_000;

    struct Worker {
        worker: web_sys::Worker,
        /// Jobs posted and not yet answered.
        load: usize,
        _message: Closure<dyn FnMut(web_sys::MessageEvent)>,
        _error: Closure<dyn FnMut(web_sys::Event)>,
    }

    struct Pending {
        resolve: js_sys::Function,
        reject: js_sys::Function,
        worker: usize,
    }

    #[derive(Default)]
    struct Pool {
        workers: Vec<Worker>,
        pending: HashMap<u32, Pending>,
        next: u32,
        /// A worker failed to start or crashed: decode inline from now on.
        broken: bool,
        idle_timer: Option<i32>,
        idle_closure: Option<Closure<dyn FnMut()>>,
        /// Terminated workers whose callbacks may still be on the stack.
        retired: Vec<Worker>,
    }

    thread_local! {
        static POOL: RefCell<Pool> = RefCell::new(Pool::default());
    }

    /// The worker script's URL, published by the page shell (engine.js) so
    /// packaging versions it with the rest of the module graph.
    fn script() -> Option<String> {
        let window = web_sys::window()?;
        js_sys::Reflect::get(&window, &"furyDecodeWorker".into())
            .ok()?
            .as_string()
    }

    fn size() -> usize {
        let cores = web_sys::window()
            .map(|w| w.navigator().hardware_concurrency())
            .unwrap_or(2.) as usize;
        // Leave a core for the page (and the browser's own threads).
        cores.saturating_sub(1).clamp(2, 4)
    }

    fn fail_all(pool: &mut Pool, reason: &str) {
        pool.broken = true;
        for (_, p) in pool.pending.drain() {
            let _ = p.reject.call1(&JsValue::NULL, &JsValue::from_str(reason));
        }
        for w in pool.workers.drain(..).collect::<Vec<_>>() {
            w.worker.terminate();
            pool.retired.push(w);
        }
    }

    fn start(pool: &mut Pool) -> bool {
        if pool.broken {
            return false;
        }
        if !pool.workers.is_empty() {
            return true;
        }
        let Some(url) = script() else {
            pool.broken = true;
            return false;
        };
        let options = web_sys::WorkerOptions::new();
        options.set_type(web_sys::WorkerType::Module);
        let module = wasm_bindgen::module();
        for _ in 0..size() {
            let Ok(worker) = web_sys::Worker::new_with_options(&url, &options) else {
                fail_all(pool, "decode worker unavailable");
                return false;
            };
            // The page's compiled module first: messages wait in order
            // until the worker's script runs.
            let init = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&init, &"module".into(), &module);
            if worker.post_message(&init).is_err() {
                worker.terminate();
                fail_all(pool, "decode worker unavailable");
                return false;
            }
            let message = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(
                move |event: web_sys::MessageEvent| {
                    let data = event.data();
                    let get = |key: &str| js_sys::Reflect::get(&data, &key.into()).ok();
                    let Some(id) = get("id").and_then(|v| v.as_f64()) else {
                        return;
                    };
                    POOL.with(|pool| {
                        let mut pool = pool.borrow_mut();
                        let Some(job) = pool.pending.remove(&(id as u32)) else {
                            return;
                        };
                        if let Some(w) = pool.workers.get_mut(job.worker) {
                            w.load = w.load.saturating_sub(1);
                        }
                        match get("out").filter(|v| !v.is_undefined()) {
                            Some(out) => {
                                let _ = job.resolve.call1(&JsValue::NULL, &out);
                            }
                            None => {
                                let error = get("error").unwrap_or(JsValue::NULL);
                                let _ = job.reject.call1(&JsValue::NULL, &error);
                            }
                        }
                        if pool.pending.is_empty() {
                            schedule_idle(&mut pool);
                        }
                    });
                },
            );
            worker.set_onmessage(Some(message.as_ref().unchecked_ref()));
            let error = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
                POOL.with(|pool| fail_all(&mut pool.borrow_mut(), "decode worker failed"));
            });
            worker.set_onerror(Some(error.as_ref().unchecked_ref()));
            pool.workers.push(Worker {
                worker,
                load: 0,
                _message: message,
                _error: error,
            });
        }
        true
    }

    /// Tears the pool down once nothing has been asked of it for a while.
    fn schedule_idle(pool: &mut Pool) {
        let Some(window) = web_sys::window() else {
            return;
        };
        if let Some(timer) = pool.idle_timer.take() {
            window.clear_timeout_with_handle(timer);
        }
        let closure = Closure::<dyn FnMut()>::new(|| {
            POOL.with(|pool| {
                let mut pool = pool.borrow_mut();
                pool.idle_timer = None;
                if pool.pending.is_empty() {
                    for w in pool.workers.drain(..) {
                        w.worker.terminate();
                    }
                }
            });
        });
        pool.idle_timer = window
            .set_timeout_with_callback_and_timeout_and_arguments_0(
                closure.as_ref().unchecked_ref(),
                IDLE_MS,
            )
            .ok();
        pool.idle_closure = Some(closure);
    }

    pub fn warm() {
        POOL.with(|pool| {
            let mut pool = pool.borrow_mut();
            if start(&mut pool) && pool.pending.is_empty() {
                schedule_idle(&mut pool);
            }
        });
    }

    fn submit(kind: u32, arg: u32, input: &[u8]) -> Option<js_sys::Promise> {
        POOL.with(|pool| {
            let mut pool = pool.borrow_mut();
            if !start(&mut pool) {
                return None;
            }
            if let (Some(timer), Some(window)) = (pool.idle_timer.take(), web_sys::window()) {
                window.clear_timeout_with_handle(timer);
            }
            let worker = (0..pool.workers.len()).min_by_key(|&i| pool.workers[i].load)?;
            let id = pool.next;
            pool.next = pool.next.wrapping_add(1);
            let mut handlers = None;
            let promise = js_sys::Promise::new(&mut |resolve, reject| {
                handlers = Some((resolve, reject));
            });
            let (resolve, reject) = handlers?;
            let bytes = js_sys::Uint8Array::from(input);
            let message = js_sys::Object::new();
            for (key, value) in [
                ("id", JsValue::from(id)),
                ("kind", JsValue::from(kind)),
                ("arg", JsValue::from(arg)),
                ("bytes", bytes.clone().into()),
            ] {
                let _ = js_sys::Reflect::set(&message, &key.into(), &value);
            }
            let transfer = js_sys::Array::of1(&bytes.buffer());
            pool.workers[worker]
                .worker
                .post_message_with_transfer(&message, &transfer)
                .ok()?;
            pool.workers[worker].load += 1;
            pool.pending.insert(
                id,
                Pending {
                    resolve,
                    reject,
                    worker,
                },
            );
            Some(promise)
        })
    }

    /// The outputs of `inputs` from the pool; None when there is no pool
    /// (the caller decodes inline). A job error is the decode's own error.
    pub async fn many(
        kind: u32,
        arg: u32,
        inputs: &[&[u8]],
    ) -> Option<crate::Result<Vec<Vec<u8>>>> {
        let promises = js_sys::Array::new();
        for input in inputs {
            promises.push(submit(kind, arg, input)?.as_ref());
        }
        Some(
            match JsFuture::from(js_sys::Promise::all(&promises)).await {
                Ok(results) => Ok(js_sys::Array::from(&results)
                    .iter()
                    .map(|out| js_sys::Uint8Array::new(&out).to_vec())
                    .collect()),
                Err(error) => {
                    // A crashed pool is not the content's fault: decode inline.
                    if POOL.with(|p| p.borrow().broken) {
                        return None;
                    }
                    Err(error
                        .as_string()
                        .unwrap_or_else(|| "decode failed".into()))
                }
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_jobs_fail() {
        assert!(run(99, 0, &[]).is_err());
    }

    #[test]
    fn ipum_results_round_trip() {
        let mut out = Vec::new();
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        out.extend_from_slice(&[9; 8]);
        let decoded = unpack_ipum(out.clone(), true).unwrap();
        assert_eq!((decoded.width, decoded.height), (2, 1));
        assert_eq!(decoded.rgba, [1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(decoded.smooth.as_deref(), Some(&[9u8; 8][..]));
        assert!(unpack_ipum(out, false).is_err());
    }
}
