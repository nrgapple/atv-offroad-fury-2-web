//! Settings → Display → Show FPS (port option, this device; `?fps=1` for a
//! session): a small readout of the frames actually presented.
//!
//! The host records the time of every frame it hands to the surface (the
//! browser's animation frames, the desktop swapchain's presents). The rate is
//! the number of those presents in the last second, so a menu that draws
//! only on input reads low (and the host shows "idle" once it sleeps) rather
//! than a target or a simulation rate. The text changes at most twice a
//! second; the frame time is the mean present-to-present interval and `max`
//! the longest one, so a stutter shows even when the rate rounds to 60.
use crate::{
    Color, DisplayList, Font, Fonts, TextStyle,
    paint::{BoxPaint, Rect},
};
use std::fmt::Write;

/// Saved choice: "on" or "off" (off by default).
pub const KEY: &str = "atv-show-fps";
/// The rolling window the rate counts presents over.
pub const WINDOW_MS: f64 = 1000.;
/// How often the text is rebuilt.
pub const REFRESH_MS: f64 = 500.;
/// Presents kept: one window at up to 500 Hz.
const CAPACITY: usize = 512;

/// The present times of the last window (a fixed ring: no allocation per
/// frame).
#[derive(Clone)]
pub struct FrameRate {
    times: [f64; CAPACITY],
    /// Index of the newest time.
    newest: usize,
    len: usize,
}

