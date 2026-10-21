//! Optional per-pass CPU timing of a frame (off unless enabled; the parity
//! and CPU baselines run without it). Each `mark` charges the time since the
//! previous mark to a named slot; `report` gives the mean per frame.
use web_time::Instant;

#[derive(Default)]
pub struct Profiler {
    enabled: bool,
    last: Option<Instant>,
    slots: Vec<(&'static str, f64)>,
    counts: Vec<(&'static str, f64)>,
    frames: u32,
}

impl Profiler {
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.slots.clear();
        self.counts.clear();
        self.frames = 0;
        self.last = None;
    }

    pub fn start(&mut self) {
        if self.enabled {
            self.last = Some(Instant::now());
        }
    }

    pub fn mark(&mut self, name: &'static str) {
        let Some(last) = self.last else {
            return;
        };
        let now = Instant::now();
        let ms = (now - last).as_secs_f64() * 1000.;
        self.last = Some(now);
        match self.slots.iter_mut().find(|(n, _)| *n == name) {
            Some((_, total)) => *total += ms,
            None => self.slots.push((name, ms)),
        }
    }

    /// Adds a per-frame count (draws of a pass) while profiling.
    pub fn count(&mut self, name: &'static str, n: u32) {
        if self.last.is_none() {
            return;
        }
        match self.counts.iter_mut().find(|(c, _)| *c == name) {
            Some((_, total)) => *total += f64::from(n),
            None => self.counts.push((name, f64::from(n))),
        }
    }

    pub fn end_frame(&mut self) {
        if self.last.take().is_some() {
            self.frames += 1;
        }
    }

    /// Mean milliseconds per profiled frame, in first-seen order.
    pub fn report(&self) -> Vec<(&'static str, f64)> {
        let n = f64::from(self.frames.max(1));
        self.slots
            .iter()
            .map(|(name, ms)| (*name, ms / n))
            .collect()
    }

    /// Mean counts per profiled frame.
    pub fn counts(&self) -> Vec<(&'static str, f64)> {
        let n = f64::from(self.frames.max(1));
        self.counts.iter().map(|(name, c)| (*name, c / n)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::Profiler;

    #[test]
    fn charges_marks_only_while_enabled() {
        let mut p = Profiler::default();
        p.start();
        p.mark("a");
        p.count("draws", 3);
        p.end_frame();
        assert!(p.report().is_empty() && p.counts().is_empty());
        p.set_enabled(true);
        for _ in 0..2 {
            p.start();
            p.mark("a");
            p.mark("b");
            p.count("draws", 3);
            p.end_frame();
        }
        let names: Vec<_> = p.report().iter().map(|(n, _)| *n).collect();
        assert_eq!(names, ["a", "b"]);
        assert_eq!(p.counts(), [("draws", 3.)]);
    }
}
