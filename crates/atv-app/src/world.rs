//! World boards in the app (research/WORLD_BOARDS.md): the menus' board
//! fetches and a finished event's submission over the Pass Key transport
//! (`cloud::fetch` on the web, ureq on a thread elsewhere, against
//! furyremastered.com), the clock for the daily event, and the placings
//! line on the results.
use crate::App;
use atv_core::world_board::{self, Board, Placings};
use atv_ui::cloud::{Request, Response};
use atv_ui::front::controller::world_page::BoardState;

/// What a request was for.
#[derive(Clone, Debug, PartialEq)]
enum Purpose {
    Board(String),
    Submit,
}

/// A request's id and answer.
#[cfg(not(target_arch = "wasm32"))]
type Answer = (u64, Response);

#[derive(Default)]
pub struct World {
    next_id: u64,
    pending: Vec<(u64, Purpose)>,
    /// Desktop and Android: answers from the request threads.
    #[cfg(not(target_arch = "wasm32"))]
    answers: Option<(
        std::sync::mpsc::Sender<Answer>,
        std::sync::mpsc::Receiver<Answer>,
    )>,
}

/// The UTC day number now.
pub fn today() -> i64 {
    #[cfg(not(target_arch = "wasm32"))]
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0., |d| d.as_secs_f64() * 1000.);
    #[cfg(target_arch = "wasm32")]
    let ms = js_sys::Date::now();
    world_board::day_of(ms)
}

/// A finished race the world boards may take, kept until its name is final.
#[derive(Clone, Debug)]
pub struct Run {
    pub course: String,
    pub expert: bool,
    pub vehicle: u16,
    pub lap_ms: Option<u32>,
    pub race_ms: Option<u32>,
    pub daily: Option<String>,
    pub trace: std::sync::Arc<world_board::Trace>,
}

impl Run {
    /// The run of a finished single-player race that kept records (no Kid,
    /// assists, Practice, split screen, Duel or online), if it has a time
    /// for a board.
    pub fn of_race(frame: &atv_core::race::Frame) -> Option<Self> {
        let watch = frame.records.as_ref()?;
        let trace = frame.trace.clone()?;
        if frame.duel || !frame.split_records.is_empty() {
            return None;
        }
        let (course, expert) = match watch.key.strip_suffix(":expert") {
            Some(course) => (course.to_string(), true),
            None => (watch.key.clone(), false),
        };
        let laps = world_board::default_laps(&course) as usize;
        let race_ms = (watch.full_length && trace.marks.len() == laps)
            .then(|| trace.marks.last().copied())
            .flatten();
        let lap_ms = frame.best.map(world_board::millis);
        let pick = world_board::daily(today());
        let daily = (race_ms.is_some()
            && !expert
            && pick.course == course
            && pick.class.admits(watch.vehicle))
        .then_some(pick.date);
        (lap_ms.is_some() || race_ms.is_some()).then_some(Self {
            course,
            expert,
            vehicle: watch.vehicle,
            lap_ms,
            race_ms,
            daily,
            trace,
        })
    }

    /// The submission body under `name`.
    pub fn body(&self, name: &str) -> String {
        serde_json::to_string(&world_board::RaceSubmission {
            name,
            vehicle: self.vehicle,
            course: &self.course,
            expert: self.expert,
            lap_ms: self.lap_ms,
            race_ms: self.race_ms,
            daily: self.daily.as_deref(),
            trace: &self.trace,
        })
        .unwrap_or_default()
    }
}

impl App {
    fn world_send(
        &mut self,
        purpose: Purpose,
        method: &'static str,
        path: String,
        body: Option<String>,
    ) {
        let id = self.world.next_id;
        self.world.next_id += 1;
        self.world.pending.push((id, purpose));
        let request = Request {
            id,
            method,
            path: path.into(),
            headers: Vec::new(),
            body,
        };
        #[cfg(target_arch = "wasm32")]
        {
            let proxy = self.proxy.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let response = crate::cloud::fetch(request).await.unwrap_or_default();
                let _ = proxy.send_event(crate::Message::World(id, response));
            });
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let answers = self
                .world
                .answers
                .get_or_insert_with(std::sync::mpsc::channel)
                .0
                .clone();
            let origin = crate::cloud::origin();
            std::thread::spawn(move || {
                let _ = answers.send((id, crate::cloud::native_request(&origin, request)));
            });
        }
    }

    /// Each event-loop pass: answers (desktop), the menus' clock and the
    /// boards they want, and a finished event waiting to be sent.
    pub(crate) fn world_poll(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        while let Some(answer) = self
            .world
            .answers
            .as_ref()
            .and_then(|a| a.1.try_recv().ok())
        {
            self.world_response(answer.0, answer.1);
        }
        if let Some(body) = self.overlay.as_mut().and_then(|o| o.world_outbox.take()) {
            self.world_send(
                Purpose::Submit,
                "POST",
                "/api/boards/submit".into(),
                Some(body),
            );
        }
        let Some(front) = self.front.clone() else {
            return;
        };
        let wants = {
            let mut f = front.borrow_mut();
            if !matches!(f.state.page.as_str(), "single" | "daily" | "hall-of-fame") {
                return;
            }
            let day = Some(today());
            if f.state.world.day != day {
                f.state.world.day = day;
                let view = f.view;
                f.state.refresh(&view);
            }
            let wants = f.state.world_wants();
            for id in &wants {
                f.state.world_loading(id);
            }
            wants
        };
        for id in wants {
            let path = format!("/api/boards/{id}");
            self.world_send(Purpose::Board(id), "GET", path, None);
        }
    }

    pub(crate) fn world_response(&mut self, id: u64, response: Response) {
        let Some(at) = self.world.pending.iter().position(|(i, _)| *i == id) else {
            return;
        };
        let (_, purpose) = self.world.pending.swap_remove(at);
        match purpose {
            Purpose::Board(board) => {
                let state = match response.status {
                    0 => BoardState::Offline,
                    200 => serde_json::from_str::<Board>(&response.body)
                        .map_or(BoardState::Failed, BoardState::Ready),
                    _ => BoardState::Failed,
                };
                if let Some(front) = &self.front {
                    let mut f = front.borrow_mut();
                    let view = f.view;
                    f.state.world_answer(&board, state, &view);
                }
            }
            Purpose::Submit => {
                let placings = (response.status == 200)
                    .then(|| serde_json::from_str::<Placings>(&response.body).ok())
                    .flatten();
                let line = match &placings {
                    Some(p) => p.lines().join(" · "),
                    None if response.status == 0 => "World boards offline".into(),
                    None => String::new(),
                };
                if let Some(overlay) = &mut self.overlay {
                    overlay.results.world_line = (!line.is_empty()).then_some(line);
                }
                if let (Some(p), Some(front)) = (&placings, &self.front) {
                    let placed: Vec<String> = p
                        .boards
                        .iter()
                        .filter(|(_, b)| b.placed)
                        .map(|(id, _)| id.clone())
                        .collect();
                    front.borrow_mut().state.world_placed(&p.name, &placed);
                }
            }
        }
        self.wake();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}
