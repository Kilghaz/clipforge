//! Pure view logic for the editor: timeline geometry, selection semantics
//! and media-reference building. No Slint here.

use std::collections::BTreeSet;

use clipforge_core::music::song_spans;
use clipforge_core::project::{Caption, CaptionStyle};
use clipforge_core::timeline::{effective_overlap, placements, total_duration};
use clipforge_core::{Clip, ClipId, MediaRef, Project, RefKind, Ticks};
use clipforge_library::{MediaRecord, ProbeState};
use clipforge_media::MediaKind;

/// Narrowest a clip may be drawn, so very short clips stay clickable.
pub(crate) const MIN_CLIP_WIDTH: f32 = 56.0;
/// Default zoom.
pub(crate) const DEFAULT_PIXELS_PER_SECOND: f32 = 40.0;

/// Horizontal placement of a clip in the timeline strip.
#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) struct ClipBox {
    pub x: f32,
    pub width: f32,
    /// Pixels of overlap with the previous clip (transition), for the badge.
    pub overlap: f32,
}

/// Lays clips out left to right. Widths follow duration (with a minimum);
/// transitions pull a clip left by their overlap.
#[must_use]
pub(crate) fn layout(clips: &[Clip], pixels_per_second: f32) -> Vec<ClipBox> {
    let mut out = Vec::with_capacity(clips.len());
    let mut cursor = 0.0f32;
    for (i, clip) in clips.iter().enumerate() {
        let natural = secs(clip.duration()) * pixels_per_second;
        let width = natural.max(MIN_CLIP_WIDTH);
        let overlap_px = if i == 0 {
            0.0
        } else {
            let prev: &ClipBox = &out[i - 1];
            (secs(effective_overlap(clips, i)) * pixels_per_second)
                .min(prev.width / 2.0)
                .min(width / 2.0)
        };
        let x = cursor - overlap_px;
        out.push(ClipBox {
            x,
            width,
            overlap: overlap_px,
        });
        cursor = x + width;
    }
    out
}

/// Total width of the strip.
#[must_use]
pub(crate) fn strip_width(boxes: &[ClipBox]) -> f32 {
    boxes.last().map_or(0.0, |b| b.x + b.width)
}

/// Insertion index for a drop at pixel `x`: before the clip whose centre is
/// right of `x`, else at the end.
#[must_use]
pub(crate) fn drop_index(boxes: &[ClipBox], x: f32) -> usize {
    boxes
        .iter()
        .position(|b| x < b.x + b.width / 2.0)
        .unwrap_or(boxes.len())
}

/// Index of the clip under pixel `x`, if any.
#[must_use]
pub(crate) fn clip_at_x(boxes: &[ClipBox], x: f32) -> Option<usize> {
    boxes.iter().rposition(|b| x >= b.x && x < b.x + b.width)
}

/// Timeline time for a pixel position along the strip (time is
/// proportional to pixels only between clip boundaries, so map within the
/// clip under the cursor).
#[must_use]
pub(crate) fn time_at_x(clips: &[Clip], boxes: &[ClipBox], x: f32) -> Ticks {
    let places = placements(clips);
    if boxes.is_empty() {
        return Ticks::ZERO;
    }
    let Some(i) = clip_at_x(boxes, x.max(0.0)) else {
        return if x < 0.0 {
            Ticks::ZERO
        } else {
            places.last().map_or(Ticks::ZERO, |p| p.end)
        };
    };
    let frac = ((x - boxes[i].x) / boxes[i].width).clamp(0.0, 1.0);
    let p = places[i];
    p.start + Ticks::from_seconds_f64(secs(p.duration()) as f64 * f64::from(frac))
}

/// Pixel position for a timeline time (inverse of [`time_at_x`]).
#[must_use]
pub(crate) fn x_at_time(clips: &[Clip], boxes: &[ClipBox], t: Ticks) -> f32 {
    let places = placements(clips);
    if places.is_empty() {
        return 0.0;
    }
    // The clip that starts latest at or before t owns the pixel mapping.
    let Some(i) = places.iter().rposition(|p| p.start <= t) else {
        return 0.0;
    };
    let p = places[i];
    let d = secs(p.duration());
    let frac = if d > 0.0 {
        (secs(t - p.start) / d).clamp(0.0, 1.0)
    } else {
        0.0
    };
    boxes[i].x + frac * boxes[i].width
}

#[allow(clippy::cast_possible_truncation)]
fn secs(t: Ticks) -> f32 {
    t.as_seconds_f64() as f32
}

/// Direction of a keyboard reorder (Alt/Option + ←/→).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum Nudge {
    Earlier,
    Later,
}

/// The `to` for `Command::move_clips` that moves the selected clips one
/// position earlier or later. A scattered selection is gathered into a
/// block next to its first (earlier) or last (later) clip. `None` when the
/// block already sits at that edge.
#[must_use]
pub(crate) fn nudge_target(len: usize, indices: &[usize], dir: Nudge) -> Option<usize> {
    let valid = || indices.iter().copied().filter(|i| *i < len);
    let first = valid().min()?;
    let last = valid().max()?;
    match dir {
        Nudge::Earlier => first.checked_sub(1),
        Nudge::Later => (last + 1 < len).then_some(last + 2),
    }
}

