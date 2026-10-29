//! The page's decode worker pool (web): Web Workers (`web/decode-worker.js`)
//! running this same WebAssembly module, each job the app's exported
//! `offload_job`. Load-time decodes (`atv_game::offload`) and content
//! verification run here so they use the phone's other cores and leave the
//! page's thread free. Every job is a pure function of its bytes, so a job
//! gives the same output wherever it runs; without a pool callers run it
//! inline.
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

/// Whether workers are up (a load warmed the pool and it is not idle yet).
pub fn running() -> bool {
    POOL.with(|pool| {
        let pool = pool.borrow();
        !pool.broken && !pool.workers.is_empty()
    })
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
