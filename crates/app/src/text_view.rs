//! Pure view logic for the text track (docs/ux/decisions/m5b-text-track.md):
//! lane layout, preview geometry and the drag gestures. No Slint here.

use clipforge_core::text::{FRAME_UNITS, TextItem, TextStyle};
use clipforge_core::{Clip, Ticks};

use crate::editor_view::{ClipBox, x_at_time};

/// Rows the text lane shows at most; further overlaps share the last row.
pub(crate) const MAX_TEXT_ROWS: usize = 3;
/// Snap distance, in frame units.
pub(crate) const SNAP_UNITS: i32 = 120;
/// Safe margin from the frame edges (5 %), in frame units.
pub(crate) const SAFE_MARGIN: i32 = 500;
/// Length of a new text.
pub(crate) const NEW_TEXT_DURATION: Ticks = Ticks::from_seconds(4);

/// Row per text so overlapping items do not hide each other: the first row
/// whose last item ends before this one starts (in timeline order).
#[must_use]
pub(crate) fn text_rows(texts: &[TextItem]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..texts.len()).collect();
    order.sort_by_key(|&i| (texts[i].start, i));
    let mut row_end: Vec<Ticks> = Vec::new();
    let mut rows = vec![0; texts.len()];
    for i in order {
        let t = &texts[i];
        let row = row_end
            .iter()
            .position(|end| *end <= t.start)
            .unwrap_or(row_end.len())
            .min(MAX_TEXT_ROWS - 1);
        if row == row_end.len() {
            row_end.push(t.end());
        } else {
            row_end[row] = row_end[row].max(t.end());
        }
        rows[i] = row;
    }
    rows
}

/// One text on the lane.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextBlock {
    pub index: usize,
    pub x: f32,
    pub width: f32,
    pub row: usize,
    /// First line of the text.
    pub title: String,
}

/// Blocks on the strip's x scale and the number of rows in use (≥ 1).
#[must_use]
pub(crate) fn text_blocks(
    texts: &[TextItem],
    clips: &[Clip],
    boxes: &[ClipBox],
) -> (Vec<TextBlock>, usize) {
    let rows = text_rows(texts);
    let blocks = texts
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let x = x_at_time(clips, boxes, t.start);
            let end = x_at_time(clips, boxes, t.end());
            TextBlock {
                index: i,
                x,
                width: (end - x).max(24.0),
                row: rows[i],
                title: t.text.lines().next().unwrap_or_default().to_owned(),
            }
        })
        .collect();
    let used = rows.iter().max().map_or(1, |r| r + 1);
    (blocks, used)
}

/// What a pointer drag on the preview does.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum TextGesture {
    Move,
    /// A side handle: change the wrap width around the centre.
    Width {
        left: bool,
    },
    /// A corner handle: scale font size and width.
    Scale,
}

impl TextGesture {
    /// From the Slint handle code: 0 move, 1 left side, 2 right side,
    /// 3 corner.
    #[must_use]
    pub(crate) fn from_code(code: i32) -> TextGesture {
        match code {
            1 => TextGesture::Width { left: true },
            2 => TextGesture::Width { left: false },
            3 => TextGesture::Scale,
            _ => TextGesture::Move,
        }
    }
}

/// Snap guides shown while moving: where the vertical and horizontal
/// guide lines are (frame units), if any.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Guides {
    pub x: Option<i32>,
    pub y: Option<i32>,
}

/// Snapping for a moved text: its centre to the frame centre or another
/// text's centre, its box edges to the 5 % safe margin. `half` is half the
/// text box (frame units), `others` the centres of the other visible texts.
/// Returns the correction to apply and the guides to show.
#[must_use]
pub(crate) fn snap(
    moved: &TextItem,
    half: (i32, i32),
    others: &[(i32, i32)],
) -> (i32, i32, Guides) {
    let pick = |pos: i32, half: i32, others: &mut dyn Iterator<Item = i32>| -> (i32, Option<i32>) {
        // (centre to snap to, where the guide line goes)
        let mut candidates = vec![
            (FRAME_UNITS / 2, FRAME_UNITS / 2),
            (SAFE_MARGIN + half, SAFE_MARGIN),
            (FRAME_UNITS - SAFE_MARGIN - half, FRAME_UNITS - SAFE_MARGIN),
        ];
        candidates.extend(others.map(|o| (o, o)));
        candidates
            .into_iter()
            .map(|(c, g)| ((c - pos).abs(), c, g))
            .filter(|(d, _, _)| *d <= SNAP_UNITS)
            .min_by_key(|(d, _, _)| *d)
            .map_or((0, None), |(_, c, g)| (c - pos, Some(g)))
    };
    let (dx, gx) = pick(moved.x, half.0, &mut others.iter().map(|o| o.0));
    let (dy, gy) = pick(moved.y, half.1, &mut others.iter().map(|o| o.1));
    (dx, dy, Guides { x: gx, y: gy })
}