/// Next (`forward`) or previous clip for ↑/↓. Without a focused clip, the
/// first or last clip. A stale index is clamped first.
#[must_use]
pub(crate) fn clip_nav(focus: Option<usize>, len: usize, forward: bool) -> Option<usize> {
    let last = len.checked_sub(1)?;
    Some(match focus {
        None if forward => 0,
        None => last,
        Some(f) if forward => (f.min(last) + 1).min(last),
        Some(f) => f.min(last).saturating_sub(1),
    })
}

/// Selection with an anchor for shift-clicks and a keyboard focus.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Selection {
    pub ids: BTreeSet<ClipId>,
    pub anchor: Option<ClipId>,
    /// Keyboard focus; clicks move it too.
    pub focus: Option<ClipId>,
}

impl Selection {
    /// Applies a click on `index` with the given modifiers.
    pub(crate) fn click(&mut self, clips: &[Clip], index: usize, shift: bool, toggle: bool) {
        let Some(clicked) = clips.get(index).map(|c| c.id) else {
            return;
        };
        self.focus = Some(clicked);
        if shift {
            let anchor_idx = self
                .anchor
                .and_then(|a| clips.iter().position(|c| c.id == a))
                .unwrap_or(index);
            let (lo, hi) = if anchor_idx <= index {
                (anchor_idx, index)
            } else {
                (index, anchor_idx)
            };
            if !toggle {
                self.ids.clear();
            }
            self.ids.extend(clips[lo..=hi].iter().map(|c| c.id));
        } else if toggle {
            if !self.ids.remove(&clicked) {
                self.ids.insert(clicked);
            }
            self.anchor = Some(clicked);
        } else {
            self.ids.clear();
            self.ids.insert(clicked);
            self.anchor = Some(clicked);
        }
    }

    pub(crate) fn select_all(&mut self, clips: &[Clip]) {
        self.ids = clips.iter().map(|c| c.id).collect();
    }

    pub(crate) fn clear(&mut self) {
        self.ids.clear();
        self.anchor = None;
    }

    /// Moves keyboard focus to `index` without touching the selection.
    pub(crate) fn set_focus(&mut self, clips: &[Clip], index: usize) {
        if let Some(c) = clips.get(index) {
            self.focus = Some(c.id);
        }
    }

    #[must_use]
    pub(crate) fn focus_index(&self, clips: &[Clip]) -> Option<usize> {
        let f = self.focus?;
        clips.iter().position(|c| c.id == f)
    }

    /// Enter: flips the focused clip in or out of the selection.
    pub(crate) fn toggle_focused(&mut self, clips: &[Clip]) {
        if let Some(i) = self.focus_index(clips) {
            self.click(clips, i, false, true);
        }
    }

    /// Drops ids that no longer exist.
    pub(crate) fn retain_existing(&mut self, clips: &[Clip]) {
        let existing: BTreeSet<ClipId> = clips.iter().map(|c| c.id).collect();
        self.ids.retain(|id| existing.contains(id));
        if let Some(a) = self.anchor
            && !existing.contains(&a)
        {
            self.anchor = None;
        }
        if let Some(f) = self.focus
            && !existing.contains(&f)
        {
            self.focus = None;
        }
    }

    /// Selected clip indices in timeline order.
    #[must_use]
    pub(crate) fn indices(&self, clips: &[Clip]) -> Vec<usize> {
        clips
            .iter()
            .enumerate()
            .filter(|(_, c)| self.ids.contains(&c.id))
            .map(|(i, _)| i)
            .collect()
    }

    #[must_use]
    pub(crate) fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }
}

/// Builds a project media reference from a library record. Photos and
/// videos with a known duration can be placed on the timeline.
#[must_use]
pub(crate) fn media_ref_for(record: &MediaRecord) -> Option<MediaRef> {
    if record.probe != ProbeState::Done {
        return None;
    }
    let info = record.info.as_ref()?;
    let timed = info.duration.is_some_and(|d| d > Ticks::ZERO);
    let kind = match record.kind {
        MediaKind::Photo => RefKind::Photo,
        MediaKind::Video if timed => RefKind::Video,
        MediaKind::Audio if timed => RefKind::Audio,
        _ => return None,
    };
    Some(MediaRef {
        id: record.id,
        kind,
        path: record.path.clone(),
        fingerprint_hash: record.fingerprint.hash,
        size: record.fingerprint.size,
        pixel_size: info.display_size(),
        duration: if kind == RefKind::Photo {
            None
        } else {
            info.duration
        },
        captured_at_ms: info.captured_at_ms,
        name: record.file_name(),
    })
}

/// New clips for `refs` using the project's defaults.
#[must_use]
pub(crate) fn clips_for(project: &Project, refs: &[MediaRef]) -> Vec<Clip> {
    refs.iter()
        .map(|r| {
            let mut c = match (r.kind, r.duration) {
                (RefKind::Video, Some(d)) => Clip::video(r.id, d),
                _ => Clip::photo(r.id, project.settings.default_photo_duration),
            };
            c.fit = project.settings.default_fit;
            c.transition_in = project.settings.default_transition;
            if c.is_photo() {
                c.motion = project.settings.default_motion;
            }
            c
        })
        .collect()
}

/// Indices of `targets` that are photos (motion only applies to photos).
#[must_use]
pub(crate) fn photo_targets(project: &Project, targets: &[usize]) -> Vec<usize> {
    targets
        .iter()
        .copied()
        .filter(|&i| project.clips.get(i).is_some_and(Clip::is_photo))
        .collect()
}

