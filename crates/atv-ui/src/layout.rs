//! A small element tree laid out with taffy (CSS flexbox/grid) and painted
//! into a [`DisplayList`]. Text leaves follow CSS line boxes: `line-height`
//! with half-leading around the font's ascent/descent, greedy wrapping, and
//! first baselines for `align-items: baseline`. Inline rows lay out text runs
//! and inline-block chips (e.g. `<kbd>`) on one baseline.
use crate::{
    color::{Color, Fill},
    paint::{BoxPaint, BoxShadow, DisplayList, ImageId, Rect},
    text::{Font, Fonts, Shape, TextStyle},
};
use taffy::prelude::*;

/// Box decoration (CSS background, border, radius, shadows).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Paint {
    pub radii: [f32; 4],
    /// Elliptical corners' vertical radii (0: circular).
    pub radii_y: [f32; 4],
    pub fill: Option<Fill>,
    pub border: [f32; 4],
    pub border_color: Color,
    pub shadows: Vec<BoxShadow>,
    /// CSS `transform: skewX()` about the box's top-left corner: tan(angle).
    pub skew_x: f32,
}

impl Paint {
    pub fn fill(fill: impl Into<Fill>) -> Self {
        Self {
            fill: Some(fill.into()),
            ..Default::default()
        }
    }
    pub fn radius(mut self, r: f32) -> Self {
        self.radii = [r; 4];
        self
    }
    pub fn shadow(mut self, shadow: BoxShadow) -> Self {
        self.shadows.push(shadow);
        self
    }
    fn visible(&self) -> bool {
        self.fill.is_some() || !self.shadows.is_empty() || self.border.iter().any(|w| *w > 0.)
    }
    fn at(&self, rect: Rect) -> BoxPaint {
        BoxPaint {
            rect,
            radii: self.radii,
            radii_y: self.radii_y,
            fill: self.fill.clone(),
            border: self.border,
            border_color: self.border_color,
            shadows: self.shadows.clone(),
            skew_x: self.skew_x,
            pivot: [rect.x, rect.y],
            ..Default::default()
        }
    }
}

/// CSS `object-fit` for image content.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Fit {
    #[default]
    Cover,
    Contain,
    Fill,
}

/// One run of an inline row.
#[derive(Clone, Debug, PartialEq)]
pub enum Inline {
    /// Text in an inline box: `margin` left/right.
    Text {
        text: String,
        style: TextStyle,
        line_height: f32,
        margin: [f32; 2],
    },
    /// An `inline-block` with one line of text (e.g. `<kbd>`).
    Chip {
        text: String,
        style: TextStyle,
        line_height: f32,
        /// Top, right, bottom, left.
        padding: [f32; 4],
        min_width: f32,
        margin: [f32; 2],
        paint: Paint,
    },
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum Content {
    #[default]
    None,
    /// Block text; wraps at spaces when `wrap`.
    Text {
        text: String,
        style: TextStyle,
        line_height: f32,
        wrap: bool,
    },
    /// One line of inline runs. `strut` is the container's font and line height.
    Inline {
        strut: (Font, f32),
        runs: Vec<Inline>,
    },
    /// A vector icon drawn at the content box origin (CSS `width/height`).
    Shape(Shape),
    /// A registered image of `size` texels filling the content box.
    Image {
        image: ImageId,
        size: [f32; 2],
        fit: Fit,
        /// CSS `filter: drop-shadow(dx dy blur color)`.
        shadow: Option<crate::text::Shadow>,
    },
}

#[derive(Clone, Debug, Default)]
pub struct El {
    pub style: Style,
    pub paint: Paint,
    pub content: Content,
    pub children: Vec<El>,
    /// Hit-test target.
    pub id: Option<u32>,
    /// CSS `opacity` (applied to the element's own paint and descendants).
    pub opacity: f32,
    /// `overflow: auto` scroll position: children move up by this much and
    /// are clipped to the padding box (radius = the box's first radius).
    pub scroll: Option<f32>,
}

impl El {
    pub fn new(style: Style) -> Self {
        Self {
            style,
            opacity: 1.,
            ..Default::default()
        }
    }
    pub fn paint(mut self, paint: Paint) -> Self {
        self.paint = paint;
        self
    }
    pub fn content(mut self, content: Content) -> Self {
        self.content = content;
        self
    }
    pub fn child(mut self, child: El) -> Self {
        self.children.push(child);
        self
    }
    pub fn children(mut self, children: impl IntoIterator<Item = El>) -> Self {
        self.children.extend(children);
        self
    }
    pub fn id(mut self, id: u32) -> Self {
        self.id = Some(id);
        self
    }
    pub fn scroll(mut self, offset: f32) -> Self {
        self.scroll = Some(offset);
        self
    }
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }
    pub fn text(text: impl Into<String>, style: TextStyle, line_height: f32) -> Self {
        Self::new(Style::default()).content(Content::Text {
            text: text.into(),
            style,
            line_height,
            wrap: true,
        })
    }
}

