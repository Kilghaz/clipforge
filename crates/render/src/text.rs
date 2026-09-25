//! Text items: layout with `parley`, glyphs with `swash`, from the bundled
//! fonts only (all SIL Open Font License), so every machine renders the
//! same pixels.
//!
//! A text becomes a straight-alpha RGBA image of its block (glyphs plus
//! shadow or background box) with the text box's position inside it. The
//! image depends on the text, style and box width, not on the position, so
//! moving a text reuses the cached image. Both compositors draw it over the
//! finished frame with the item's entrance / exit movement.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use clipforge_core::text::{FRAME_UNITS, Font, TextAlign, TextItem, TextMotion, TextStyle};
use parley::fontique::{Blob, Collection, CollectionOptions, SourceCache};
use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, FontStyle, FontWeight, Layout,
    LayoutContext, LineHeight, PositionedLayoutItem, StyleProperty,
};
use swash::FontRef;
use swash::scale::{Render, ScaleContext, Source};
use swash::zeno::{Angle, Format, Transform, Vector};

use crate::draw::RectF;
use crate::frame::Frame;

/// The bundled fonts, in `Font::ALL` order.
static FONTS: [&[u8]; 6] = [
    include_bytes!("../../../assets/fonts/InterVariable.ttf"),
    include_bytes!("../../../assets/fonts/Montserrat.ttf"),
    include_bytes!("../../../assets/fonts/PlayfairDisplay.ttf"),
    include_bytes!("../../../assets/fonts/BebasNeue.ttf"),
    include_bytes!("../../../assets/fonts/DancingScript.ttf"),
    include_bytes!("../../../assets/fonts/Caveat.ttf"),
];

/// Rendered text blocks kept per renderer.
const CACHE_ENTRIES: usize = 48;

/// A rendered text block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextImage {
    pub width: u32,
    pub height: u32,
    /// Straight-alpha RGBA.
    pub rgba: Vec<u8>,
    /// Top-left of the text box inside the image (the rest is room for the
    /// shadow or the background box).
    pub box_x: u32,
    pub box_y: u32,
    /// Size of the text box: the item's width × the laid-out height.
    pub box_width: u32,
    pub box_height: u32,
}

impl TextImage {
    /// Where the image goes in a `w × h` frame for `item` (no movement).
    #[must_use]
    pub fn rect(&self, item: &TextItem, (w, h): (u32, u32)) -> RectF {
        let b = box_rect(item, self, (w, h));
        RectF {
            x: b.x - f64::from(self.box_x),
            y: b.y - f64::from(self.box_y),
            width: f64::from(self.width),
            height: f64::from(self.height),
        }
    }
}

/// The text box of `item` in frame pixels.
#[must_use]
pub fn box_rect(item: &TextItem, img: &TextImage, (w, h): (u32, u32)) -> RectF {
    let cx = f64::from(item.x) / f64::from(FRAME_UNITS) * f64::from(w);
    let cy = f64::from(item.y) / f64::from(FRAME_UNITS) * f64::from(h);
    RectF {
        x: (cx - f64::from(img.box_width) / 2.0).round(),
        y: (cy - f64::from(img.box_height) / 2.0).round(),
        width: f64::from(img.box_width),
        height: f64::from(img.box_height),
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
    text: String,
    style: TextStyle,
    box_width: u32,
    short_side: u32,
}

struct Engine {
    fonts: FontContext,
    layouts: LayoutContext<[u8; 4]>,
    scaler: ScaleContext,
    /// Family name per `Font`, from the font files themselves.
    families: Vec<String>,
}

struct Cache {
    images: HashMap<Key, Arc<TextImage>>,
    order: Vec<Key>,
}

/// Lays out and rasterises text items. Owned by each compositor (and by
/// the editor, for hit boxes).
pub struct TextRenderer {
    engine: Mutex<Engine>,
    cache: Mutex<Cache>,
}

impl std::fmt::Debug for TextRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextRenderer").finish_non_exhaustive()
    }
}

