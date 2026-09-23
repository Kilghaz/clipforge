//! Pure view logic for the editor: timeline geometry, selection semantics
//! and media-reference building. No Slint here.

use std::collections::BTreeSet;

use clipforge_core::timeline::{effective_overlap, placements};
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

/// Selection with an anchor for shift-clicks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Selection {
    pub ids: BTreeSet<ClipId>,
    pub anchor: Option<ClipId>,
}

impl Selection {
    /// Applies a click on `index` with the given modifiers.
    pub(crate) fn click(&mut self, clips: &[Clip], index: usize, shift: bool, toggle: bool) {
        let Some(clicked) = clips.get(index).map(|c| c.id) else {
            return;
        };
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

    /// Drops ids that no longer exist.
    pub(crate) fn retain_existing(&mut self, clips: &[Clip]) {
        let existing: BTreeSet<ClipId> = clips.iter().map(|c| c.id).collect();
        self.ids.retain(|id| existing.contains(id));
        if let Some(a) = self.anchor
            && !existing.contains(&a)
        {
            self.anchor = None;
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
    let kind = match record.kind {
        MediaKind::Photo => RefKind::Photo,
        MediaKind::Video if info.duration.is_some_and(|d| d > Ticks::ZERO) => RefKind::Video,
        _ => return None,
    };
    Some(MediaRef {
        id: record.id,
        kind,
        path: record.path.clone(),
        fingerprint_hash: record.fingerprint.hash,
        size: record.fingerprint.size,
        pixel_size: info.display_size(),
        duration: if kind == RefKind::Video {
            info.duration
        } else {
            None
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
            c
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipforge_core::MediaId;
    use clipforge_core::project::{Transition, TransitionKind};

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
        let c = clips_for(&p, std::slice::from_ref(&r));
        assert_eq!(c[0].duration(), Ticks::from_seconds(7));
        assert_eq!(c[0].fit, clipforge_core::Fit::Cover);
        let v = MediaRef {
            kind: RefKind::Video,
            duration: Some(Ticks::from_seconds(12)),
            ..r
        };
        let c = clips_for(&p, &[v]);
        assert!(!c[0].is_photo());
        assert_eq!(c[0].duration(), Ticks::from_seconds(12));
    }
}