/// Size, first baseline and wrapped lines of a text leaf.
fn text_lines(
    fonts: &mut Fonts,
    text: &str,
    style: &TextStyle,
    line_height: f32,
    wrap: bool,
    width: Option<f32>,
) -> (Vec<String>, f32, f32) {
    let lines = match (wrap, width) {
        (true, Some(w)) => fonts.wrap(text, &style.font, style.letter_spacing, w),
        _ => vec![text.to_string()],
    };
    let widest = lines
        .iter()
        .map(|l| fonts.measure(l, &style.font, style.letter_spacing).width)
        .fold(0., f32::max);
    let m = fonts.measure("", &style.font, style.letter_spacing);
    let baseline = css_baseline(line_height, m.ascent, m.descent);
    (lines, widest, baseline)
}

/// Inline run geometry: (x, width, top relative to baseline, bottom relative to
/// baseline, baseline offset inside the run box).
struct RunBox {
    x: f32,
    width: f32,
    above: f32,
    below: f32,
}

fn inline_layout(
    fonts: &mut Fonts,
    strut: &(Font, f32),
    runs: &[Inline],
) -> (Vec<RunBox>, f32, f32, f32) {
    let half = |font: &Font, line_height: f32, fonts: &mut Fonts| {
        let m = fonts.measure("", font, 0.);
        let above = css_baseline(line_height, m.ascent, m.descent);
        (above, line_height - above)
    };
    let (mut above, mut below) = half(&strut.0, strut.1, fonts);
    let mut x = 0.;
    let mut boxes = Vec::new();
    for run in runs {
        match run {
            Inline::Text {
                text,
                style,
                line_height,
                margin,
            } => {
                let w = fonts.measure(text, &style.font, style.letter_spacing).width;
                let (a, b) = half(&style.font, *line_height, fonts);
                above = above.max(a);
                below = below.max(b);
                boxes.push(RunBox {
                    x: x + margin[0],
                    width: w,
                    above: a,
                    below: b,
                });
                x += margin[0] + w + margin[1];
            }
            Inline::Chip {
                text,
                style,
                line_height,
                padding,
                min_width,
                margin,
                ..
            } => {
                let w = fonts.measure(text, &style.font, style.letter_spacing).width;
                // box-sizing: border-box (game.css `*`): min-width includes padding.
                let width = (w + padding[1] + padding[3]).max(*min_width);
                let (a, b) = half(&style.font, *line_height, fonts);
                // inline-block baseline: its line's baseline.
                let (ca, cb) = (a + padding[0], b + padding[2]);
                above = above.max(ca);
                below = below.max(cb);
                boxes.push(RunBox {
                    x: x + margin[0],
                    width,
                    above: ca,
                    below: cb,
                });
                x += margin[0] + width + margin[1];
            }
        }
    }
    (boxes, x, above, below)
}

fn build<'a>(tree: &mut TaffyTree<usize>, el: &'a El, contents: &mut Vec<&'a Content>) -> NodeId {
    let children: Vec<NodeId> = el
        .children
        .iter()
        .map(|c| build(tree, c, contents))
        .collect();
    if children.is_empty() && el.content != Content::None {
        contents.push(&el.content);
        tree.new_leaf_with_context(el.style.clone(), contents.len() - 1)
            .unwrap()
    } else {
        tree.new_with_children(el.style.clone(), &children).unwrap()
    }
}