impl Default for TextRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl TextRenderer {
    #[must_use]
    pub fn new() -> TextRenderer {
        // Only the bundled fonts: no system fonts, so output is identical
        // on every machine.
        let mut collection = Collection::new(CollectionOptions {
            shared: false,
            system_fonts: false,
        });
        let families = FONTS
            .iter()
            .zip(Font::ALL)
            .map(|(data, font)| {
                let registered = collection.register_fonts(Blob::new(Arc::new(*data)), None);
                registered
                    .first()
                    .and_then(|(id, _)| collection.family_name(*id).map(str::to_owned))
                    .unwrap_or_else(|| format!("{font:?}"))
            })
            .collect();
        TextRenderer {
            engine: Mutex::new(Engine {
                fonts: FontContext {
                    collection,
                    source_cache: SourceCache::default(),
                },
                layouts: LayoutContext::new(),
                scaler: ScaleContext::new(),
                families,
            }),
            cache: Mutex::new(Cache {
                images: HashMap::new(),
                order: Vec::new(),
            }),
        }
    }

    /// Family name of a bundled font (as the UI toolkit knows it too).
    #[must_use]
    pub fn family_name(&self, font: Font) -> String {
        self.engine
            .lock()
            .ok()
            .and_then(|e| e.families.get(font.index()).cloned())
            .unwrap_or_default()
    }

