//! Captions and title text: layout with `parley`, glyphs with `swash`, one
//! bundled font (Inter, OFL) so every machine renders the same pixels.
//!
//! A caption becomes a small straight-alpha RGBA image plus its position in
//! the frame. Both compositors draw that image over the clip's picture, so
//! the text moves with the clip through transitions. Images are cached; a
//! caption is laid out once per frame size, not once per frame.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use clipforge_core::Fit;
use clipforge_core::project::{Caption, CaptionStyle, Quarter};
use parley::fontique::{Blob, Collection, CollectionOptions, SourceCache};
use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, FontWeight, Layout, LayoutContext,
    LineHeight, PositionedLayoutItem, StyleProperty,
};
use swash::FontRef;
use swash::scale::{Render, ScaleContext, Source};
use swash::zeno::{Format, Vector};

use crate::frame::Frame;
use crate::layout::place;

static FONT: &[u8] = include_bytes!("../../../assets/fonts/InterVariable.ttf");

/// Captions kept rendered per compositor.
const CACHE_ENTRIES: usize = 32;

/// A rendered caption: straight-alpha RGBA placed at `(x, y)` in the frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptionImage {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Where the picture is visible in the frame, in pixels. Captions are
/// placed inside it so a letterboxed photo keeps its caption on the photo.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Area {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Area {
    /// The whole `w × h` frame.
    #[must_use]
    pub const fn full(w: u32, h: u32) -> Area {
        Area {
            x: 0,
            y: 0,
            width: w,
            height: h,
        }
    }

    /// Where a `src`-sized picture (before the user's rotation) shows in a
    /// `w × h` frame with `fit`, ignoring any Ken Burns movement.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn of_picture(src: (u32, u32), rotate: Quarter, (w, h): (u32, u32), fit: Fit) -> Area {
        let (sw, sh) = if rotate.swaps_dimensions() {
            (src.1, src.0)
        } else {
            src
        };
        let r = place(sw, sh, w, h, fit);
        let x0 = r.x.clamp(0, i64::from(w));
        let y0 = r.y.clamp(0, i64::from(h));
        let x1 = (r.x + i64::from(r.width)).clamp(0, i64::from(w));
        let y1 = (r.y + i64::from(r.height)).clamp(0, i64::from(h));
        if x1 <= x0 || y1 <= y0 {
            return Area::full(w, h);
        }
        Area {
            x: x0 as u32,
            y: y0 as u32,
            width: (x1 - x0) as u32,
            height: (y1 - y0) as u32,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
    caption: Caption,
    frame: (u32, u32),
    area: Area,
    on_light: bool,
}

/// Look of one style at a given frame size (all lengths in pixels).
struct Look {
    /// Font size and weight of the first line and of the others.
    size: f32,
    weight: f32,
    rest_size: f32,
    rest_weight: f32,
    align: Alignment,
    /// Widest text before it wraps.
    max_width: f32,
    shadow: bool,
    band: bool,
}

struct Engine {
    fonts: FontContext,
    layouts: LayoutContext<[u8; 4]>,
    scaler: ScaleContext,
    family: String,
}

struct Cache {
    images: HashMap<Key, Arc<CaptionImage>>,
    order: Vec<Key>,
}