/// Lays `root` out in a `width` x `height` viewport and paints it.
pub fn render(
    root: &El,
    fonts: &mut Fonts,
    width: f32,
    height: f32,
    list: &mut DisplayList,
    hits: &mut Vec<(Rect, u32)>,
) {
    let mut tree: TaffyTree<usize> = TaffyTree::new();
    // Fractional boxes like browser layout; rounding would wrap measured text.
    tree.disable_rounding();
    let mut contents: Vec<&Content> = Vec::new();
    let node = build(&mut tree, root, &mut contents);
    tree.compute_layout_with_measure(
        node,
        Size {
            width: AvailableSpace::Definite(width),
            height: AvailableSpace::Definite(height),
        },
        |inputs, _id, context, style| {
            let Some(&mut index) = context else {
                return taffy::compute_leaf_layout(inputs, style, |_, _| 0., |_, _| Size::ZERO);
            };
            let content = contents[index];
            let mut baseline = None;
            let mut output = taffy::compute_leaf_layout(
                inputs,
                style,
                |_, _| 0.,
                |known, available| {
                    let width = known.width.or(match available.width {
                        AvailableSpace::Definite(w) => Some(w),
                        AvailableSpace::MinContent => Some(0.),
                        AvailableSpace::MaxContent => None,
                    });
                    match content {
                        Content::Text {
                            text,
                            style,
                            line_height,
                            wrap,
                        } => {
                            let (lines, widest, b) =
                                text_lines(fonts, text, style, *line_height, *wrap, width);
                            baseline = Some(b);
                            Size {
                                width: known.width.unwrap_or(widest),
                                height: known.height.unwrap_or(lines.len() as f32 * line_height),
                            }
                        }
                        Content::Inline { strut, runs } => {
                            let (_, w, above, below) = inline_layout(fonts, strut, runs);
                            baseline = Some(above);
                            Size {
                                width: known.width.unwrap_or(w),
                                height: known.height.unwrap_or(above + below),
                            }
                        }
                        _ => Size::ZERO,
                    }
                },
            );
            if let Some(b) = baseline {
                let pad = length(style.padding.top) + length(style.border.top);
                output.baselines.first = Some(b + pad);
            }
            output
        },
    )
    .unwrap();
    paint(&tree, node, root, 0., 0., fonts, list, hits);
}

#[allow(clippy::too_many_arguments)]
fn paint(
    tree: &TaffyTree<usize>,
    node: NodeId,
    el: &El,
    ox: f32,
    oy: f32,
    fonts: &mut Fonts,
    list: &mut DisplayList,
    hits: &mut Vec<(Rect, u32)>,
) {
    let layout = tree.layout(node).unwrap();
    let rect = Rect::new(
        ox + layout.location.x,
        oy + layout.location.y,
        layout.size.width,
        layout.size.height,
    );
    if el.style.display == Display::None {
        return;
    }
    let grouped = el.opacity < 1.;
    if grouped {
        list.opacity(el.opacity);
    }
    if el.paint.visible() {
        list.push(crate::paint::Item::Box(el.paint.at(rect)));
    }
    if let Some(id) = el.id {
        hits.push((rect, id));
    }
    let content = rect.inset(
        layout.padding.top + layout.border.top,
        layout.padding.right + layout.border.right,
        layout.padding.bottom + layout.border.bottom,
        layout.padding.left + layout.border.left,
    );
    match &el.content {
        Content::Text {
            text,
            style,
            line_height,
            wrap,
        } => {
            let (lines, _, baseline) =
                text_lines(fonts, text, style, *line_height, *wrap, Some(content.w));
            for (i, line) in lines.iter().enumerate() {
                list.text(
                    line,
                    style,
                    content.x,
                    content.y + i as f32 * line_height + baseline,
                );
            }
        }
        Content::Inline { strut, runs } => {
            let (boxes, _, above, _) = inline_layout(fonts, strut, runs);
            let base = content.y + above;
            for (run, b) in runs.iter().zip(&boxes) {
                match run {
                    Inline::Text { text, style, .. } => {
                        list.text(text, style, content.x + b.x, base)
                    }
                    Inline::Chip {
                        text,
                        style,
                        padding,
                        paint,
                        ..
                    } => {
                        let chip =
                            Rect::new(content.x + b.x, base - b.above, b.width, b.above + b.below);
                        list.push(crate::paint::Item::Box(paint.at(chip)));
                        let w = fonts.measure(text, &style.font, style.letter_spacing).width;
                        // text-align: center inside the chip.
                        let inner = chip.w - padding[1] - padding[3];
                        list.text(text, style, chip.x + padding[3] + (inner - w) / 2., base);
                    }
                }
            }
        }
        Content::Shape(shape) => list.push(crate::paint::Item::Shape {
            shape: shape.clone(),
            x: content.x,
            y: content.y,
            rotation: 0.,
        }),
        Content::Image {
            image,
            size,
            fit,
            shadow,
        } => {
            let [iw, ih] = *size;
            if iw > 0. && ih > 0. && content.w > 0. && content.h > 0. {
                let (src, dst) = match fit {
                    Fit::Fill => (Rect::new(0., 0., iw, ih), content),
                    Fit::Cover => {
                        let s = (content.w / iw).max(content.h / ih);
                        let (sw, sh) = (content.w / s, content.h / s);
                        (Rect::new((iw - sw) / 2., (ih - sh) / 2., sw, sh), content)
                    }
                    Fit::Contain => {
                        let s = (content.w / iw).min(content.h / ih);
                        let (dw, dh) = (iw * s, ih * s);
                        (
                            Rect::new(0., 0., iw, ih),
                            Rect::new(
                                content.x + (content.w - dw) / 2.,
                                content.y + (content.h - dh) / 2.,
                                dw,
                                dh,
                            ),
                        )
                    }
                };
                if let Some(sh) = shadow {
                    list.push(crate::paint::Item::ImageShadow {
                        image: *image,
                        src,
                        dst,
                        dx: sh.dx,
                        dy: sh.dy,
                        blur: sh.blur,
                        color: sh.color,
                    });
                }
                list.push(crate::paint::Item::Image {
                    image: *image,
                    src,
                    dst,
                    rotation: 0.,
                    pivot: [0., 0.],
                });
            }
        }
        Content::None => {}
    }
    if let Some(offset) = el.scroll {
        list.clip(rect, el.paint.radii[0]);
        for (i, child) in el.children.iter().enumerate() {
            let id = tree.child_at_index(node, i).unwrap();
            paint(tree, id, child, rect.x, rect.y - offset, fonts, list, hits);
        }
        list.unclip();
    } else {
        for (i, child) in el.children.iter().enumerate() {
            let id = tree.child_at_index(node, i).unwrap();
            paint(tree, id, child, rect.x, rect.y, fonts, list, hits);
        }
    }
    if grouped {
        list.unopacity();
    }
}

