//! The diagnostics overlay (web/diagnostics.js): F3 cycles Off, Performance
//! and Debug; the mode saves under `atv.diagnostics.v1`. The host supplies the
//! lines; this keeps the frame statistics and draws the panel
//! (`#diagnostics-overlay`: top right, dark translucent box, 11 px monospace,
//! lines wrapped within the screen).
use crate::layout::{El, Paint, margins, render, sides};
use crate::{Color, DisplayList, Fonts, TextStyle, text::Font};
use taffy::prelude::*;

pub const KEY: &str = "atv.diagnostics.v1";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Off,
    Performance,
    Debug,
}

impl Mode {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "off" => Some(Self::Off),
            "performance" => Some(Self::Performance),
            "debug" => Some(Self::Debug),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Performance => "performance",
            Self::Debug => "debug",
        }
    }

    /// F3: Off -> Performance -> Debug -> Off.
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::Performance,
            Self::Performance => Self::Debug,
            Self::Debug => Self::Off,
        }
    }
}

/// FrameStats: the last 120 frame intervals and CPU costs.
#[derive(Clone, Debug, Default)]
pub struct FrameStats {
    previous: Option<f64>,
    intervals: Vec<f64>,
    costs: Vec<f64>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Snapshot {
    pub fps: Option<f64>,
    pub frame_ms: Option<f64>,
    pub p95_ms: Option<f64>,
    pub cpu_ms: Option<f64>,
}

impl FrameStats {
    /// One rendered frame at `time` ms costing `cpu_ms`; inactive frames
    /// break the interval chain.
    pub fn sample(&mut self, time: f64, cpu_ms: f64, active: bool) {
        if !active {
            self.previous = None;
            return;
        }
        if let Some(previous) = self.previous {
            let interval = time - previous;
            if interval > 0. && interval < 1000. {
                self.intervals.push(interval);
                if self.intervals.len() > 120 {
                    self.intervals.remove(0);
                }
            }
        }
        self.previous = Some(time);
        if cpu_ms.is_finite() && cpu_ms >= 0. {
            self.costs.push(cpu_ms);
            if self.costs.len() > 120 {
                self.costs.remove(0);
            }
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        let mean = |v: &[f64]| (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64);
        let frame_ms = mean(&self.intervals);
        let mut sorted = self.intervals.clone();
        sorted.sort_by(f64::total_cmp);
        Snapshot {
            fps: frame_ms.map(|ms| 1000. / ms),
            frame_ms,
            p95_ms: (!sorted.is_empty())
                .then(|| sorted[((sorted.len() as f64 * 0.95).ceil() as usize).max(1) - 1]),
            cpu_ms: mean(&self.costs),
        }
    }
}

/// `number()` in diagnostics.js: fixed places, "—" when unknown.
pub fn number(value: Option<f64>, places: usize) -> String {
    match value.filter(|v| v.is_finite()) {
        Some(v) => format!("{v:.places$}"),
        None => "—".into(),
    }
}

/// `vector()`: components at two places.
pub fn vector(value: &[f32]) -> String {
    value
        .iter()
        .map(|v| number(Some(f64::from(*v)), 2))
        .collect::<Vec<_>>()
        .join(" / ")
}

/// The panel with `lines` in a `width` x `height` viewport (CSS pixels).
pub fn paint(list: &mut DisplayList, fonts: &mut Fonts, width: f32, height: f32, lines: &[String]) {
    let narrow = width <= 600.;
    let size = if narrow { 10. } else { 11. };
    let style = TextStyle::new(
        Font {
            family: "ui-monospace, 'SF Mono', Menlo, Consolas, 'DejaVu Sans Mono', monospace",
            size,
            weight: 400,
            italic: false,
        },
        Color::hex("#e5f7ff"),
    );
    let mut panel = El::new(Style {
        position: Position::Absolute,
        inset: taffy::Rect {
            top: LengthPercentageAuto::length(if narrow { 72. } else { 14. }),
            right: LengthPercentageAuto::length(14.),
            bottom: LengthPercentageAuto::auto(),
            left: LengthPercentageAuto::auto(),
        },
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        max_size: Size {
            width: length(width - 28.),
            height: length(height - 100.),
        },
        padding: if narrow {
            sides(8., 8., 8., 8.)
        } else {
            sides(10., 12., 10., 12.)
        },
        border: sides(1., 1., 1., 1.),
        overflow: taffy::Point {
            x: taffy::Overflow::Hidden,
            y: taffy::Overflow::Hidden,
        },
        ..Default::default()
    })
    .paint(Paint {
        fill: Some(Color::hex("#06121de8").into()),
        border: [1.; 4],
        border_color: Color::hex("#8ca7b655"),
        ..Default::default()
    });
    for line in lines {
        panel = panel.child(
            El::new(Style {
                margin: margins(0., 0., 0., 0.),
                ..Default::default()
            })
            .content(crate::layout::Content::Text {
                text: line.clone(),
                style: style.clone(),
                line_height: size * 1.55,
                wrap: true,
            }),
        );
    }
    let root = El::new(Style {
        size: Size {
            width: length(width),
            height: length(height),
        },
        ..Default::default()
    })
    .child(panel);
    render(&root, fonts, width, height, list, &mut Vec::new());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f3_cycles_and_stats_follow_the_browser() {
        assert_eq!(Mode::default(), Mode::Off);
        assert_eq!(Mode::Off.next().next().next(), Mode::Off);
        assert_eq!(Mode::parse("debug"), Some(Mode::Debug));
        let mut stats = FrameStats::default();
        for i in 0..21 {
            stats.sample(f64::from(i) * 20., 2., true);
        }
        let s = stats.snapshot();
        assert_eq!(number(s.fps, 1), "50.0");
        assert_eq!(number(s.p95_ms, 1), "20.0");
        assert_eq!(number(s.cpu_ms, 2), "2.00");
        assert_eq!(number(None, 1), "—");
        assert_eq!(vector(&[1., -2.5]), "1.00 / -2.50");
    }
}