/// Lays out and rasterises captions. Owned by each compositor.
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
        // Only the bundled font: no system fonts, so output is identical on
        // every machine.
        let mut collection = Collection::new(CollectionOptions {
            shared: false,
            system_fonts: false,
        });
        let families = collection.register_fonts(Blob::new(Arc::new(FONT)), None);
        let family = families
            .first()
            .and_then(|(id, _)| collection.family_name(*id).map(str::to_owned))
            .unwrap_or_else(|| "Inter Variable".to_owned());
        TextRenderer {
            engine: Mutex::new(Engine {
                fonts: FontContext {
                    collection,
                    source_cache: SourceCache::default(),
                },
                layouts: LayoutContext::new(),
                scaler: ScaleContext::new(),
                family,
            }),
            cache: Mutex::new(Cache {
                images: HashMap::new(),
                order: Vec::new(),
            }),
        }
    }

    /// The caption rendered for a `w × h` frame, placed inside `area`;
    /// `None` for empty text. Text size follows the frame, so it stays the
    /// same from clip to clip. `on_light` switches to dark text without
    /// effects (light title cards).
    #[must_use]
    pub fn caption(
        &self,
        caption: &Caption,
        (w, h): (u32, u32),
        area: Area,
        on_light: bool,
    ) -> Option<Arc<CaptionImage>> {
        if caption.text.trim().is_empty() || w == 0 || h == 0 {
            return None;
        }
        let area = clamp_area(area, w, h);
        let key = Key {
            caption: caption.clone(),
            frame: (w, h),
            area,
            on_light,
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
            Arc::new(render_caption(
                &mut engine,
                caption,
                (w, h),
                area,
                on_light,
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
}

/// `area` limited to the frame, never empty.
fn clamp_area(a: Area, w: u32, h: u32) -> Area {
    let x = a.x.min(w.saturating_sub(1));
    let y = a.y.min(h.saturating_sub(1));
    Area {
        x,
        y,
        width: a.width.min(w - x).max(1),
        height: a.height.min(h - y).max(1),
    }
}

fn look(style: CaptionStyle, w: u32, h: u32, area: Area) -> Look {
    // Sizes are designed at 1080 px on the short side and scale with it,
    // so portrait and landscape frames get the same text size.
    #[allow(clippy::cast_precision_loss)]
    let u = w.min(h) as f32 / 1080.0;
    // Wrap inside the picture, but never narrower than 40 % of the frame
    // (a portrait photo in a landscape frame would wrap every word).
    #[allow(clippy::cast_precision_loss)]
    let width = (area.width as f32).max(w as f32 * 0.4);
    match style {
        CaptionStyle::Classic => Look {
            size: 52.0 * u,
            weight: 600.0,
            rest_size: 52.0 * u,
            rest_weight: 600.0,
            align: Alignment::Center,
            max_width: width * 0.8,
            shadow: true,
            band: false,
        },
        CaptionStyle::Banner => Look {
            size: 44.0 * u,
            weight: 500.0,
            rest_size: 44.0 * u,
            rest_weight: 500.0,
            align: Alignment::Center,
            max_width: width * 0.86,
            shadow: false,
            band: true,
        },
        CaptionStyle::Headline => Look {
            size: 104.0 * u,
            weight: 700.0,
            rest_size: 46.0 * u,
            rest_weight: 400.0,
            align: Alignment::Center,
            max_width: width * 0.84,
            shadow: true,
            band: false,
        },
        CaptionStyle::Corner => Look {
            size: 34.0 * u,
            weight: 500.0,
            rest_size: 34.0 * u,
            rest_weight: 500.0,
            align: Alignment::Start,
            max_width: width * 0.5,
            shadow: true,
            band: false,
        },
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss
)]
fn render_caption(
    engine: &mut Engine,
    caption: &Caption,
    (w, h): (u32, u32),
    area: Area,
    on_light: bool,
) -> Option<CaptionImage> {
    let look = look(caption.style, w, h, area);
    let band_w = area.width;
    let text = caption.text.trim_end();
    let first_end = text.find('\n').unwrap_or(text.len());

    let family = engine.family.clone();
    let mut builder = engine
        .layouts
        .ranged_builder(&mut engine.fonts, text, 1.0, true);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named(&family)));
    builder.push_default(StyleProperty::FontSize(look.rest_size));
    builder.push_default(StyleProperty::FontWeight(FontWeight::new(look.rest_weight)));
    builder.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(
        1.25,
    )));
    builder.push(StyleProperty::FontSize(look.size), 0..first_end);
    builder.push(
        StyleProperty::FontWeight(FontWeight::new(look.weight)),
        0..first_end,
    );
    let mut layout: Layout<[u8; 4]> = builder.build(text);
    layout.break_all_lines(Some(look.max_width));
    // Alignment happens within the wrap width; re-break at the text's own
    // width (same line breaks) so centred lines centre inside the text box.
    let natural = layout.width().ceil();
    layout.break_all_lines(Some(natural));
    layout.align(look.align, AlignmentOptions::default());

    let text_w = layout.width().ceil().max(1.0) as u32;
    let text_h = layout.height().ceil().max(1.0) as u32;
    let u = w.min(h) as f32 / 1080.0;
    // Room around the text for the shadow, or the band's padding.
    let blur = (6.0 * u).round().max(1.0) as u32;
    let shadow_dy = (3.0 * u).round() as u32;
    let pad_x = if look.band {
        (32.0 * u).round() as u32
    } else {
        blur * 2
    };
    let pad_y = if look.band {
        (20.0 * u).round() as u32
    } else {
        blur * 2 + shadow_dy
    };
    let (cw, ch) = if look.band {
        (band_w, text_h + 2 * pad_y)
    } else {
        (text_w + 2 * pad_x, text_h + 2 * pad_y)
    };
    // Where the layout's origin sits inside the canvas.
    let origin_x = if look.band {
        ((band_w - text_w.min(band_w)) / 2) as f32
    } else {
        pad_x as f32
    };
    let origin_y = pad_y as f32;

    let mask = rasterise(engine, &layout, cw, ch, origin_x, origin_y);

    let colour: [u8; 3] = if on_light {
        [26, 26, 30]
    } else {
        [255, 255, 255]
    };
    let mut rgba = vec![0u8; (cw * ch * 4) as usize];
    if look.band && !on_light {
        for px in rgba.as_chunks_mut::<4>().0 {
            px.copy_from_slice(&[0, 0, 0, 150]);
        }
    }
    if look.shadow && !on_light {
        let blurred = box_blur(&mask, cw, ch, blur);
        for y in 0..ch {
            for x in 0..cw {
                let sy = y.checked_sub(shadow_dy);
                let a = sy.map_or(0, |sy| blurred[(sy * cw + x) as usize]);
                let i = ((y * cw + x) * 4) as usize;
                // Soft black at up to 70 % opacity.
                rgba[i + 3] = (u32::from(a) * 180 / 255) as u8;
            }
        }
    }
    // Text over whatever is there (straight alpha "over").
    for (i, &a) in mask.iter().enumerate() {
        if a == 0 {
            continue;
        }
        let px = &mut rgba[i * 4..i * 4 + 4];
        let sa = f32::from(a) / 255.0;
        let da = f32::from(px[3]) / 255.0;
        let out_a = sa + da * (1.0 - sa);
        for c in 0..3 {
            let s = f32::from(colour[c]);
            let d = f32::from(px[c]);
            px[c] = ((s * sa + d * da * (1.0 - sa)) / out_a).round() as u8;
        }
        px[3] = (out_a * 255.0).round() as u8;
    }

    // Placement inside the visible picture; margins follow the frame.
    let (ax, ay) = (area.x as i32, area.y as i32);
    let (aw, ah) = (area.width as i32, area.height as i32);
    let (cw_i, ch_i) = (cw as i32, ch as i32);
    let short = w.min(h) as f32;
    let margin = (short * 0.05) as i32;
    let bottom = ay + ah - (short * 0.07) as i32;
    let (x, y) = match caption.style {
        CaptionStyle::Classic => (ax + (aw - cw_i) / 2, bottom - ch_i),
        CaptionStyle::Banner => (ax, ay + ah - ch_i - (short * 0.06) as i32),
        CaptionStyle::Headline => (ax + (aw - cw_i) / 2, ay + (ah - ch_i) / 2),
        CaptionStyle::Corner => (
            ax + margin - pad_x as i32,
            ay + ah - margin - ch_i + pad_y as i32,
        ),
    };
    Some(CaptionImage {
        x,
        y,
        width: cw,
        height: ch,
        rgba,
    })
}

