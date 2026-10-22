//! Anonymous usage events (web/analytics.js through engine.js's
//! `window.furyUsage`): session_start, ride_start, ride_failed, ride_time
//! and race_finish with the same enumerated fields as web/game.js. The page
//! shell owns consent (Do Not Track, GPC, automation, local hosts), device
//! context, visibility and the beacon; desktop builds send nothing.
use serde_json::{Value, json};
use web_time::Instant;

/// Calls `window.furyUsage[method](...args)` when the page shell has it.
pub fn call(method: &str, args: &[Value]) {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        let Some(window) = web_sys::window() else {
            return;
        };
        let Ok(usage) = js_sys::Reflect::get(&window, &"furyUsage".into()) else {
            return;
        };
        if usage.is_undefined() || usage.is_null() {
            return;
        }
        let Ok(function) = js_sys::Reflect::get(&usage, &method.into())
            .and_then(|f| f.dyn_into::<js_sys::Function>())
        else {
            return;
        };
        let list = js_sys::Array::new();
        for arg in args {
            match js_sys::JSON::parse(&arg.to_string()) {
                Ok(value) => list.push(&value),
                Err(_) => return,
            };
        }
        let _ = function.apply(&usage, &list);
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (method, args);
}

/// game.js's ride context: mode, activity, venue, vehicle number and the
/// race settings (`startRide` / `onlineRide`).
pub fn ride(mode: &str, map: &str, quad: u16, race: Option<(u32, usize)>) -> Value {
    let mut ride = json!({
        "mode": mode,
        "activity": if race.is_some() { "race" } else { "freestyle" },
        "map": map,
        "vehicle": if quad > 26 { "motorcycle" } else { "atv" },
        "quad": quad,
    });
    if let Some((laps, cpu)) = race {
        ride["laps"] = laps.into();
        ride["cpu"] = cpu.into();
    }
    ride
}

/// The ride being reported.
#[derive(Default)]
pub struct Usage {
    /// A ride loading since the instant (ride_start carries its load time).
    pub pending: Option<(Value, Instant)>,
    pub ride: Option<Value>,
    /// Riding (not in a menu), as frontend.js `active(open)` reports.
    pub active: bool,
    /// race_finish was sent for this ride.
    pub finished: bool,
}

impl Usage {
    pub fn loading(&mut self, ride: Value) {
        self.pending = Some((ride, Instant::now()));
    }

    /// The ride is drawn: ride_start with its load seconds (and the room's
    /// rider count online).
    pub fn started(&mut self, players: Option<usize>) {
        let Some((mut ride, since)) = self.pending.take() else {
            return;
        };
        if let Some(players) = players {
            ride["players"] = players.into();
        }
        let seconds = since.elapsed().as_secs_f64();
        call(
            "rideStarted",
            &[ride.clone(), json!({ "seconds": seconds })],
        );
        // The frontend closes over the new ride (frontend.js active(false)).
        call("setRiding", &[true.into()]);
        self.ride = Some(ride);
        self.active = true;
        self.finished = false;
    }

    pub fn failed(&mut self) {
        if let Some((ride, _)) = self.pending.take() {
            call("rideFailed", &[ride]);
        }
        self.ride = None;
    }

    pub fn ended(&mut self) {
        if self.ride.take().is_some() {
            call("rideEnded", &[]);
        }
    }

    /// A menu opened (false) or closed (true) over the ride; opening it
    /// reports ten or more seconds of riding time.
    pub fn set_riding(&mut self, active: bool) {
        if self.ride.is_none() || active == self.active {
            return;
        }
        self.active = active;
        call("setRiding", &[active.into()]);
        if !active {
            call("report", &[10.into()]);
        }
    }

    /// race_finish once per race, from the finished race frame.
    pub fn race_finished(&mut self, place: usize, field: usize, seconds: f32) {
        let Some(ride) = &self.ride else { return };
        if self.finished {
            return;
        }
        self.finished = true;
        let mut event = ride.clone();
        event["place"] = place.into();
        event["field"] = field.into();
        event["seconds"] = f64::from(seconds).into();
        call("track", &["race_finish".into(), event]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ride_context_matches_game_js() {
        assert_eq!(
            ride("single", "env05", 29, Some((3, 4))),
            json!({"mode": "single", "activity": "race", "map": "env05",
                   "vehicle": "motorcycle", "quad": 29, "laps": 3, "cpu": 4})
        );
        let mut usage = Usage::default();
        usage.loading(ride("freeride", "env08", 1, None));
        usage.started(Some(5));
        assert_eq!(usage.ride.as_ref().unwrap()["players"], 5);
        usage.race_finished(1, 4, 90.);
        assert!(usage.finished);
        usage.ended();
        assert!(usage.ride.is_none());
    }
}