    /// The rendered block of `item` for a `w × h` frame; `None` for empty
    /// text.
    #[must_use]
    pub fn image(&self, item: &TextItem, (w, h): (u32, u32)) -> Option<Arc<TextImage>> {
        if item.text.trim().is_empty() || w == 0 || h == 0 {
            return None;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let box_width = (f64::from(item.width.max(1)) / f64::from(FRAME_UNITS) * f64::from(w))
            .round()
            .max(1.0) as u32;
        let key = Key {
            text: item.text.clone(),
            style: item.style.clone(),
            box_width,
            short_side: w.min(h),
        };
        if let Some(hit) = self
            .cache
            .lock()
            .ok()
            .and_then(|c| c.images.get(&key).cloned())
        {
            return Some(hit);
        }
        let image = {
            let mut engine = self.engine.lock().ok()?;
            Arc::new(render_block(
                &mut engine,
                &item.text,
                &item.style,
                box_width,
                w.min(h),
            )?)
        };
        if let Ok(mut cache) = self.cache.lock() {
            if cache.images.len() >= CACHE_ENTRIES && !cache.order.is_empty() {
                let oldest = cache.order.remove(0);
                cache.images.remove(&oldest);
            }
            cache.order.push(key.clone());
            cache.images.insert(key, Arc::clone(&image));
        }
        Some(image)
    }

    /// The text box of `item` in a `w × h` frame (pixels), for hit testing
    /// and selection handles. Empty text gets a box one line high.
    #[must_use]
    pub fn hit_box(&self, item: &TextItem, (w, h): (u32, u32)) -> RectF {
        if let Some(img) = self.image(item, (w, h)) {
            return box_rect(item, &img, (w, h));
        }
        let line = f64::from(item.style.size) / f64::from(FRAME_UNITS) * f64::from(w.min(h)) * 1.25;
        let bw = f64::from(item.width) / f64::from(FRAME_UNITS) * f64::from(w);
        let cx = f64::from(item.x) / f64::from(FRAME_UNITS) * f64::from(w);
        let cy = f64::from(item.y) / f64::from(FRAME_UNITS) * f64::from(h);
        RectF {
            x: cx - bw / 2.0,
            y: cy - line / 2.0,
            width: bw,
            height: line,
        }
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss
)]
fn render_block(
    engine: &mut Engine,
    text: &str,
    style: &TextStyle,
    box_width: u32,
    short_side: u32,
) -> Option<TextImage> {
    let size = (f32::from(style.size) / FRAME_UNITS as f32 * short_side as f32).max(1.0);
    let text = text.trim_end();
    let family = engine
        .families
        .get(style.font.index())
        .cloned()
        .unwrap_or_default();
    let mut builder = engine
        .layouts
        .ranged_builder(&mut engine.fonts, text, 1.0, true);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named(&family)));
    builder.push_default(StyleProperty::FontSize(size));
    builder.push_default(StyleProperty::FontWeight(FontWeight::new(if style.bold {
        700.0
    } else {
        400.0
    })));
    if style.italic {
        builder.push_default(StyleProperty::FontStyle(FontStyle::Italic));
    }
    builder.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(
        1.25,
    )));
    let mut layout: Layout<[u8; 4]> = builder.build(text);
    layout.break_all_lines(Some(box_width as f32));
    let align = match style.align {
        TextAlign::Left => Alignment::Left,
        TextAlign::Center => Alignment::Center,
        TextAlign::Right => Alignment::Right,
    };
    layout.align(align, AlignmentOptions::default());

    let box_height = layout.height().ceil().max(1.0) as u32;
    let u = short_side as f32 / 1080.0;
    let blur = (6.0 * u).round().max(1.0) as u32;
    let shadow_dy = (3.0 * u).round() as u32;
    // Background padding around the lines, and room for the shadow.
    let pad_bg = if style.background.is_some() {
        (size * 0.3).round() as u32
    } else {
        0
    };
    let margin = pad_bg.max(if style.shadow {
        blur * 2 + shadow_dy
    } else {
        1
    });
    let (cw, ch) = (box_width + 2 * margin, box_height + 2 * margin);
    let (ox, oy) = (margin as f32, margin as f32);

    let (mask, extent) = rasterise(engine, &layout, cw, ch, ox, oy, style.italic);
    let [r, g, b, a] = style.color;
    let mut rgba = vec![0u8; (cw * ch * 4) as usize];
    // Transparent pixels carry the text colour so bilinear sampling (moves,
    // zooms) never pulls in a dark fringe.
    for px in rgba.as_chunks_mut::<4>().0 {
        *px = [r, g, b, 0];
    }
    if let Some(bg) = style.background {
        // A box around the lines actually drawn, not the whole text box.
        let (x0, x1) = extent;
        let bx0 = (ox + x0 - pad_bg as f32).floor().max(0.0) as u32;
        let bx1 = ((ox + x1 + pad_bg as f32).ceil() as u32).min(cw);
        let by0 = margin - pad_bg;
        let by1 = (margin + box_height + pad_bg).min(ch);
        for y in by0..by1 {
            for x in bx0..bx1 {
                let i = ((y * cw + x) * 4) as usize;
                rgba[i..i + 4].copy_from_slice(&bg);
            }
        }
    } else if style.shadow {
        let blurred = box_blur(&mask, cw, ch, blur);
        for y in 0..ch {
            for x in 0..cw {
                let sy = y.checked_sub(shadow_dy);
                let sa = sy.map_or(0, |sy| blurred[(sy * cw + x) as usize]);
                let i = ((y * cw + x) * 4) as usize;
                rgba[i..i + 4].copy_from_slice(&[0, 0, 0, (u32::from(sa) * 180 / 255) as u8]);
            }
        }
    }
    // Glyphs over the shadow / box (straight-alpha "over").
    let text_alpha = f32::from(a) / 255.0;
    for (i, &m) in mask.iter().enumerate() {
        if m == 0 {
            continue;
        }
        let px = &mut rgba[i * 4..i * 4 + 4];
        let sa = f32::from(m) / 255.0 * text_alpha;
        let da = f32::from(px[3]) / 255.0;
        let out_a = sa + da * (1.0 - sa);
        for (c, s) in [r, g, b].into_iter().enumerate() {
            let s = f32::from(s);
            let d = f32::from(px[c]);
            px[c] = ((s * sa + d * da * (1.0 - sa)) / out_a).round() as u8;
        }
        px[3] = (out_a * 255.0).round() as u8;
    }
    Some(TextImage {
        width: cw,
        height: ch,
        rgba,
        box_x: margin,
        box_y: margin,
        box_width,
        box_height,
    })
}