impl Default for FrameRate {
    fn default() -> Self {
        Self {
            times: [0.; CAPACITY],
            newest: 0,
            len: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Reading {
    /// Presents per second over the window ending at the reading's time.
    /// An interval that began before the window counts by the fraction
    /// inside it.
    pub fps: f64,
    /// Mean interval between presents inside the window.
    pub frame_ms: Option<f64>,
    /// The longest interval ending inside the window (a stall that began
    /// before it included).
    pub max_ms: Option<f64>,
}

impl FrameRate {
    /// A frame presented at `time` (milliseconds, any origin).
    pub fn present(&mut self, time: f64) {
        if self.len > 0 && time < self.times[self.newest] {
            // A clock that went backwards starts a new record.
            self.len = 0;
        }
        self.newest = (self.newest + 1) % CAPACITY;
        self.times[self.newest] = time;
        self.len = (self.len + 1).min(CAPACITY);
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }

    /// The newest present time.
    pub fn last(&self) -> Option<f64> {
        (self.len > 0).then(|| self.times[self.newest])
    }

    /// The rate over the window ending at `now` (at or after the newest
    /// present; time after it holds no frame yet).
    pub fn reading(&self, now: f64) -> Reading {
        let start = now - WINDOW_MS;
        let mut frames = 0.;
        let (mut sum, mut whole) = (0., 0u32);
        let mut max: Option<f64> = None;
        // Whether the record reaches back past the window's start.
        let mut covered = false;
        let mut oldest = now;
        let mut later: Option<f64> = None;
        for i in 0..self.len {
            let t = self.times[(self.newest + CAPACITY - i) % CAPACITY];
            if let Some(b) = later {
                let len = b - t;
                if len > 0. {
                    max = Some(max.map_or(len, |m: f64| m.max(len)));
                    if t >= start {
                        frames += 1.;
                        sum += len;
                        whole += 1;
                    } else {
                        frames += (b - start) / len;
                    }
                }
            }
            if t < start {
                covered = true;
                break;
            }
            oldest = t;
            later = Some(t);
        }
        let span = if covered { WINDOW_MS } else { now - oldest };
        Reading {
            fps: if span > 0. { frames * 1000. / span } else { 0. },
            frame_ms: (whole > 0).then(|| sum / f64::from(whole)),
            max_ms: max,
        }
    }
}

/// The readout's two lines: "60 FPS" and "16.7 ms · max 18", or "idle"
/// while the host draws no frames. Rewrites `top`/`bottom` in place.
pub fn text(reading: Option<&Reading>, top: &mut String, bottom: &mut String) {
    top.clear();
    bottom.clear();
    let Some(r) = reading else {
        top.push_str("idle");
        return;
    };
    let _ = write!(top, "{:.0} FPS", r.fps);
    if let Some(ms) = r.frame_ms {
        let _ = write!(bottom, "{ms:.1} ms");
    }
    if let Some(max) = r.max_ms {
        if !bottom.is_empty() {
            bottom.push_str(" · ");
        }
        let _ = write!(bottom, "max {max:.0}");
    }
}

const MONO: &str = "ui-monospace, 'SF Mono', Menlo, Consolas, 'DejaVu Sans Mono', monospace";
const TOP_SIZE: f32 = 12.;
const BOTTOM_SIZE: f32 = 9.;
const PAD: f32 = 5.;

fn style(size: f32, weight: u16, color: Color) -> TextStyle {
    TextStyle::new(
        Font {
            family: MONO,
            size,
            weight,
            italic: false,
        },
        color,
    )
}

/// The readout's size in CSS pixels for lines of `top`/`bottom` characters.
pub fn size(fonts: &mut Fonts, top: usize, bottom: usize) -> (f32, f32) {
    let top_w = fonts
        .measure("0", &style(TOP_SIZE, 700, Color::WHITE).font, 0.)
        .width;
    let bottom_w = fonts
        .measure("0", &style(BOTTOM_SIZE, 500, Color::WHITE).font, 0.)
        .width;
    let w = (top_w * top as f32).max(bottom_w * bottom as f32) + PAD * 2.;
    let h = if bottom > 0 {
        TOP_SIZE + BOTTOM_SIZE + 3. + PAD * 2.
    } else {
        TOP_SIZE + PAD * 2.
    };
    (w.ceil(), h.ceil())
}

/// Paints the readout with its top-left corner at `x`, `y` (CSS pixels).
/// Each character is its own item on whole device pixels (a monospace
/// cell), so a changed number reuses the atlas's glyphs instead of
/// rasterizing a new string: the atlas holds a few dozen glyphs for good.
pub fn paint(
    list: &mut DisplayList,
    fonts: &mut Fonts,
    top: &str,
    bottom: &str,
    x: f32,
    y: f32,
    scale: f32,
) {
    let (w, h) = size(fonts, top.chars().count(), bottom.chars().count());
    list.push(crate::Item::Box(BoxPaint {
        rect: Rect::new(x, y, w, h),
        radii: [5.; 4],
        fill: Some(Color::hex("#000000a6").into()),
        ..Default::default()
    }));
    let snap = |v: f32| (v * scale).round() / scale;
    let mut line = |text: &str, style: &TextStyle, baseline: f32| {
        let advance = fonts.measure("0", &style.font, 0.).width;
        for (i, c) in text.chars().enumerate() {
            if c == ' ' {
                continue;
            }
            let mut glyph = [0u8; 4];
            list.text(
                &*c.encode_utf8(&mut glyph),
                style,
                snap(x + PAD + advance * i as f32),
                snap(baseline),
            );
        }
    };
    let top_style = style(TOP_SIZE, 700, Color::hex("#f1d84e"));
    let bottom_style = style(BOTTOM_SIZE, 500, Color::hex("#e9e9e9"));
    line(top, &top_style, y + PAD + TOP_SIZE * 0.82);
    if !bottom.is_empty() {
        line(
            bottom,
            &bottom_style,
            y + PAD + TOP_SIZE + 3. + BOTTOM_SIZE * 0.82,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(rate: &mut FrameRate, start: f64, interval: f64, frames: usize) -> f64 {
        let mut t = start;
        for _ in 0..frames {
            t += interval;
            rate.present(t);
        }
        t
    }

    #[test]
    fn steady_rates_read_as_presented() {
        let mut rate = FrameRate::default();
        let t = run(&mut rate, 0., 1000. / 60., 180);
        let r = rate.reading(t);
        assert!((r.fps - 60.).abs() < 0.5, "{r:?}");
        assert!((r.frame_ms.unwrap() - 16.67).abs() < 0.05);
        assert!((r.max_ms.unwrap() - 16.67).abs() < 0.05);

        let mut rate = FrameRate::default();
        let t = run(&mut rate, 0., 1000. / 30., 90);
        let r = rate.reading(t);
        assert!((r.fps - 30.).abs() < 0.5, "{r:?}");
        assert!((r.frame_ms.unwrap() - 33.33).abs() < 0.05);
        let (mut top, mut bottom) = (String::new(), String::new());
        text(Some(&r), &mut top, &mut bottom);
        assert_eq!(
            (top.as_str(), bottom.as_str()),
            ("30 FPS", "33.3 ms · max 33")
        );
    }

    #[test]
    fn dropped_frames_lower_the_rate_and_show_as_max() {
        let mut rate = FrameRate::default();
        let frame = 1000. / 60.;
        let mut t = run(&mut rate, 0., frame, 120);
        // Three frames missed: one 66.7 ms interval.
        t += frame * 4.;
        rate.present(t);
        t = run(&mut rate, t, frame, 30);
        let r = rate.reading(t);
        assert!((r.fps - 57.).abs() < 0.6, "{r:?}");
        assert!((r.max_ms.unwrap() - 66.7).abs() < 0.1, "{r:?}");
        // Two seconds of steady frames later the stall has left the window.
        let t = run(&mut rate, t, frame, 120);
        let r = rate.reading(t);
        assert!(
            (r.fps - 60.).abs() < 0.5 && r.max_ms.unwrap() < 17.,
            "{r:?}"
        );
    }

    #[test]
    fn on_demand_frames_read_low_not_sixty() {
        // A menu idle for 5 s draws 12 frames at 60 Hz for an input.
        let mut rate = FrameRate::default();
        rate.present(0.);
        let t = run(&mut rate, 5000., 1000. / 60., 12);
        let r = rate.reading(t);
        // 11 whole intervals plus the idle gap's part inside the window.
        assert!(r.fps > 11. && r.fps < 11.3, "{r:?}");
        assert!((r.frame_ms.unwrap() - 16.67).abs() < 0.05);
        assert!(r.max_ms.unwrap() > 5000.);
        // Read later with no new frame: the rate falls to nothing.
        assert!(rate.reading(t + 900.).fps < 7.);
        assert_eq!(rate.reading(t + 1500.).fps, 0.);
        // Too few presents: no frame time.
        let mut rate = FrameRate::default();
        rate.present(10.);
        let r = rate.reading(10.);
        assert_eq!((r.fps, r.frame_ms, r.max_ms), (0., None, None));
        let (mut top, mut bottom) = (String::new(), String::new());
        text(None, &mut top, &mut bottom);
        assert_eq!((top.as_str(), bottom.as_str()), ("idle", ""));
    }

    #[test]
    fn the_ring_never_grows() {
        let mut rate = FrameRate::default();
        let t = run(&mut rate, 0., 1., 5000);
        // 1 kHz for 5 s: the ring keeps the newest 512 (a partial window).
        let r = rate.reading(t);
        assert!((r.fps - 1000.).abs() < 1., "{r:?}");
        assert_eq!(rate.len, CAPACITY);
    }
}