/// True if a transition of `duration` would be shortened on any target
/// because a neighbouring clip is too short (overlaps are capped at half of
/// either neighbour; the first clip's opening at half of itself).
#[must_use]
pub(crate) fn transition_capped(project: &Project, targets: &[usize], duration: Ticks) -> bool {
    let clips = &project.clips;
    targets.iter().any(|&i| {
        let Some(clip) = clips.get(i) else {
            return false;
        };
        let mut cap = clip.duration() / 2;
        if i > 0 {
            cap = cap.min(clips[i - 1].duration() / 2);
        }
        duration > cap
    })
}

/// What a timeline clip card shows besides its picture.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct ClipFlags {
    /// Index of the transition kind in `TransitionKind::ALL` (0 = cut).
    pub transition: i32,
    /// The clip is a photo with a Ken Burns movement.
    pub moving: bool,
}

#[must_use]
pub(crate) fn clip_flags(clip: &Clip) -> ClipFlags {
    ClipFlags {
        transition: i32::try_from(clip.transition_in.kind.index()).unwrap_or(0),
        moving: clip.is_photo() && clip.motion != clipforge_core::Motion::None,
    }
}

/// Library items split by where they go: photos and videos become clips,
/// audio becomes songs of the music track.
#[must_use]
pub(crate) fn split_songs(refs: Vec<MediaRef>) -> (Vec<MediaRef>, Vec<MediaRef>) {
    refs.into_iter().partition(|r| r.kind != RefKind::Audio)
}

/// One song on the music lane.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SongBlock {
    /// Index into `Music::songs`.
    pub song: usize,
    pub x: f32,
    pub width: f32,
    pub title: String,
    /// A second or later pass of a looped playlist.
    pub repeat: bool,
}

/// The music lane: song blocks on the strip's time scale, and where the
/// fade-out starts and the music ends (strip x).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct MusicLane {
    pub blocks: Vec<SongBlock>,
    pub fade_x: f32,
    pub end_x: f32,
}

#[must_use]
pub(crate) fn music_lane(project: &Project, clips: &[Clip], boxes: &[ClipBox]) -> MusicLane {
    let show_end = total_duration(clips);
    let spans = song_spans(project, show_end);
    let mut seen = vec![false; project.music.songs.len()];
    let blocks: Vec<SongBlock> = spans
        .iter()
        .map(|s| {
            let x = x_at_time(clips, boxes, s.start);
            let end = x_at_time(clips, boxes, s.end());
            let repeat = seen.get(s.song).copied().unwrap_or(false);
            if let Some(v) = seen.get_mut(s.song) {
                *v = true;
            }
            SongBlock {
                song: s.song,
                x,
                width: (end - x).max(1.0),
                title: project
                    .media_ref(s.media)
                    .map(|m| m.name.clone())
                    .unwrap_or_default(),
                repeat,
            }
        })
        .collect();
    let end = spans.last().map_or(Ticks::ZERO, |s| s.end());
    let fade = project.music.fade_out.max(Ticks::ZERO).min(end);
    MusicLane {
        blocks,
        fade_x: x_at_time(clips, boxes, end - fade),
        end_x: x_at_time(clips, boxes, end),
    }
}

/// What the caption field shows for the targets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CaptionView {
    /// The shared text ("" when none of the targets has a caption).
    pub text: String,
    /// The targets' captions differ; the field shows a placeholder.
    pub mixed: bool,
    /// Style of the first caption among the targets, else `fallback`.
    pub style: CaptionStyle,
    /// Some target has a caption (enables "Remove captions").
    pub any: bool,
}

#[must_use]
pub(crate) fn caption_view(
    project: &Project,
    targets: &[usize],
    fallback: CaptionStyle,
) -> CaptionView {
    let texts: Vec<&str> = targets
        .iter()
        .map(|&i| {
            project.clips[i]
                .caption
                .as_ref()
                .map_or("", |c| c.text.as_str())
        })
        .collect();
    let first = texts.first().copied().unwrap_or("");
    let mixed = texts.iter().any(|t| *t != first);
    let captions = || {
        targets
            .iter()
            .filter_map(|&i| project.clips[i].caption.as_ref())
    };
    CaptionView {
        text: if mixed {
            String::new()
        } else {
            first.to_owned()
        },
        mixed,
        style: captions().next().map_or(fallback, |c| c.style),
        any: captions().next().is_some(),
    }
}

/// Clips the caption controls act on: the selection, or with nothing
/// selected every clip except title cards (their text is the title, so a
/// bulk caption must not overwrite it).
#[must_use]
pub(crate) fn caption_targets(project: &Project, selected: &[usize]) -> Vec<usize> {
    if selected.is_empty() {
        (0..project.clips.len())
            .filter(|&i| !project.clips[i].is_title())
            .collect()
    } else {
        selected.to_vec()
    }
}

/// Entries that give every target `text` (empty text removes the caption).
/// Existing captions keep their style; new ones get `style`.
#[must_use]
pub(crate) fn caption_text_entries(
    project: &Project,
    targets: &[usize],
    text: &str,
    style: CaptionStyle,
) -> Vec<(usize, Option<Caption>)> {
    targets
        .iter()
        .map(|&i| {
            let caption = (!text.is_empty()).then(|| {
                let style = project.clips[i].caption.as_ref().map_or(style, |c| c.style);
                Caption::new(text, style)
            });
            (i, caption)
        })
        .collect()
}