/// Keyboard navigation on the text lane: the next / previous text in time
/// order from `focus` (the first / last one without a focus).
#[must_use]
pub(crate) fn text_nav(texts: &[TextItem], focus: Option<usize>, forward: bool) -> Option<usize> {
    let mut order: Vec<usize> = (0..texts.len()).collect();
    order.sort_by_key(|&i| (texts[i].start, i));
    let pos = focus.and_then(|f| order.iter().position(|&i| i == f));
    match (pos, forward) {
        (None, true) => order.first().copied(),
        (None, false) => order.last().copied(),
        (Some(p), true) => order.get(p + 1).or(order.get(p)).copied(),
        (Some(p), false) => order.get(p.saturating_sub(1)).copied(),
    }
}

/// Applies a preview drag from `start` to `now` (pointer positions as
/// fractions of the picture, 0..1) to `orig`. `aspect` is width / height
/// of the frame (for the scale gesture's distances).
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub(crate) fn apply_gesture(
    orig: &TextItem,
    gesture: TextGesture,
    start: (f32, f32),
    now: (f32, f32),
    aspect: f32,
) -> (TextItem, Guides) {
    let units = |f: f32| (f * FRAME_UNITS as f32).round() as i32;
    let mut t = orig.clone();
    let guides = Guides::default();
    match gesture {
        TextGesture::Move => {
            // Snapping is applied afterwards, to the whole selection (`snap`).
            t.x = orig.x + units(now.0 - start.0);
            t.y = orig.y + units(now.1 - start.1);
        }
        TextGesture::Width { left } => {
            let dx = units(now.0 - start.0);
            let grow = if left { -dx } else { dx };
            t.width = (orig.width + 2 * grow).clamp(TextItem::MIN_WIDTH, FRAME_UNITS * 2);
        }
        TextGesture::Scale => {
            let centre = (
                orig.x as f32 / FRAME_UNITS as f32,
                orig.y as f32 / FRAME_UNITS as f32,
            );
            let dist = |p: (f32, f32)| {
                let dx = (p.0 - centre.0) * aspect;
                let dy = p.1 - centre.1;
                (dx * dx + dy * dy).sqrt()
            };
            let d0 = dist(start);
            if d0 > 1e-4 {
                let s = dist(now) / d0;
                let size = (f32::from(orig.style.size) * s).round();
                let size = size.clamp(
                    f32::from(TextStyle::MIN_SIZE),
                    f32::from(TextStyle::MAX_SIZE),
                );
                // Keep width and size in proportion, limited like the size.
                let s = size / f32::from(orig.style.size);
                t.style.size = size as u16;
                t.width = ((orig.width as f32 * s).round() as i32)
                    .clamp(TextItem::MIN_WIDTH, FRAME_UNITS * 2);
            }
        }
    }
    (t, guides)
}

/// Nudges a text by `(dx, dy)` in frame units (arrow keys).
#[must_use]
pub(crate) fn nudge(orig: &TextItem, dx: i32, dy: i32) -> TextItem {
    TextItem {
        x: orig.x + dx,
        y: orig.y + dy,
        ..orig.clone()
    }
}

/// What a drag on the text lane does.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum LaneGesture {
    Move,
    Start,
    End,
}

impl LaneGesture {
    /// From the Slint code: 0 body, 1 left edge, 2 right edge.
    #[must_use]
    pub(crate) fn from_code(code: i32) -> LaneGesture {
        match code {
            1 => LaneGesture::Start,
            2 => LaneGesture::End,
            _ => LaneGesture::Move,
        }
    }
}

