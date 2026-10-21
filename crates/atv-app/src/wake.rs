//! Keeps the screen awake while riding (web/wake-lock.js): controller and
//! keyboard play produce no touches, so phones and tablets would otherwise
//! dim and lock mid-ride. The Screen Wake Lock is held while a ride is on
//! screen and released in menus; the browser drops it whenever the page is
//! hidden, so it is requested again on return or on the next gesture (iOS
//! Home Screen apps support it from iOS 18.4). The API is reached through
//! `js_sys::Reflect` (web-sys marks it unstable). Desktop builds have no
//! clean cross-platform screensaver inhibitor in winit; they do nothing.
#[cfg(target_arch = "wasm32")]
use std::{cell::RefCell, rc::Rc};

#[derive(Default)]
pub struct ScreenWake {
    #[cfg(target_arch = "wasm32")]
    state: Rc<RefCell<State>>,
}

#[cfg(target_arch = "wasm32")]
#[derive(Default)]
struct State {
    wanted: bool,
    sentinel: Option<wasm_bindgen::JsValue>,
    pending: bool,
    requests: u32,
}

#[cfg(target_arch = "wasm32")]
fn api() -> Option<wasm_bindgen::JsValue> {
    let navigator = web_sys::window()?.navigator();
    let lock = js_sys::Reflect::get(&navigator, &"wakeLock".into()).ok()?;
    let request = js_sys::Reflect::get(&lock, &"request".into()).ok()?;
    request.is_function().then_some(lock)
}

#[cfg(target_arch = "wasm32")]
fn release_lock(lock: &wasm_bindgen::JsValue) {
    use wasm_bindgen::JsCast;
    if let Ok(release) = js_sys::Reflect::get(lock, &"release".into())
        && let Ok(release) = release.dyn_into::<js_sys::Function>()
        && let Ok(promise) = release.call0(lock)
        && let Ok(promise) = promise.dyn_into::<js_sys::Promise>()
    {
        // A rejected release (already released) is not an error.
        wasm_bindgen_futures::spawn_local(async move {
            let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
        });
    }
}

impl ScreenWake {
    /// Whether the page should keep the screen awake: a ride on screen, no
    /// menu. `visible`: the document is visible (hidden pages lose the lock).
    pub fn set(&self, wanted: bool, visible: bool) {
        #[cfg(target_arch = "wasm32")]
        {
            let mut s = self.state.borrow_mut();
            let changed = s.wanted != wanted;
            s.wanted = wanted;
            if !visible {
                // The browser has released it; ask again when visible.
                s.sentinel = None;
                return;
            }
            if !wanted {
                if let Some(lock) = s.sentinel.take() {
                    release_lock(&lock);
                }
                return;
            }
            let released = s.sentinel.as_ref().is_some_and(|l| {
                js_sys::Reflect::get(l, &"released".into())
                    .ok()
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            });
            if released {
                s.sentinel = None;
            }
            drop(s);
            if changed || released {
                self.acquire();
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = (wanted, visible);
    }

    /// A pointer or key gesture: browsers that wanted an activation grant
    /// the lock now.
    pub fn gesture(&self) {
        #[cfg(target_arch = "wasm32")]
        self.acquire();
    }

    /// The lock is held (test evidence).
    pub fn held(&self) -> bool {
        #[cfg(target_arch = "wasm32")]
        {
            self.state.borrow().sentinel.is_some()
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            false
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn acquire(&self) {
        use wasm_bindgen::JsCast;
        let mut s = self.state.borrow_mut();
        let visible = web_sys::window()
            .and_then(|w| w.document())
            .is_none_or(|d| d.visibility_state() != web_sys::VisibilityState::Hidden);
        if !s.wanted || s.sentinel.is_some() || s.pending || !visible {
            return;
        }
        let Some(lock) = api() else { return };
        let Ok(request) = js_sys::Reflect::get(&lock, &"request".into())
            .and_then(|r| r.dyn_into::<js_sys::Function>())
        else {
            return;
        };
        let Ok(promise) = request
            .call1(&lock, &"screen".into())
            .and_then(|p| p.dyn_into::<js_sys::Promise>())
        else {
            return;
        };
        s.pending = true;
        s.requests += 1;
        drop(s);
        let state = self.state.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let result = wasm_bindgen_futures::JsFuture::from(promise).await;
            let mut s = state.borrow_mut();
            s.pending = false;
            if let Ok(lock) = result {
                let visible = web_sys::window()
                    .and_then(|w| w.document())
                    .is_none_or(|d| d.visibility_state() != web_sys::VisibilityState::Hidden);
                if s.wanted && visible {
                    s.sentinel = Some(lock);
                } else {
                    release_lock(&lock);
                }
            }
        });
    }
}