/// Coverage mask (0–255) of the layout's glyphs on a `cw × ch` canvas.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]
fn rasterise(
    engine: &mut Engine,
    layout: &Layout<[u8; 4]>,
    cw: u32,
    ch: u32,
    origin_x: f32,
    origin_y: f32,
) -> Vec<u8> {
    let mut mask = vec![0u8; (cw * ch) as usize];
    for line in layout.lines() {
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
            let mut scaler = engine
                .scaler
                .builder(font_ref)
                .size(run.font_size())
                .hint(false)
                .normalized_coords(run.normalized_coords())
                .build();
            for glyph in glyph_run.positioned_glyphs() {
                let gx = origin_x + glyph.x;
                let gy = origin_y + glyph.y;
                let Some(image) = Render::new(&[Source::Outline])
                    .format(Format::Alpha)
                    .offset(Vector::new(gx.fract(), gy.fract()))
                    .render(&mut scaler, glyph.id as u16)
                else {
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
    mask
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

/// Draws a caption over a frame (straight-alpha "over"), clipped to the
/// frame.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]
pub fn draw_caption(frame: &mut Frame, image: &CaptionImage) {
    let (fw, fh) = (frame.width as i32, frame.height as i32);
    for row in 0..image.height as i32 {
        let y = image.y + row;
        if y < 0 || y >= fh {
            continue;
        }
        for col in 0..image.width as i32 {
            let x = image.x + col;
            if x < 0 || x >= fw {
                continue;
            }
            let si = ((row * image.width as i32 + col) * 4) as usize;
            let a = u32::from(image.rgba[si + 3]);
            if a == 0 {
                continue;
            }
            let di = ((y * fw + x) * 4) as usize;
            for c in 0..3 {
                let s = u32::from(image.rgba[si + c]);
                let d = u32::from(frame.rgba[di + c]);
                frame.rgba[di + c] = ((s * a + d * (255 - a) + 127) / 255) as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coverage(img: &CaptionImage) -> usize {
        img.rgba
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[3] > 128)
            .count()
    }

    #[test]
    fn empty_text_renders_nothing() {
        let t = TextRenderer::new();
        assert!(
            t.caption(
                &Caption::new("  \n", CaptionStyle::Classic),
                (1920, 1080),
                Area::full(1920, 1080),
                false
            )
            .is_none()
        );
    }

    #[test]
    fn captions_sit_where_their_style_says() {
        let t = TextRenderer::new();
        let (w, h) = (1920u32, 1080u32);
        let classic = t
            .caption(
                &Caption::new("Rome, 2024", CaptionStyle::Classic),
                (w, h),
                Area::full(w, h),
                false,
            )
            .unwrap();
        assert!(coverage(&classic) > 500, "glyphs were drawn");
        let centre_x = classic.x + classic.width as i32 / 2;
        assert!((centre_x - 960).abs() <= 2, "centred: {centre_x}");
        assert!(classic.y > 800, "near the bottom: {}", classic.y);

        let banner = t
            .caption(
                &Caption::new("Rome", CaptionStyle::Banner),
                (w, h),
                Area::full(w, h),
                false,
            )
            .unwrap();
        assert_eq!((banner.x, banner.width), (0, w), "full-width band");
        assert!(banner.rgba[3] > 100, "band is tinted");

        let headline = t
            .caption(
                &Caption::new("Summer\nItaly 2024", CaptionStyle::Headline),
                (w, h),
                Area::full(w, h),
                false,
            )
            .unwrap();
        let mid_y = headline.y + headline.height as i32 / 2;
        assert!((mid_y - 540).abs() <= 2, "vertically centred: {mid_y}");
        assert!(headline.height > classic.height, "two lines, larger");

        let corner = t
            .caption(
                &Caption::new("Rome", CaptionStyle::Corner),
                (w, h),
                Area::full(w, h),
                false,
            )
            .unwrap();
        assert!(
            corner.x < 100 && corner.y > 900,
            "{} {}",
            corner.x,
            corner.y
        );
    }

    #[test]
    fn captions_stay_on_a_letterboxed_picture() {
        let t = TextRenderer::new();
        // A portrait photo in the middle third of a landscape frame.
        let area = Area {
            x: 640,
            y: 0,
            width: 640,
            height: 1080,
        };
        let corner = t
            .caption(
                &Caption::new("Rome", CaptionStyle::Corner),
                (1920, 1080),
                area,
                false,
            )
            .unwrap();
        assert!(corner.x > 600 && corner.x < 720, "{}", corner.x);
        let banner = t
            .caption(
                &Caption::new("Rome", CaptionStyle::Banner),
                (1920, 1080),
                area,
                false,
            )
            .unwrap();
        assert_eq!((banner.x, banner.width), (640, 640));
        let classic = t
            .caption(
                &Caption::new("Rome", CaptionStyle::Classic),
                (1920, 1080),
                area,
                false,
            )
            .unwrap();
        assert!((classic.x + classic.width as i32 / 2 - 960).abs() <= 2);
    }

    #[test]
    fn long_text_wraps_inside_the_frame() {
        let t = TextRenderer::new();
        let long =
            "A very long caption that keeps going and going well past the width of the frame";
        let img = t
            .caption(
                &Caption::new(long, CaptionStyle::Classic),
                (1280, 720),
                Area::full(1280, 720),
                false,
            )
            .unwrap();
        assert!(img.x >= 0 && img.x as u32 + img.width <= 1280);
        let one_line = t
            .caption(
                &Caption::new("A", CaptionStyle::Classic),
                (1280, 720),
                Area::full(1280, 720),
                false,
            )
            .unwrap();
        assert!(
            img.height > one_line.height * 3 / 2,
            "wrapped onto more lines"
        );
    }

    #[test]
    fn text_scales_with_the_short_side_and_is_cached() {
        let t = TextRenderer::new();
        let c = Caption::new("Hello", CaptionStyle::Classic);
        let big = t
            .caption(&c, (1920, 1080), Area::full(1920, 1080), false)
            .unwrap();
        let small = t
            .caption(&c, (960, 540), Area::full(960, 540), false)
            .unwrap();
        let ratio = f64::from(big.height) / f64::from(small.height);
        assert!((ratio - 2.0).abs() < 0.15, "{ratio}");
        let portrait = t
            .caption(&c, (1080, 1920), Area::full(1080, 1920), false)
            .unwrap();
        assert!(portrait.height.abs_diff(big.height) <= 2);
        let again = t
            .caption(&c, (1920, 1080), Area::full(1920, 1080), false)
            .unwrap();
        assert!(Arc::ptr_eq(&big, &again));
    }

    #[test]
    fn light_backgrounds_get_dark_text() {
        let t = TextRenderer::new();
        let img = t
            .caption(
                &Caption::new("The end", CaptionStyle::Headline),
                (960, 540),
                Area::full(960, 540),
                true,
            )
            .unwrap();
        let opaque: Vec<&[u8; 4]> = img
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[3] > 250)
            .collect();
        assert!(!opaque.is_empty());
        assert!(opaque.iter().all(|p| p[0] < 60), "dark glyphs");
    }

    #[test]
    fn drawing_blends_over_the_frame() {
        let mut frame = Frame::solid(100, 50, [0, 0, 255]);
        let img = CaptionImage {
            x: 90,
            y: 40,
            width: 20,
            height: 20,
            rgba: [255, 255, 255, 255].repeat(400),
        };
        draw_caption(&mut frame, &img);
        fn px(f: &Frame, x: usize, y: usize) -> [u8; 3] {
            let i = (y * 100 + x) * 4;
            [f.rgba[i], f.rgba[i + 1], f.rgba[i + 2]]
        }
        assert_eq!(px(&frame, 95, 45), [255, 255, 255]);
        assert_eq!(px(&frame, 10, 10), [0, 0, 255]);
        let half = CaptionImage {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            rgba: vec![255, 0, 0, 128],
        };
        draw_caption(&mut frame, &half);
        assert_eq!(px(&frame, 0, 0), [128, 0, 127]);
    }
}
