//! Adaptive render resolution (web/render-support.js `RenderScale`): the
//! browser renders at most 1.5 backing pixels per CSS pixel and steps down
//! by 0.25 towards 1.0 after 1.5 s of slow (>24 ms) frames, back up after
//! 6 s of fast (<17.5 ms) frames; falling back after a recovery doubles the
//! wait before the next try (up to 60 s). Sampled only while riding.

pub struct RenderScale {
    max: f64,
    min: f64,
    step: f64,
    slow_ms: f64,
    fast_ms: f64,
    down_after: f64,
    up_after: f64,
    pub ratio: f64,
    average: f64,
    slow: f64,
    fast: f64,
    pub changes: u32,
    raised: bool,
}

impl Default for RenderScale {
    fn default() -> Self {
        Self {
            max: 1.5,
            min: 1.,
            step: 0.25,
            slow_ms: 24.,
            fast_ms: 17.5,
            down_after: 1.5,
            up_after: 6.,
            ratio: 1.5,
            average: 1000. / 60.,
            slow: 0.,
            fast: 0.,
            changes: 0,
            raised: false,
        }
    }
}

impl RenderScale {
    /// `frame_ms` is the raw interval between active frames.
    pub fn sample(&mut self, frame_ms: f64) -> f64 {
        if !frame_ms.is_finite() || frame_ms <= 0. || frame_ms > 250. {
            return self.ratio;
        }
        self.average += (frame_ms - self.average) * 0.1;
        let seconds = frame_ms / 1000.;
        self.slow = if self.average > self.slow_ms {
            self.slow + seconds
        } else {
            0.
        };
        self.fast = if self.average < self.fast_ms {
            self.fast + seconds
        } else {
            0.
        };
        if self.slow >= self.down_after && self.ratio > self.min {
            if self.raised {
                self.up_after = (self.up_after * 2.).min(60.);
            }
            self.ratio = (self.ratio - self.step).max(self.min);
            self.slow = 0.;
            self.raised = false;
            self.changes += 1;
        } else if self.fast >= self.up_after && self.ratio < self.max {
            self.ratio = (self.ratio + self.step).min(self.max);
            self.fast = 0.;
            self.raised = true;
            self.changes += 1;
        }
        self.ratio
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_down_on_slow_frames_and_recovers_with_backoff() {
        let mut scale = RenderScale::default();
        assert_eq!(scale.ratio, 1.5);
        // 33 ms frames: the average passes 24 ms after 6 frames, then each
        // 1.5 s (46 frames) of slow frames is one step.
        for _ in 0..60 {
            scale.sample(33.);
        }
        assert_eq!(scale.ratio, 1.25);
        for _ in 0..50 {
            scale.sample(33.);
        }
        assert_eq!(scale.ratio, 1.);
        // Fast frames raise it again after 6 s; a relapse doubles that wait.
        for _ in 0..700 {
            scale.sample(10.);
        }
        assert_eq!(scale.ratio, 1.25);
        for _ in 0..200 {
            scale.sample(33.);
        }
        assert_eq!(scale.ratio, 1.);
        assert_eq!(scale.up_after, 12.);
        // Hitches and stalls are ignored.
        assert_eq!(scale.sample(400.), 1.);
    }
}