/// Coverage mask (0–255) of the layout on a `cw × ch` canvas, and the
/// horizontal extent of the lines (relative to the layout origin).
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::too_many_arguments
)]
fn rasterise(
    engine: &mut Engine,
    layout: &Layout<[u8; 4]>,
    cw: u32,
    ch: u32,
    origin_x: f32,
    origin_y: f32,
    want_italic: bool,
) -> (Vec<u8>, (f32, f32)) {
    let mut mask = vec![0u8; (cw * ch) as usize];
    let (mut min_x, mut max_x) = (f32::MAX, f32::MIN);
    for line in layout.lines() {
        let m = line.metrics();
        min_x = min_x.min(m.offset);
        max_x = max_x.max(m.offset + m.advance - m.trailing_whitespace);
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                continue;
            };
            let run = glyph_run.run();
            let font = run.font();
            let Some(font_ref) = FontRef::from_index(font.data.as_ref(), font.index as usize)
            else {
                continue;
            };
            let synthesis = run.synthesis();
            let mut scaler = engine
                .scaler
                .builder(font_ref)
                .size(run.font_size())
                .hint(false)
                .normalized_coords(run.normalized_coords())
                .build();
            // Fonts without an italic get a slant; without a bold, a thicker
            // outline.
            let skew = synthesis
                .skew()
                .or(want_italic.then_some(14.0))
                .filter(|a| *a != 0.0);
            let transform =
                skew.map(|deg| Transform::skew(Angle::from_degrees(deg), Angle::from_degrees(0.0)));
            for glyph in glyph_run.positioned_glyphs() {
                let gx = origin_x + glyph.x;
                let gy = origin_y + glyph.y;
                let mut render = Render::new(&[Source::Outline]);
                render
                    .format(Format::Alpha)
                    .transform(transform)
                    .offset(Vector::new(gx.fract(), gy.fract()));
                if synthesis.embolden() {
                    render.embolden(run.font_size() / 30.0);
                }
                let Some(image) = render.render(&mut scaler, glyph.id as u16) else {
                    continue;
                };
                let p = image.placement;
                let left = gx.floor() as i32 + p.left;
                let top = gy.floor() as i32 - p.top;
                for row in 0..p.height as i32 {
                    let y = top + row;
                    if y < 0 || y >= ch as i32 {
                        continue;
                    }
                    for col in 0..p.width as i32 {
                        let x = left + col;
                        if x < 0 || x >= cw as i32 {
                            continue;
                        }
                        let a = image.data[(row * p.width as i32 + col) as usize];
                        let m = &mut mask[(y as u32 * cw + x as u32) as usize];
                        *m = (*m).max(a);
                    }
                }
            }
        }
    }
    if min_x > max_x {
        (min_x, max_x) = (0.0, 0.0);
    }
    (mask, (min_x, max_x))
}