/// Entries that change the style of the targets' existing captions.
#[must_use]
pub(crate) fn caption_style_entries(
    project: &Project,
    targets: &[usize],
    style: CaptionStyle,
) -> Vec<(usize, Option<Caption>)> {
    targets
        .iter()
        .filter_map(|&i| {
            let c = project.clips[i].caption.as_ref()?;
            (c.style != style).then(|| (i, Some(Caption::new(c.text.clone(), style))))
        })
        .collect()
}

/// Entries that fill captions from each target's media with `text_for`;
/// title cards are left out (they have no media), clips for which it returns
/// `None` (no date) are skipped. Returns the entries and how many clips were
/// skipped.
pub(crate) fn caption_fill_entries(
    project: &Project,
    targets: &[usize],
    style: CaptionStyle,
    text_for: impl Fn(&MediaRef) -> Option<String>,
) -> (Vec<(usize, Option<Caption>)>, usize) {
    let mut skipped = 0;
    let entries = targets
        .iter()
        .filter(|&&i| !project.clips[i].is_title())
        .filter_map(|&i| {
            let clip = &project.clips[i];
            let text = project.media_ref(clip.media).and_then(&text_for);
            if text.is_none() {
                skipped += 1;
            }
            let style = clip.caption.as_ref().map_or(style, |c| c.style);
            text.map(|t| (i, Some(Caption::new(t, style))))
        })
        .collect();
    (entries, skipped)
}

/// A caption from a file name: the name without its extension.
#[must_use]
pub(crate) fn caption_from_file_name(name: &str) -> String {
    std::path::Path::new(name)
        .file_stem()
        .map_or_else(|| name.to_owned(), |s| s.to_string_lossy().into_owned())
}

/// Where a trim drag started: the clip's trim and its on-screen edges.
/// Positions during the drag are computed from this anchor and the absolute
/// cursor x, so the edge sits exactly under the pointer.
#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) struct TrimAnchor {
    pub index: usize,
    /// Dragging the left (in) edge; otherwise the right (out) edge.
    pub left: bool,
    pub in_point: Ticks,
    pub out_point: Ticks,
    /// Length of the source video; the out point cannot pass it.
    pub natural: Ticks,
    /// Strip x of the clip's left and right edge when the drag started.
    pub start_x: f32,
    pub end_x: f32,
}

/// Shortest a trimmed clip may become.
pub(crate) const MIN_TRIM: Ticks = Ticks::from_millis(200);

/// New `(in, out)` with the dragged edge under `cursor` (strip x), clamped
/// to the source (0 .. natural) and to `MIN_TRIM`.
#[must_use]
pub(crate) fn trim_at_cursor(
    a: &TrimAnchor,
    cursor: f32,
    pixels_per_second: f32,
) -> (Ticks, Ticks) {
    let secs = |px: f32| Ticks::from_seconds_f64(f64::from(px / pixels_per_second));
    if a.left {
        // The right edge stays put: the clip spans cursor .. end_x.
        let len = secs(a.end_x - cursor);
        let new_in = (a.out_point - len).clamp(Ticks::ZERO, a.out_point - MIN_TRIM);
        (new_in, a.out_point)
    } else {
        // The left edge stays put: the clip spans start_x .. cursor.
        let len = secs(cursor - a.start_x);
        let new_out = (a.in_point + len).clamp(a.in_point + MIN_TRIM, a.natural);
        (a.in_point, new_out)
    }
}