/// Shorthands for taffy styles.
pub fn px(v: f32) -> LengthPercentage {
    LengthPercentage::length(v)
}
pub fn sides(t: f32, r: f32, b: f32, l: f32) -> taffy::Rect<LengthPercentage> {
    taffy::Rect {
        top: px(t),
        right: px(r),
        bottom: px(b),
        left: px(l),
    }
}
pub fn margins(t: f32, r: f32, b: f32, l: f32) -> taffy::Rect<LengthPercentageAuto> {
    taffy::Rect {
        top: LengthPercentageAuto::length(t),
        right: LengthPercentageAuto::length(r),
        bottom: LengthPercentageAuto::length(b),
        left: LengthPercentageAuto::length(l),
    }
}

fn length(value: LengthPercentage) -> f32 {
    let raw = value.into_raw();
    if raw.tag() == taffy::CompactLength::LENGTH_TAG {
        raw.value()
    } else {
        0.
    }
}

/// Baseline offset in a CSS line box: Blink lays text out with the font's
/// ascent and descent rounded to whole pixels and floors the half-leading.
pub fn css_baseline(line_height: f32, ascent: f32, descent: f32) -> f32 {
    let (ascent, descent) = (ascent.round(), descent.round());
    ((line_height - (ascent + descent)) / 2.).floor() + ascent
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn absolute_children_use_the_positioned_parent() {
        let track = El::new(Style {
            position: Position::Absolute,
            inset: taffy::Rect {
                left: LengthPercentageAuto::length(0.),
                right: LengthPercentageAuto::length(0.),
                top: LengthPercentageAuto::length(18.),
                bottom: LengthPercentageAuto::auto(),
            },
            size: Size {
                width: Dimension::auto(),
                height: Dimension::length(8.),
            },
            ..Default::default()
        })
        .id(2);
        // taffy follows CSS: absolute boxes use the nearest positioned ancestor.
        let input = El::new(Style {
            position: Position::Relative,
            size: Size {
                width: Dimension::auto(),
                height: Dimension::length(44.),
            },
            ..Default::default()
        })
        .id(1)
        .child(track);
        let root = El::new(Style {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            padding: sides(10., 22., 10., 22.),
            size: Size {
                width: Dimension::length(640.),
                height: Dimension::auto(),
            },
            ..Default::default()
        })
        .child(input);
        let mut hits = Vec::new();
        render(
            &root,
            &mut Fonts::new(),
            1280.,
            720.,
            &mut DisplayList::default(),
            &mut hits,
        );
        let get = |id| hits.iter().find(|(_, i)| *i == id).unwrap().0;
        assert_eq!(get(1).w, 596.);
        assert_eq!(get(2).w, 596., "{:?}", get(2));
    }
}