/// Two passes of a separable box blur (close to a Gaussian).
fn box_blur(mask: &[u8], w: u32, h: u32, radius: u32) -> Vec<u8> {
    let (w, h, r) = (w as usize, h as usize, radius as usize);
    let mut a: Vec<u16> = mask.iter().map(|v| u16::from(*v)).collect();
    let mut b = vec![0u16; a.len()];
    let span = (2 * r + 1) as u32;
    for _ in 0..2 {
        for y in 0..h {
            for x in 0..w {
                let lo = x.saturating_sub(r);
                let hi = (x + r).min(w - 1);
                let sum: u32 = a[y * w + lo..=y * w + hi]
                    .iter()
                    .map(|v| u32::from(*v))
                    .sum();
                #[allow(clippy::cast_possible_truncation)]
                {
                    b[y * w + x] = (sum / span) as u16;
                }
            }
        }
        for y in 0..h {
            for x in 0..w {
                let lo = y.saturating_sub(r);
                let hi = (y + r).min(h - 1);
                let sum: u32 = (lo..=hi).map(|yy| u32::from(b[yy * w + x])).sum();
                #[allow(clippy::cast_possible_truncation)]
                {
                    a[y * w + x] = (sum / span) as u16;
                }
            }
        }
    }
    #[allow(clippy::cast_possible_truncation)]
    a.into_iter().map(|v| v.min(255) as u8).collect()
}

/// How a text is drawn at one instant: where, how opaque, what part.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct TextDraw {
    /// Destination of the whole image (after movement and zoom).
    pub dst: RectF,
    pub alpha: f32,
    /// Only this part of the frame shows the text (wipes), frame pixels.
    pub clip: Option<RectF>,
}

/// Smoothstep, as for clip transitions.
fn ease(p: f32) -> f32 {
    let p = p.clamp(0.0, 1.0);
    p * p * (3.0 - 2.0 * p)
}

/// Applies the item's entrance and exit at `t` to its resting rectangle.
/// `None` when the item is not visible.
#[must_use]
pub fn text_draw(
    item: &TextItem,
    img: &TextImage,
    t: clipforge_core::Ticks,
    (w, h): (u32, u32),
) -> Option<TextDraw> {
    if !item.visible_at(t) {
        return None;
    }
    let phase = item.phase_at(t);
    let rest = img.rect(item, (w, h));
    let bx = box_rect(item, img, (w, h));
    let mut draw = TextDraw {
        dst: rest,
        alpha: 1.0,
        clip: None,
    };
    // Slides travel 8 % of the short side.
    let dist = f64::from(w.min(h)) * 0.08;
    for (kind, p, entering) in [
        (phase.enter.0, phase.enter.1, true),
        (phase.exit.0, phase.exit.1, false),
    ] {
        if p >= 1.0 {
            continue;
        }
        let e = ease(p);
        let away = f64::from(1.0 - e) * dist;
        // Entering moves *towards* the named direction and arrives; exiting
        // continues in the named direction and leaves.
        let sign = if entering { 1.0 } else { -1.0 };
        match kind {
            TextMotion::Cut => {}
            TextMotion::Fade => draw.alpha *= e,
            TextMotion::SlideLeft => {
                draw.dst.x += sign * away;
                draw.alpha *= e;
            }
            TextMotion::SlideRight => {
                draw.dst.x -= sign * away;
                draw.alpha *= e;
            }
            TextMotion::SlideUp => {
                draw.dst.y += sign * away;
                draw.alpha *= e;
            }
            TextMotion::SlideDown => {
                draw.dst.y -= sign * away;
                draw.alpha *= e;
            }
            TextMotion::WipeLeft
            | TextMotion::WipeRight
            | TextMotion::WipeUp
            | TextMotion::WipeDown => {
                // The visible part of the block grows (entering) or shrinks
                // (exiting) with the edge moving in the named direction.
                let full = rest;
                let f = f64::from(e);
                let c = match (kind, entering) {
                    (TextMotion::WipeLeft, true) | (TextMotion::WipeRight, false) => RectF {
                        x: full.x + full.width * (1.0 - f),
                        width: full.width * f,
                        ..full
                    },
                    (TextMotion::WipeRight, true) | (TextMotion::WipeLeft, false) => RectF {
                        width: full.width * f,
                        ..full
                    },
                    (TextMotion::WipeUp, true) | (TextMotion::WipeDown, false) => RectF {
                        y: full.y + full.height * (1.0 - f),
                        height: full.height * f,
                        ..full
                    },
                    _ => RectF {
                        height: full.height * f,
                        ..full
                    },
                };
                draw.clip = Some(match draw.clip {
                    Some(prev) => intersect(prev, c),
                    None => c,
                });
            }
            TextMotion::Zoom => {
                let s = 0.7 + 0.3 * f64::from(e);
                let (cx, cy) = (bx.x + bx.width / 2.0, bx.y + bx.height / 2.0);
                draw.dst = RectF {
                    x: cx + (draw.dst.x - cx) * s,
                    y: cy + (draw.dst.y - cy) * s,
                    width: draw.dst.width * s,
                    height: draw.dst.height * s,
                };
                draw.alpha *= e;
            }
        }
    }
    Some(draw)
}