/// Box of the clip being trimmed from its left edge: the right edge stays
/// where it was when the drag started, the left edge moves.
#[must_use]
pub(crate) fn left_trim_box(a: &TrimAnchor, in_point: Ticks, pixels_per_second: f32) -> ClipBox {
    #[allow(clippy::cast_possible_truncation)]
    let natural_w = (a.out_point - in_point).as_seconds_f64() as f32 * pixels_per_second;
    let width = natural_w.max(MIN_CLIP_WIDTH);
    ClipBox {
        x: a.end_x - width,
        width,
        overlap: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipforge_core::project::{Transition, TransitionKind};
    use clipforge_core::{Command, MediaId};

    fn photos(secs: &[i64]) -> Vec<Clip> {
        secs.iter()
            .map(|s| Clip::photo(MediaId::new(), Ticks::from_seconds(*s)))
            .collect()
    }

    #[test]
    fn layout_is_proportional_with_minimum_width() {
        let clips = photos(&[4, 1, 10]);
        let b = layout(&clips, 40.0);
        assert_eq!(
            b[0],
            ClipBox {
                x: 0.0,
                width: 160.0,
                overlap: 0.0
            }
        );
        assert_eq!(
            b[1],
            ClipBox {
                x: 160.0,
                width: MIN_CLIP_WIDTH,
                overlap: 0.0
            }
        );
        assert_eq!(b[2].x, 160.0 + MIN_CLIP_WIDTH);
        assert_eq!(strip_width(&b), 160.0 + MIN_CLIP_WIDTH + 400.0);
        assert!(layout(&[], 40.0).is_empty());
    }

    #[test]
    fn transitions_pull_clips_left() {
        let mut clips = photos(&[4, 4]);
        clips[1].transition_in = Transition {
            kind: TransitionKind::CrossDissolve,
            duration: Ticks::SECOND,
        };
        let b = layout(&clips, 40.0);
        assert_eq!(b[1].x, 120.0);
        assert_eq!(b[1].overlap, 40.0);
    }

    #[test]
    fn drop_index_and_hit_testing() {
        let clips = photos(&[2, 2, 2]);
        let b = layout(&clips, 40.0); // widths 80
        assert_eq!(drop_index(&b, -5.0), 0);
        assert_eq!(drop_index(&b, 39.0), 0);
        assert_eq!(drop_index(&b, 41.0), 1);
        assert_eq!(drop_index(&b, 199.0), 2);
        assert_eq!(drop_index(&b, 200.0), 3, "exactly at a centre goes after");
        assert_eq!(drop_index(&b, 1000.0), 3);
        assert_eq!(clip_at_x(&b, 0.0), Some(0));
        assert_eq!(clip_at_x(&b, 79.9), Some(0));
        assert_eq!(clip_at_x(&b, 80.0), Some(1));
        assert_eq!(clip_at_x(&b, 240.0), None);
    }

    #[test]
    fn time_and_pixels_round_trip() {
        let clips = photos(&[4, 1]);
        let b = layout(&clips, 40.0); // 160 px + 56 px (min width for 1 s)
        assert_eq!(time_at_x(&clips, &b, 80.0), Ticks::from_seconds(2));
        assert_eq!(x_at_time(&clips, &b, Ticks::from_seconds(2)), 80.0);
        // Inside the stretched short clip, half way = 4.5 s.
        let t = time_at_x(&clips, &b, 160.0 + 28.0);
        assert!((t.as_seconds_f64() - 4.5).abs() < 0.01);
        assert!((x_at_time(&clips, &b, t) - 188.0).abs() < 0.5);
        assert_eq!(time_at_x(&clips, &b, -10.0), Ticks::ZERO);
        assert_eq!(time_at_x(&clips, &b, 10_000.0), Ticks::from_seconds(5));
        assert_eq!(time_at_x(&[], &[], 10.0), Ticks::ZERO);
    }

    #[test]
    fn selection_click_semantics() {
        let clips = photos(&[1, 1, 1, 1, 1]);
        let mut s = Selection::default();
        s.click(&clips, 1, false, false);
        assert_eq!(s.indices(&clips), [1]);
        s.click(&clips, 3, true, false);
        assert_eq!(s.indices(&clips), [1, 2, 3], "shift extends from anchor");
        s.click(&clips, 0, false, true);
        assert_eq!(s.indices(&clips), [0, 1, 2, 3], "toggle adds");
        s.click(&clips, 2, false, true);
        assert_eq!(s.indices(&clips), [0, 1, 3], "toggle removes");
        s.click(&clips, 4, false, false);
        assert_eq!(s.indices(&clips), [4]);
        s.click(&clips, 9, false, false);
        assert_eq!(s.indices(&clips), [4], "out of range is ignored");
        s.select_all(&clips);
        assert_eq!(s.indices(&clips).len(), 5);
        let fewer = &clips[..2];
        s.retain_existing(fewer);
        assert_eq!(s.indices(fewer), [0, 1]);
        s.clear();
        assert!(s.is_empty());
    }

    #[test]
    fn clips_for_take_project_defaults() {
        let mut p = Project::new();
        p.settings.default_photo_duration = Ticks::from_seconds(7);
        p.settings.default_fit = clipforge_core::Fit::Cover;
        let r = MediaRef {
            id: MediaId::new(),
            kind: RefKind::Photo,
            path: "/a".into(),
            fingerprint_hash: 1,
            size: 1,
            pixel_size: Some((10, 10)),
            duration: None,
            captured_at_ms: None,
            name: "a".into(),
        };
        p.settings.default_motion = clipforge_core::Motion::ZoomOut;
        let c = clips_for(&p, std::slice::from_ref(&r));
        assert_eq!(c[0].duration(), Ticks::from_seconds(7));
        assert_eq!(
            c[0].motion,
            clipforge_core::Motion::ZoomOut,
            "photos get the default motion"
        );
        assert_eq!(c[0].fit, clipforge_core::Fit::Cover);
        let v = MediaRef {
            kind: RefKind::Video,
            duration: Some(Ticks::from_seconds(12)),
            ..r
        };
        let c = clips_for(&p, &[v]);
        assert!(!c[0].is_photo());
        assert_eq!(
            c[0].motion,
            clipforge_core::Motion::None,
            "videos never get motion"
        );
        assert_eq!(c[0].duration(), Ticks::from_seconds(12));
    }

    fn mixed_project() -> Project {
        let mut p = Project::new();
        let photo = MediaRef {
            id: MediaId::new(),
            kind: RefKind::Photo,
            path: "/p".into(),
            fingerprint_hash: 1,
            size: 1,
            pixel_size: Some((10, 10)),
            duration: None,
            captured_at_ms: None,
            name: "p".into(),
        };
        let video = MediaRef {
            id: MediaId::new(),
            kind: RefKind::Video,
            duration: Some(Ticks::from_seconds(8)),
            ..photo.clone()
        };
        let entries = vec![
            (0, Clip::photo(photo.id, Ticks::from_seconds(4))),
            (1, Clip::video(video.id, Ticks::from_seconds(8))),
            (2, Clip::photo(photo.id, Ticks::from_seconds(1))),
            (3, Clip::photo(photo.id, Ticks::from_seconds(4))),
        ];
        clipforge_core::Command::InsertClips {
            entries,
            media: vec![photo, video],
        }
        .apply(&mut p)
        .unwrap();
        p
    }

    #[test]
    fn photo_targets_skip_videos() {
        let p = mixed_project();
        assert_eq!(photo_targets(&p, &[0, 1, 2, 3]), [0, 2, 3]);
        assert_eq!(photo_targets(&p, &[1]), Vec::<usize>::new());
        assert_eq!(
            photo_targets(&p, &[9]),
            Vec::<usize>::new(),
            "stale index ignored"
        );
    }

    #[test]
    fn capped_transition_is_reported_when_a_neighbour_is_too_short() {
        let p = mixed_project();
        // Clip 3 follows the 1 s photo: anything over 0.5 s is shortened.
        assert!(transition_capped(&p, &[3], Ticks::from_millis(800)));
        // Clip 2 itself is 1 s long.
        assert!(transition_capped(&p, &[2], Ticks::from_millis(600)));
        // First clip: capped by its own half only.
        assert!(transition_capped(&p, &[0], Ticks::from_millis(2_500)));
    }

    #[test]
    fn uncapped_transition_is_not_reported() {
        let p = mixed_project();
        assert!(!transition_capped(&p, &[1], Ticks::SECOND));
        assert!(!transition_capped(&p, &[0, 1], Ticks::from_millis(2_000)));
        assert!(!transition_capped(&p, &[], Ticks::from_seconds(10)));
    }

    #[test]
    fn clip_view_flags_follow_motion_and_transition() {
        let mut p = mixed_project();
        assert_eq!(
            clip_flags(&p.clips[0]),
            ClipFlags {
                transition: 0,
                moving: false
            }
        );
        p.clips[0].motion = clipforge_core::Motion::PanLeft;
        p.clips[0].transition_in = Transition {
            kind: TransitionKind::SlideUp,
            duration: Ticks::SECOND,
        };
        assert_eq!(
            clip_flags(&p.clips[0]),
            ClipFlags {
                transition: 6,
                moving: true
            }
        );
        p.clips[1].motion = clipforge_core::Motion::ZoomIn;
        assert!(
            !clip_flags(&p.clips[1]).moving,
            "videos never show the motion badge"
        );
    }
    #[test]
    fn nudge_left_and_right_move_the_block_by_one() {
        // Block [2, 3] of 6 clips.
        assert_eq!(nudge_target(6, &[2, 3], Nudge::Earlier), Some(1));
        assert_eq!(nudge_target(6, &[2, 3], Nudge::Later), Some(5));
        let Some(Command::Reorder { order }) = Command::move_clips(6, &[2, 3], 1) else {
            panic!("earlier must reorder");
        };
        assert_eq!(order, vec![0, 2, 3, 1, 4, 5]);
        let Some(Command::Reorder { order }) = Command::move_clips(6, &[2, 3], 5) else {
            panic!("later must reorder");
        };
        assert_eq!(order, vec![0, 1, 4, 2, 3, 5]);
    }

    #[test]
    fn nudge_at_the_edges_is_none() {
        assert_eq!(nudge_target(4, &[0], Nudge::Earlier), None);
        assert_eq!(nudge_target(4, &[3], Nudge::Later), None);
        assert_eq!(nudge_target(4, &[2, 3], Nudge::Later), None);
        assert_eq!(nudge_target(4, &[], Nudge::Later), None);
        assert_eq!(nudge_target(4, &[9], Nudge::Earlier), None, "stale index");
        // Second to last moves to the end.
        assert_eq!(nudge_target(4, &[2], Nudge::Later), Some(4));
    }

    #[test]
    fn nudge_gathers_a_scattered_selection() {
        // [1, 4] of 6, earlier: the block lands before the clip preceding the
        // first selected one.
        let to = nudge_target(6, &[1, 4], Nudge::Earlier);
        assert_eq!(to, Some(0));
        let Some(Command::Reorder { order }) = Command::move_clips(6, &[1, 4], 0) else {
            panic!("must reorder");
        };
        assert_eq!(order, vec![1, 4, 0, 2, 3, 5]);
        // Later: after the clip following the last selected one.
        assert_eq!(nudge_target(6, &[1, 4], Nudge::Later), Some(6));
    }

    #[test]
    fn clip_nav_clamps_at_both_ends() {
        assert_eq!(clip_nav(None, 0, true), None, "empty timeline");
        assert_eq!(clip_nav(None, 3, true), Some(0), "no focus yet: first clip");
        assert_eq!(
            clip_nav(None, 3, false),
            Some(2),
            "no focus yet going back: last clip"
        );
        assert_eq!(clip_nav(Some(0), 3, false), Some(0));
        assert_eq!(clip_nav(Some(1), 3, true), Some(2));
        assert_eq!(clip_nav(Some(2), 3, true), Some(2));
        assert_eq!(
            clip_nav(Some(7), 3, false),
            Some(1),
            "stale index clamps, then moves"
        );
    }

    #[test]
    fn timeline_focus_follows_clicks_and_survives_removal() {
        let clips = photos(&[1, 1, 1, 1]);
        let mut sel = Selection::default();
        sel.click(&clips, 2, false, false);
        assert_eq!(sel.focus_index(&clips), Some(2));
        sel.set_focus(&clips, 3);
        assert_eq!(sel.focus_index(&clips), Some(3));
        assert_eq!(sel.indices(&clips), vec![2], "focus alone does not select");
        sel.toggle_focused(&clips);
        assert_eq!(sel.indices(&clips), vec![2, 3]);
        let fewer = clips[..3].to_vec();
        sel.retain_existing(&fewer);
        assert_eq!(sel.focus_index(&fewer), None, "focused clip removed");
    }

    fn anchor(left: bool) -> TrimAnchor {
        // A 10 s video trimmed to 2..6 s, drawn at 40 px/s from x = 100 to 260.
        TrimAnchor {
            index: 0,
            left,
            in_point: Ticks::from_seconds(2),
            out_point: Ticks::from_seconds(6),
            natural: Ticks::from_seconds(10),
            start_x: 100.0,
            end_x: 260.0,
        }
    }

    #[test]
    fn right_edge_follows_the_cursor_exactly() {
        let a = anchor(false);
        assert_eq!(
            trim_at_cursor(&a, 260.0, 40.0),
            (Ticks::from_seconds(2), Ticks::from_seconds(6)),
            "no move, no change"
        );
        assert_eq!(trim_at_cursor(&a, 300.0, 40.0).1, Ticks::from_seconds(7));
        assert_eq!(trim_at_cursor(&a, 180.0, 40.0).1, Ticks::from_seconds(4));
    }

    #[test]
    fn right_edge_stops_at_the_end_of_the_video_and_the_minimum() {
        let a = anchor(false);
        assert_eq!(
            trim_at_cursor(&a, 10_000.0, 40.0).1,
            Ticks::from_seconds(10),
            "cannot pass the source"
        );
        assert_eq!(
            trim_at_cursor(&a, 0.0, 40.0).1,
            Ticks::from_seconds(2) + MIN_TRIM,
            "cannot invert"
        );
    }

    #[test]
    fn left_edge_follows_the_cursor_and_keeps_the_right_edge() {
        let a = anchor(true);
        assert_eq!(
            trim_at_cursor(&a, 100.0, 40.0),
            (Ticks::from_seconds(2), Ticks::from_seconds(6))
        );
        assert_eq!(
            trim_at_cursor(&a, 140.0, 40.0).0,
            Ticks::from_seconds(3),
            "drag right: later in point"
        );
        assert_eq!(
            trim_at_cursor(&a, 60.0, 40.0).0,
            Ticks::from_seconds(1),
            "drag left: earlier in point"
        );
        assert_eq!(
            trim_at_cursor(&a, -500.0, 40.0).0,
            Ticks::ZERO,
            "cannot pass the start of the video"
        );
        assert_eq!(
            trim_at_cursor(&a, 5_000.0, 40.0).0,
            Ticks::from_seconds(6) - MIN_TRIM
        );
        // The box keeps its right edge and puts the left edge under the cursor.
        let b = left_trim_box(&a, Ticks::from_seconds(3), 40.0);
        assert!(
            (b.x - 140.0).abs() < 1e-3 && (b.x + b.width - 260.0).abs() < 1e-3,
            "{b:?}"
        );
        // Very short: minimum width, right edge still fixed.
        let b = left_trim_box(&a, Ticks::from_seconds(6) - MIN_TRIM, 40.0);
        assert_eq!(b.width, MIN_CLIP_WIDTH);
        assert!((b.x + b.width - 260.0).abs() < 1e-3);
    }

    fn audio(secs: i64, name: &str) -> MediaRef {
        MediaRef {
            id: MediaId::new(),
            kind: RefKind::Audio,
            path: "/a".into(),
            fingerprint_hash: 1,
            size: 1,
            pixel_size: None,
            duration: Some(Ticks::from_seconds(secs)),
            captured_at_ms: None,
            name: name.into(),
        }
    }

    fn photo_project(secs: &[i64]) -> Project {
        let mut p = Project::new();
        let m = MediaRef {
            kind: RefKind::Photo,
            duration: None,
            name: "IMG_1.jpg".into(),
            captured_at_ms: Some(1_720_000_000_000),
            ..audio(0, "")
        };
        Command::InsertClips {
            entries: secs
                .iter()
                .enumerate()
                .map(|(i, s)| (i, Clip::photo(m.id, Ticks::from_seconds(*s))))
                .collect(),
            media: vec![m],
        }
        .apply(&mut p)
        .unwrap();
        p
    }

    fn with_songs(mut p: Project, songs: &[(i64, &str)]) -> Project {
        let refs: Vec<MediaRef> = songs.iter().map(|(s, n)| audio(*s, n)).collect();
        let music = clipforge_core::Music {
            songs: refs
                .iter()
                .map(|r| clipforge_core::Song::new(r.id))
                .collect(),
            ..clipforge_core::Music::default()
        };
        Command::SetMusic { music, media: refs }
            .apply(&mut p)
            .unwrap();
        p
    }

    #[test]
    fn song_blocks_follow_the_playlist_and_mark_repeats() {
        let p = with_songs(
            photo_project(&[4, 4, 4, 4, 4]),
            &[(7, "a.mp3"), (5, "b.mp3")],
        );
        let boxes = layout(&p.clips, 20.0);
        let lane = music_lane(&p, &p.clips, &boxes);
        let summary: Vec<(usize, f32, f32, bool)> = lane
            .blocks
            .iter()
            .map(|b| (b.song, b.x, b.width, b.repeat))
            .collect();
        assert_eq!(
            summary,
            vec![
                (0, 0.0, 140.0, false),
                (1, 140.0, 100.0, false),
                (0, 240.0, 140.0, true),
                (1, 380.0, 20.0, true),
            ]
        );
        assert_eq!(lane.blocks[0].title, "a.mp3");
        assert!((lane.end_x - 400.0).abs() < 1e-3);
        assert!((lane.fade_x - 340.0).abs() < 1e-3, "3 s fade out");
    }

    #[test]
    fn song_blocks_are_empty_without_songs() {
        let p = photo_project(&[4]);
        let lane = music_lane(&p, &p.clips, &layout(&p.clips, 10.0));
        assert!(lane.blocks.is_empty());
        let p = with_songs(Project::new(), &[(7, "a.mp3")]);
        assert!(
            music_lane(&p, &p.clips, &[]).blocks.is_empty(),
            "no clips, no show"
        );
    }

    #[test]
    fn caption_for_multi_selection_is_shared_or_mixed() {
        let mut p = photo_project(&[4, 4, 4]);
        let v = caption_view(&p, &[0, 1], CaptionStyle::Banner);
        assert_eq!(
            v,
            CaptionView {
                text: String::new(),
                mixed: false,
                style: CaptionStyle::Banner,
                any: false
            }
        );
        Command::SetCaptions {
            entries: caption_text_entries(&p, &[0, 1], "Rome", CaptionStyle::Corner),
        }
        .apply(&mut p)
        .unwrap();
        let v = caption_view(&p, &[0, 1], CaptionStyle::Classic);
        assert_eq!(
            (v.text.as_str(), v.mixed, v.style, v.any),
            ("Rome", false, CaptionStyle::Corner, true)
        );
        let v = caption_view(&p, &[0, 1, 2], CaptionStyle::Classic);
        assert!(v.mixed && v.text.is_empty() && v.any);
        // Style changes only touch existing captions; empty text removes.
        let e = caption_style_entries(&p, &[0, 1, 2], CaptionStyle::Banner);
        assert_eq!(e.len(), 2);
        let e = caption_text_entries(&p, &[0], "", CaptionStyle::Classic);
        assert_eq!(e, vec![(0, None)]);
    }

    #[test]
    fn caption_from_file_name_drops_the_extension() {
        assert_eq!(caption_from_file_name("IMG_4021.jpg"), "IMG_4021");
        assert_eq!(
            caption_from_file_name("Rome at dusk.final.HEIC"),
            "Rome at dusk.final"
        );
        assert_eq!(caption_from_file_name("noext"), "noext");
        let p = photo_project(&[4, 4]);
        let (entries, skipped) = caption_fill_entries(&p, &[0, 1], CaptionStyle::Classic, |m| {
            Some(caption_from_file_name(&m.name))
        });
        assert_eq!(skipped, 0);
        assert_eq!(entries[1].1.as_ref().unwrap().text, "IMG_1");
    }

    #[test]
    fn library_ids_split_into_clips_and_songs() {
        let p = photo_project(&[4]);
        let photo = p.media.values().next().unwrap().clone();
        let (clips, songs) = split_songs(vec![audio(3, "s"), photo.clone(), audio(4, "t")]);
        assert_eq!(clips, vec![photo]);
        assert_eq!(songs.len(), 2);
    }

    #[test]
    fn title_background_colours_match_the_renderer() {
        use clipforge_core::project::TitleBackground;
        let theme = include_str!("../ui/theme.slint");
        for (bg, token) in TitleBackground::ALL
            .iter()
            .zip(["black", "charcoal", "blue", "red", "white"])
        {
            let key = format!("title-bg-{token}: #");
            let at = theme.find(&key).unwrap_or_else(|| panic!("{key} missing")) + key.len();
            let hex = &theme[at..at + 6];
            let rgb = [0, 2, 4].map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap());
            assert_eq!(rgb, clipforge_render::title_rgb(*bg), "{token}");
        }
    }

    #[test]
    fn bulk_captions_leave_title_cards_alone() {
        use clipforge_core::project::TitleBackground;
        let mut p = photo_project(&[4, 4]);
        Command::InsertClips {
            entries: vec![(
                0,
                Clip::title("Summer", TitleBackground::Black, Ticks::SECOND),
            )],
            media: vec![],
        }
        .apply(&mut p)
        .unwrap();
        assert_eq!(
            caption_targets(&p, &[]),
            vec![1, 2],
            "nothing selected: no titles"
        );
        assert_eq!(
            caption_targets(&p, &[0, 2]),
            vec![0, 2],
            "an explicit selection keeps them"
        );
        let (entries, skipped) = caption_fill_entries(&p, &[0, 1, 2], CaptionStyle::Classic, |m| {
            Some(m.name.clone())
        });
        assert_eq!(entries.len(), 2);
        assert_eq!(skipped, 0, "title cards are not counted as missing a date");
    }
}
