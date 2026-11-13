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

/// A finished Freestyle objective the world boards may take
/// (`score:<series>:<course>`).
#[derive(Clone, Debug, PartialEq)]
pub struct ScoreRun {
    pub series: &'static str,
    pub course: String,
    pub vehicle: u16,
    pub score: i64,
    pub seconds: u32,
    pub trace: world_board::Trace,
}

impl ScoreRun {
    /// The run of a finished single-player objective whose score earned a
    /// local Hall of Fame place (`fame`, after its `finish`), ridden for
    /// `elapsed` seconds. Like a race, a run an assist acted in sends
    /// nothing; split screen keeps its own records and never comes here.
    pub fn of_objective(
        series: &'static str,
        course: &str,
        fame: &atv_core::hall_of_fame::FreestyleWatch,
        score: f32,
        elapsed: f32,
        assisted: bool,
        trace: &world_board::Trace,
    ) -> Option<Self> {
        let tag = fame.tag();
        let score = score.max(0.) as i64;
        let placed = fame
            .fame
            .objectives
            .iter()
            .any(|e| e.name == tag && e.value == score);
        (placed && score > 0 && !assisted).then(|| Self {
            series,
            course: course.to_string(),
            vehicle: fame.vehicle,
            score,
            seconds: world_board::run_seconds(elapsed),
            trace: trace.clone(),
        })
    }

    /// The submission body under `name`.
    pub fn body(&self, name: &str) -> String {
        serde_json::to_string(&world_board::FreestyleSubmission {
            name,
            vehicle: self.vehicle,
            series: self.series,
            course: &self.course,
            score: self.score,
            seconds: self.seconds,
            trace: &self.trace,
        })
        .unwrap_or_default()
    }
}

/// A finished event waiting for its Hall of Fame name.
#[derive(Clone, Debug)]
pub enum Finished {
    Race(Run),
    Freestyle(ScoreRun),
}

impl Finished {
    /// A daily event time is sent whether or not it set a local record.
    pub fn daily(&self) -> bool {
        matches!(self, Self::Race(run) if run.daily.is_some())
    }

    pub fn body(&self, name: &str) -> String {
        match self {
            Self::Race(run) => run.body(name),
            Self::Freestyle(run) => run.body(name),
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use atv_core::hall_of_fame::{FreestyleWatch, HallOfFame};

    fn trace(seconds: u32) -> world_board::Trace {
        let mut trace = world_board::Trace::default();
        for second in 0..=seconds {
            trace.step(second as f32, [40. * second as f32, 0., 7.], 0);
        }
        trace
    }

    #[test]
    fn a_freestyle_score_is_sent_only_when_it_earned_a_local_place() {
        let mut fame = FreestyleWatch::new("unspstunt", "env05", 4, None, None);
        fame.finish(31_250.7);
        let run = ScoreRun::of_objective(
            "unspstunt",
            "env05",
            &fame,
            31_250.7,
            96.4,
            false,
            &trace(96),
        )
        .expect("the first score of a course places");
        assert_eq!((run.score, run.seconds, run.vehicle), (31_250, 96, 4));
        let body: serde_json::Value = serde_json::from_str(&run.body("ADA")).unwrap();
        assert_eq!(body["series"], "unspstunt");
        assert_eq!(body["course"], "env05");
        assert_eq!(body["score"], 31_250);
        assert_eq!(body["trace"]["points"].as_array().unwrap().len(), 97);
        assert!(body.get("lapMs").is_none() && body.get("expert").is_none());
        assert!(!Finished::Freestyle(run).daily());

        // An assisted run keeps its local place but stays off the world's.
        let assisted = ScoreRun::of_objective(
            "unspstunt",
            "env05",
            &fame,
            31_250.7,
            96.4,
            true,
            &trace(96),
        );
        assert_eq!(assisted, None);

        // A score below the course's three places is not sent.
        let mut hall = HallOfFame::default();
        for (name, score) in [("A", 90_000.), ("B", 80_000.), ("C", 70_000.)] {
            let mut run = FreestyleWatch::new("unspstunt", "env05", 2, Some(&hall), None);
            run.finish(score);
            hall.commit_run(&atv_core::hall_of_fame::Watch::Freestyle(run), name);
        }
        let mut low = FreestyleWatch::new("unspstunt", "env05", 4, Some(&hall), None);
        low.finish(31_250.);
        let none =
            ScoreRun::of_objective("unspstunt", "env05", &low, 31_250., 96., false, &trace(96));
        assert_eq!(none, None);
        // A run that scored nothing holds no place.
        let mut zero = FreestyleWatch::new("spstunt", "free01", 4, None, None);
        zero.finish(0.);
        let none = ScoreRun::of_objective("spstunt", "free01", &zero, 0., 120., false, &trace(120));
        assert_eq!(none, None);
    }
}