fn intersect(a: RectF, b: RectF) -> RectF {
    let x0 = a.x.max(b.x);
    let y0 = a.y.max(b.y);
    let x1 = (a.x + a.width).min(b.x + b.width);
    let y1 = (a.y + a.height).min(b.y + b.height);
    RectF {
        x: x0,
        y: y0,
        width: (x1 - x0).max(0.0),
        height: (y1 - y0).max(0.0),
    }
}

/// Draws a text image over a frame (CPU): bilinear sampling into `d.dst`,
/// straight-alpha "over", limited to `d.clip`.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss
)]
pub fn draw_text(frame: &mut Frame, img: &TextImage, d: &TextDraw) {
    if d.alpha <= 0.0 || d.dst.width <= 0.0 || d.dst.height <= 0.0 {
        return;
    }
    let (fw, fh) = (i64::from(frame.width), i64::from(frame.height));
    let area = match d.clip {
        Some(c) => intersect(d.dst, c),
        None => d.dst,
    };
    let x0 = area.x.floor().max(0.0) as i64;
    let y0 = area.y.floor().max(0.0) as i64;
    let x1 = ((area.x + area.width).ceil() as i64).min(fw);
    let y1 = ((area.y + area.height).ceil() as i64).min(fh);
    let (iw, ih) = (i64::from(img.width), i64::from(img.height));
    let sx = f64::from(img.width) / d.dst.width;
    let sy = f64::from(img.height) / d.dst.height;
    let texel = |x: i64, y: i64| -> [f32; 4] {
        let x = x.clamp(0, iw - 1);
        let y = y.clamp(0, ih - 1);
        let i = ((y * iw + x) * 4) as usize;
        let a = f32::from(img.rgba[i + 3]) / 255.0;
        // Premultiply for filtering.
        [
            f32::from(img.rgba[i]) * a,
            f32::from(img.rgba[i + 1]) * a,
            f32::from(img.rgba[i + 2]) * a,
            a,
        ]
    };
    for y in y0..y1 {
        let v = (y as f64 + 0.5 - d.dst.y) * sy - 0.5;
        let vy = v.floor();
        let fy = (v - vy) as f32;
        for x in x0..x1 {
            // Hard wipe edge: pixel centres decide.
            if let Some(c) = d.clip {
                let px = x as f64 + 0.5;
                let py = y as f64 + 0.5;
                if px < c.x || px >= c.x + c.width || py < c.y || py >= c.y + c.height {
                    continue;
                }
            }
            let u = (x as f64 + 0.5 - d.dst.x) * sx - 0.5;
            let ux = u.floor();
            let fx = (u - ux) as f32;
            let (ix, iy) = (ux as i64, vy as i64);
            let q = [
                texel(ix, iy),
                texel(ix + 1, iy),
                texel(ix, iy + 1),
                texel(ix + 1, iy + 1),
            ];
            let mut s = [0.0f32; 4];
            for (k, v) in s.iter_mut().enumerate() {
                let top = q[0][k] + (q[1][k] - q[0][k]) * fx;
                let bottom = q[2][k] + (q[3][k] - q[2][k]) * fx;
                *v = top + (bottom - top) * fy;
            }
            let alpha = s[3] * d.alpha;
            if alpha <= 0.0 {
                continue;
            }
            let di = ((y * fw + x) * 4) as usize;
            // `s` is premultiplied by the texel alpha.
            for (out, src) in frame.rgba[di..di + 3].iter_mut().zip(&s[..3]) {
                let blended = src * d.alpha + f32::from(*out) * (1.0 - alpha);
                *out = blended.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipforge_core::Ticks;

    fn item(text: &str) -> TextItem {
        TextItem::new(text, Ticks::ZERO, Ticks::from_seconds(4))
    }

    fn coverage(img: &TextImage) -> usize {
        img.rgba
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[3] > 128)
            .count()
    }

    #[test]
    fn empty_text_renders_nothing_but_has_a_hit_box() {
        let t = TextRenderer::new();
        assert!(t.image(&item("  \n"), (1920, 1080)).is_none());
        let b = t.hit_box(&item(""), (1920, 1080));
        assert!((b.x + b.width / 2.0 - 960.0).abs() < 1.0 && b.height > 10.0);
    }

    #[test]
    fn every_bundled_font_draws_glyphs() {
        let t = TextRenderer::new();
        let mut sizes = Vec::new();
        for font in Font::ALL {
            let mut i = item("Summer 2026");
            i.style.font = font;
            i.style.shadow = false;
            let img = t.image(&i, (1920, 1080)).unwrap();
            assert!(coverage(&img) > 300, "{font:?} drew glyphs");
            sizes.push(coverage(&img));
            assert!(!t.family_name(font).is_empty());
        }
        sizes.dedup();
        assert!(sizes.len() > 3, "fonts look different: {sizes:?}");
    }

    #[test]
    fn position_and_width_come_from_the_item() {
        let t = TextRenderer::new();
        let mut i = item("Rome");
        i.x = 2_500;
        i.y = 8_000;
        i.width = 4_000;
        let img = t.image(&i, (1920, 1080)).unwrap();
        assert_eq!(img.box_width, 768);
        let b = box_rect(&i, &img, (1920, 1080));
        assert!((b.x + b.width / 2.0 - 480.0).abs() <= 1.0);
        assert!((b.y + b.height / 2.0 - 864.0).abs() <= 1.0);
        // Moving reuses the cached image.
        i.x = 7_000;
        assert!(Arc::ptr_eq(&img, &t.image(&i, (1920, 1080)).unwrap()));
        // Long text wraps inside the box: taller box.
        i.text = "A caption long enough to wrap onto several lines in a narrow box".into();
        let tall = t.image(&i, (1920, 1080)).unwrap();
        assert!(tall.box_height > img.box_height * 2);
    }

    #[test]
    fn size_scales_with_the_short_side() {
        let t = TextRenderer::new();
        let i = item("Hello");
        let big = t.image(&i, (1920, 1080)).unwrap();
        let small = t.image(&i, (960, 540)).unwrap();
        let ratio = f64::from(big.box_height) / f64::from(small.box_height);
        assert!((ratio - 2.0).abs() < 0.15, "{ratio}");
        let mut larger = i.clone();
        larger.style.size = 1_200;
        assert!(t.image(&larger, (1920, 1080)).unwrap().box_height > big.box_height * 3 / 2);
    }

    #[test]
    fn bold_italic_colour_and_background_change_the_pixels() {
        let t = TextRenderer::new();
        let mut i = item("Style");
        i.style.shadow = false;
        let plain = t.image(&i, (1280, 720)).unwrap();
        let mut bold = i.clone();
        bold.style.bold = true;
        assert!(coverage(&t.image(&bold, (1280, 720)).unwrap()) > coverage(&plain));
        let mut italic = i.clone();
        italic.style.italic = true;
        assert_ne!(t.image(&italic, (1280, 720)).unwrap().rgba, plain.rgba);
        let mut red = i.clone();
        red.style.color = [255, 0, 0, 255];
        let img = t.image(&red, (1280, 720)).unwrap();
        assert!(
            img.rgba
                .as_chunks::<4>()
                .0
                .iter()
                .any(|p| p[3] == 255 && p[0] == 255 && p[1] == 0)
        );
        let mut boxed = i;
        boxed.style.background = Some([0, 0, 0, 200]);
        let img = t.image(&boxed, (1280, 720)).unwrap();
        let tinted = img
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[3] >= 200)
            .count();
        assert!(tinted > coverage(&plain) * 2, "box behind the text");
    }

    #[test]
    fn entrances_and_exits_move_fade_and_reveal() {
        let t = TextRenderer::new();
        let mut i = item("Hi");
        let img = t.image(&i, (1920, 1080)).unwrap();
        let rest = img.rect(&i, (1920, 1080));
        let at = |i: &TextItem, ms: i64| {
            text_draw(i, &img, Ticks::from_millis(ms), (1920, 1080)).unwrap()
        };
        assert!(text_draw(&i, &img, Ticks::from_seconds(4), (1920, 1080)).is_none());
        // Fade in / out.
        assert!(at(&i, 0).alpha < 0.01);
        assert!((at(&i, 2_000).alpha - 1.0).abs() < 1e-6);
        assert!(at(&i, 3_999).alpha < 0.01);
        // Slide left: arrives from the right, leaves to the left.
        i.enter.kind = TextMotion::SlideLeft;
        i.exit.kind = TextMotion::SlideLeft;
        assert!(at(&i, 100).dst.x > rest.x);
        assert!((at(&i, 2_000).dst.x - rest.x).abs() < 1e-9);
        assert!(at(&i, 3_900).dst.x < rest.x);
        // Wipe right: the visible part grows from the left edge.
        i.enter.kind = TextMotion::WipeRight;
        let c = at(&i, 250).clip.unwrap();
        assert!((c.x - rest.x).abs() < 1e-9 && c.width < rest.width && c.width > 0.0);
        // Zoom: smaller at the start, centred on the box.
        i.enter.kind = TextMotion::Zoom;
        let z = at(&i, 100);
        assert!(z.dst.width < rest.width);
        let (zc, rc) = (z.dst.x + z.dst.width / 2.0, rest.x + rest.width / 2.0);
        assert!((zc - rc).abs() < 1.0);
    }

    #[test]
    fn drawing_blends_over_the_frame_and_respects_the_clip() {
        let img = TextImage {
            width: 20,
            height: 20,
            rgba: [255, 255, 255, 255].repeat(400),
            box_x: 0,
            box_y: 0,
            box_width: 20,
            box_height: 20,
        };
        let px = |f: &Frame, x: usize, y: usize| {
            let i = (y * 100 + x) * 4;
            [f.rgba[i], f.rgba[i + 1], f.rgba[i + 2]]
        };
        let d = TextDraw {
            dst: RectF {
                x: 10.0,
                y: 10.0,
                width: 20.0,
                height: 20.0,
            },
            alpha: 1.0,
            clip: Some(RectF {
                x: 10.0,
                y: 10.0,
                width: 10.0,
                height: 20.0,
            }),
        };
        let mut frame = Frame::solid(100, 50, [0, 0, 255]);
        draw_text(&mut frame, &img, &d);
        assert_eq!(px(&frame, 15, 20), [255, 255, 255]);
        assert_eq!(px(&frame, 25, 20), [0, 0, 255], "clipped away");
        assert_eq!(px(&frame, 50, 20), [0, 0, 255]);
        let half = TextDraw {
            alpha: 0.5,
            clip: None,
            ..d
        };
        let mut frame = Frame::solid(100, 50, [0, 0, 255]);
        draw_text(&mut frame, &img, &half);
        let p = px(&frame, 20, 20);
        assert!(p[0].abs_diff(128) <= 1 && p[2] == 255, "{p:?}");
    }
}