/// Applies a time delta from a lane drag, keeping the start ≥ 0 and the
/// length ≥ the minimum.
#[must_use]
pub(crate) fn lane_drag(orig: &TextItem, gesture: LaneGesture, dt: Ticks) -> TextItem {
    let mut t = orig.clone();
    match gesture {
        LaneGesture::Move => t.start = (orig.start + dt).max(Ticks::ZERO),
        LaneGesture::Start => {
            let latest = orig.end() - TextItem::MIN_DURATION;
            t.start = (orig.start + dt).clamp(Ticks::ZERO, latest);
            t.duration = orig.end() - t.start;
        }
        LaneGesture::End => {
            t.duration = (orig.duration + dt).max(TextItem::MIN_DURATION);
        }
    }
    t
}

/// A new text at the playhead (or at 0 past the end of the show).
#[must_use]
pub(crate) fn new_text(text: &str, playhead: Ticks, show_end: Ticks) -> TextItem {
    let start = if playhead < show_end {
        playhead
    } else {
        Ticks::ZERO
    };
    TextItem::new(text, start, NEW_TEXT_DURATION)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipforge_core::MediaId;

    fn text(start: i64, secs: i64) -> TextItem {
        TextItem::new(
            "A\nB",
            Ticks::from_seconds(start),
            Ticks::from_seconds(secs),
        )
    }

    #[test]
    fn text_rows_stack_overlapping_items() {
        let texts = vec![text(0, 4), text(2, 4), text(4, 2), text(3, 5), text(3, 5)];
        // 0: row 0; 1 overlaps → row 1; 2 starts when 0 ends → row 0;
        // 3 overlaps both → row 2; 4 would need row 3 → shares row 2.
        assert_eq!(text_rows(&texts), vec![0, 1, 0, 2, 2]);
        assert!(text_rows(&[]).is_empty());
    }

    #[test]
    fn text_blocks_follow_the_strip_scale() {
        let clips: Vec<Clip> = (0..3)
            .map(|_| Clip::photo(MediaId::new(), Ticks::from_seconds(4)))
            .collect();
        let boxes = crate::editor_view::layout(&clips, 20.0);
        let (blocks, rows) = text_blocks(&[text(2, 4), text(3, 1)], &clips, &boxes);
        assert_eq!(
            (blocks[0].x, blocks[0].width, blocks[0].row),
            (40.0, 80.0, 0)
        );
        assert_eq!(blocks[0].title, "A");
        assert_eq!(
            (blocks[1].x, blocks[1].width, blocks[1].row),
            (60.0, 24.0, 1),
            "min width"
        );
        assert_eq!(rows, 2);
        assert_eq!(text_blocks(&[], &clips, &boxes).1, 1);
    }

    #[test]
    fn drag_moves_by_the_pointer_delta_and_snaps_to_centre() {
        let mut t = text(0, 4);
        t.x = 2_000;
        t.y = 8_000;
        let (m, _) = apply_gesture(&t, TextGesture::Move, (0.2, 0.8), (0.3, 0.7), 16.0 / 9.0);
        assert_eq!((m.x, m.y), (3_000, 7_000));
        assert_eq!(m.style, t.style, "moving changes nothing else");
        assert_eq!(snap(&m, (1_000, 300), &[]), (0, 0, Guides::default()));
        let (m, _) = apply_gesture(
            &t,
            TextGesture::Move,
            (0.2, 0.8),
            (0.495, 0.505),
            16.0 / 9.0,
        );
        let (dx, dy, g) = snap(&m, (1_000, 300), &[]);
        assert_eq!((m.x + dx, m.y + dy), (5_000, 5_000));
        assert_eq!(
            g,
            Guides {
                x: Some(5_000),
                y: Some(5_000)
            }
        );
    }

    #[test]
    fn snapping_reaches_other_texts_and_the_safe_margin() {
        let mut t = text(0, 4);
        // Left edge near the 5 % margin: box half width 1 000 → centre 1 500.
        t.x = 1_560;
        t.y = 3_000;
        let (dx, _, g) = snap(&t, (1_000, 300), &[]);
        assert_eq!((t.x + dx, g.x), (1_500, Some(SAFE_MARGIN)));
        // Centre lines up with another text's centre.
        t.x = 7_050;
        t.y = 2_940;
        let (dx, dy, g) = snap(&t, (1_000, 300), &[(7_000, 3_000)]);
        assert_eq!((t.x + dx, t.y + dy), (7_000, 3_000));
        assert_eq!(
            g,
            Guides {
                x: Some(7_000),
                y: Some(3_000)
            }
        );
        // Bottom edge at the safe margin (half height 300 → centre 9 200).
        t.y = 9_150;
        let (_, dy, g) = snap(&t, (1_000, 300), &[]);
        assert_eq!((t.y + dy, g.y), (9_200, Some(FRAME_UNITS - SAFE_MARGIN)));
    }

    #[test]
    fn keyboard_moves_between_texts_in_time_order() {
        let texts = vec![text(5, 1), text(0, 1), text(2, 1)];
        assert_eq!(text_nav(&texts, None, true), Some(1));
        assert_eq!(text_nav(&texts, None, false), Some(0));
        assert_eq!(text_nav(&texts, Some(1), true), Some(2));
        assert_eq!(text_nav(&texts, Some(2), true), Some(0));
        assert_eq!(text_nav(&texts, Some(0), true), Some(0), "stays at the end");
        assert_eq!(
            text_nav(&texts, Some(1), false),
            Some(1),
            "stays at the start"
        );
        assert_eq!(text_nav(&[], None, true), None);
    }

    #[test]
    fn side_handle_changes_width_around_the_centre() {
        let t = text(0, 4); // width 8000, centred
        let (r, _) = apply_gesture(
            &t,
            TextGesture::Width { left: false },
            (0.9, 0.5),
            (0.8, 0.5),
            1.0,
        );
        assert_eq!((r.width, r.x), (6_000, t.x));
        let (l, _) = apply_gesture(
            &t,
            TextGesture::Width { left: true },
            (0.1, 0.5),
            (0.05, 0.5),
            1.0,
        );
        assert_eq!(l.width, 9_000);
        let (tiny, _) = apply_gesture(
            &t,
            TextGesture::Width { left: false },
            (0.9, 0.5),
            (0.0, 0.5),
            1.0,
        );
        assert_eq!(tiny.width, TextItem::MIN_WIDTH);
    }

    #[test]
    fn corner_handle_scales_size_and_width() {
        let t = text(0, 4); // size 600, width 8000, centre (0.5, 0.5)
        let (s, _) = apply_gesture(&t, TextGesture::Scale, (0.9, 0.5), (1.3, 0.5), 1.0);
        assert_eq!((s.style.size, s.width), (1_200, 16_000));
        let (small, _) = apply_gesture(
            &t,
            TextGesture::Scale,
            (0.9, 0.5),
            (0.5 + 0.4 / 100.0, 0.5),
            1.0,
        );
        assert_eq!(small.style.size, TextStyle::MIN_SIZE, "clamped");
        assert_eq!(small.width, TextItem::MIN_WIDTH.max(8_000 * 150 / 600));
    }

    #[test]
    fn lane_drag_moves_and_trims_with_a_minimum_length() {
        let t = text(2, 4);
        let s = Ticks::SECOND;
        let m = lane_drag(&t, LaneGesture::Move, s);
        assert_eq!((m.start, m.duration), (Ticks::from_seconds(3), t.duration));
        let m = lane_drag(&t, LaneGesture::Move, Ticks::from_seconds(-9));
        assert_eq!(m.start, Ticks::ZERO);
        let a = lane_drag(&t, LaneGesture::Start, s);
        assert_eq!((a.start, a.end()), (Ticks::from_seconds(3), t.end()));
        let a = lane_drag(&t, LaneGesture::Start, Ticks::from_seconds(9));
        assert_eq!(a.duration, TextItem::MIN_DURATION);
        let e = lane_drag(&t, LaneGesture::End, Ticks::ZERO - s);
        assert_eq!((e.start, e.duration), (t.start, Ticks::from_seconds(3)));
        let e = lane_drag(&t, LaneGesture::End, Ticks::from_seconds(-9));
        assert_eq!(e.duration, TextItem::MIN_DURATION);
    }

    #[test]
    fn new_text_starts_at_the_playhead() {
        let end = Ticks::from_seconds(10);
        assert_eq!(
            new_text("Hi", Ticks::from_seconds(3), end).start,
            Ticks::from_seconds(3)
        );
        assert_eq!(new_text("Hi", end, end).start, Ticks::ZERO);
        assert_eq!(new_text("Hi", Ticks::ZERO, end).duration, NEW_TEXT_DURATION);
    }
}
