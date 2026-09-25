//! Timeline maths: where clips sit in time, what plays at a given instant.

use crate::project::{Clip, Project};
use crate::time::Ticks;

/// Placement of a clip on the timeline.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Placement {
    pub index: usize,
    pub start: Ticks,
    pub end: Ticks,
}

impl Placement {
    #[must_use]
    pub fn duration(self) -> Ticks {
        self.end - self.start
    }

    #[must_use]
    pub fn contains(self, t: Ticks) -> bool {
        t >= self.start && t < self.end
    }
}

/// Effective overlap of clip `i` with its predecessor: never more than half
/// of either neighbour, so transitions cannot swallow a clip.
#[must_use]
pub fn effective_overlap(clips: &[Clip], i: usize) -> Ticks {
    if i == 0 {
        return Ticks::ZERO;
    }
    let wanted = clips[i].transition_in.overlap();
    let cap = (clips[i - 1].duration() / 2).min(clips[i].duration() / 2);
    wanted.min(cap).max(Ticks::ZERO)
}

/// How long the first clip's transition plays in from black (or white) at
/// the very start of the show: its own transition, capped at half the clip
/// like any other overlap.
#[must_use]
pub fn opening_overlap(clips: &[Clip]) -> Ticks {
    clips.first().map_or(Ticks::ZERO, |c| {
        c.transition_in
            .overlap()
            .min(c.duration() / 2)
            .max(Ticks::ZERO)
    })
}

/// Start/end of every clip, accounting for transition overlap.
#[must_use]
pub fn placements(clips: &[Clip]) -> Vec<Placement> {
    let mut out = Vec::with_capacity(clips.len());
    let mut cursor = Ticks::ZERO;
    for (i, clip) in clips.iter().enumerate() {
        let start = cursor - effective_overlap(clips, i);
        let end = start + clip.duration();
        out.push(Placement {
            index: i,
            start,
            end,
        });
        cursor = end;
    }
    out
}

/// Total length of the sequence.
#[must_use]
pub fn total_duration(clips: &[Clip]) -> Ticks {
    placements(clips).last().map_or(Ticks::ZERO, |p| p.end)
}

/// Shortest photo duration "fit to length" hands out.
pub const MIN_FIT_PHOTO: Ticks = Ticks::from_millis(500);

/// One duration for every photo so the show lasts `target` (as close as
/// the transition caps allow; videos keep their length). `None` when there
/// are no photos. Clamped to [`MIN_FIT_PHOTO`] when the videos alone are
/// already longer than `target`.
#[must_use]
pub fn fit_photo_duration(clips: &[Clip], target: Ticks) -> Option<Ticks> {
    if !clips.iter().any(Clip::is_photo) {
        return None;
    }
    let with = |d: Ticks| -> Ticks {
        let trial: Vec<Clip> = clips
            .iter()
            .map(|c| {
                let mut c = c.clone();
                if c.is_photo() {
                    c.source = crate::project::ClipSource::Photo { duration: d };
                }
                c
            })
            .collect();
        total_duration(&trial)
    };
    // The total grows monotonically with the photo duration: bisect on
    // whole milliseconds for the largest duration not exceeding `target`.
    let ms = Ticks::from_millis(1);
    let (mut lo, mut hi) = (MIN_FIT_PHOTO.flicks() / ms.flicks(), 1i64);
    if with(MIN_FIT_PHOTO) >= target {
        return Some(MIN_FIT_PHOTO);
    }
    while with(Ticks::from_millis(hi)) < target {
        hi *= 2;
        if hi > 1_000_000_000 {
            return None;
        }
    }
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        if with(Ticks::from_millis(mid)) <= target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    // Pick whichever neighbour lands closer to the target.
    let (a, b) = (Ticks::from_millis(lo), Ticks::from_millis(hi));
    let da = (target - with(a)).flicks().abs();
    let db = (with(b) - target).flicks().abs();
    Some(if db < da { b } else { a })
}

/// What is visible at `t`: the clip that owns the instant and, during a
/// transition, the outgoing clip with the transition progress in `0..1`.
#[derive(Clone, Debug, PartialEq)]
pub struct FrameAt {
    /// Incoming / current clip and the time within it.
    pub current: (usize, Ticks),
    /// Outgoing clip (index, local time) while a transition is active.
    pub outgoing: Option<(usize, Ticks)>,
    /// Transition progress 0..=1 when `outgoing` is set.
    pub progress: f32,
}

/// Resolves the timeline time `t`. Returns `None` for an empty sequence or
/// `t` beyond the end.
#[must_use]
pub fn frame_at(clips: &[Clip], t: Ticks) -> Option<FrameAt> {
    if clips.is_empty() || t < Ticks::ZERO {
        return None;
    }
    let places = placements(clips);
    if t >= places[places.len() - 1].end {
        return None;
    }
    // Latest clip whose start is <= t is the "current" (incoming) clip.
    let idx = places.iter().rposition(|p| p.start <= t)?;
    let p = places[idx];
    let current = (idx, t - p.start);
    // Overlap with predecessor?
    if idx > 0 {
        let prev = places[idx - 1];
        if t < prev.end {
            let overlap = prev.end - p.start;
            #[allow(clippy::cast_precision_loss)]
            let progress = if overlap.flicks() > 0 {
                ((t - p.start).flicks() as f64 / overlap.flicks() as f64) as f32
            } else {
                1.0
            };
            return Some(FrameAt {
                current,
                outgoing: Some((idx - 1, t - prev.start)),
                progress,
            });
        }
    }
    Some(FrameAt {
        current,
        outgoing: None,
        progress: 1.0,
    })
}

/// Convenience for [`placements`] on a project.
#[must_use]
pub fn project_placements(project: &Project) -> Vec<Placement> {
    placements(&project.clips)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::MediaId;
    use crate::project::{Transition, TransitionKind};

    fn photo(secs: i64) -> Clip {
        Clip::photo(MediaId::new(), Ticks::from_seconds(secs))
    }

    fn with_dissolve(mut c: Clip, secs_ms: i64) -> Clip {
        c.transition_in = Transition {
            kind: TransitionKind::CrossDissolve,
            duration: Ticks::from_millis(secs_ms),
        };
        c
    }

    #[test]
    fn cuts_lay_clips_end_to_end() {
        let clips = vec![photo(3), photo(2), photo(5)];
        let p = placements(&clips);
        assert_eq!(
            p[0],
            Placement {
                index: 0,
                start: Ticks::ZERO,
                end: Ticks::from_seconds(3)
            }
        );
        assert_eq!(p[1].start, Ticks::from_seconds(3));
        assert_eq!(p[2].end, Ticks::from_seconds(10));
        assert_eq!(total_duration(&clips), Ticks::from_seconds(10));
        assert_eq!(total_duration(&[]), Ticks::ZERO);
    }

    #[test]
    fn transitions_overlap_and_shorten_the_total() {
        let clips = vec![
            photo(4),
            with_dissolve(photo(4), 1000),
            with_dissolve(photo(4), 500),
        ];
        let p = placements(&clips);
        assert_eq!(p[1].start, Ticks::from_seconds(3));
        assert_eq!(p[2].start, Ticks::from_millis(6_500));
        assert_eq!(total_duration(&clips), Ticks::from_millis(10_500));
    }

    #[test]
    fn overlap_is_capped_at_half_of_the_shorter_neighbour() {
        let clips = vec![photo(1), with_dissolve(photo(10), 5000)];
        assert_eq!(effective_overlap(&clips, 1), Ticks::from_millis(500));
        assert_eq!(effective_overlap(&clips, 0), Ticks::ZERO);
        let first_with_transition = vec![with_dissolve(photo(4), 1000)];
        assert_eq!(
            placements(&first_with_transition)[0].start,
            Ticks::ZERO,
            "first clip never shifts"
        );
    }

    #[test]
    fn opening_overlap_is_capped_like_any_overlap() {
        assert_eq!(opening_overlap(&[]), Ticks::ZERO);
        assert_eq!(
            opening_overlap(&[photo(4)]),
            Ticks::ZERO,
            "a cut has no opening"
        );
        assert_eq!(
            opening_overlap(&[with_dissolve(photo(4), 1000)]),
            Ticks::SECOND
        );
        assert_eq!(
            opening_overlap(&[with_dissolve(photo(1), 3000)]),
            Ticks::from_millis(500)
        );
    }

    #[test]
    fn frame_at_resolves_current_and_outgoing() {
        let clips = vec![photo(4), with_dissolve(photo(4), 1000)];
        // Middle of clip 0.
        let f = frame_at(&clips, Ticks::from_seconds(1)).unwrap();
        assert_eq!(f.current, (0, Ticks::from_seconds(1)));
        assert_eq!(f.outgoing, None);
        // In the overlap (3.0..4.0): current = clip 1, outgoing = clip 0.
        let f = frame_at(&clips, Ticks::from_millis(3_500)).unwrap();
        assert_eq!(f.current, (1, Ticks::from_millis(500)));
        assert_eq!(f.outgoing, Some((0, Ticks::from_millis(3_500))));
        assert!((f.progress - 0.5).abs() < 1e-6);
        // After the overlap.
        let f = frame_at(&clips, Ticks::from_seconds(5)).unwrap();
        assert_eq!(f.current, (1, Ticks::from_seconds(2)));
        assert_eq!(f.outgoing, None);
        // Boundaries.
        assert!(frame_at(&clips, Ticks::from_seconds(7)).is_none());
        assert!(frame_at(&clips, Ticks::from_flicks(-1)).is_none());
        assert!(frame_at(&[], Ticks::ZERO).is_none());
        assert_eq!(
            frame_at(&clips, Ticks::ZERO).unwrap().current,
            (0, Ticks::ZERO)
        );
    }

    #[test]
    fn fit_photo_duration_hits_the_target_around_videos_and_overlaps() {
        use crate::project::ClipSource;
        let video = Clip {
            source: ClipSource::Video {
                in_point: Ticks::ZERO,
                out_point: Ticks::from_seconds(6),
            },
            ..photo(1)
        };
        let clips = vec![
            photo(4),
            with_dissolve(photo(4), 1000),
            video,
            with_dissolve(photo(4), 1000),
        ];
        let target = Ticks::from_seconds(60);
        let d = fit_photo_duration(&clips, target).unwrap();
        let fitted: Vec<Clip> = clips
            .iter()
            .map(|c| {
                let mut c = c.clone();
                if c.is_photo() {
                    c.source = ClipSource::Photo { duration: d };
                }
                c
            })
            .collect();
        let total = total_duration(&fitted);
        assert!(
            (total - target).flicks().abs() <= Ticks::from_millis(2).flicks(),
            "{total}"
        );
        // (60 - 6 + 2 overlaps of 1 s) / 3 photos = 18.667 s
        assert!((d.as_seconds_f64() - 18.667).abs() < 0.01, "{d}");
    }

    #[test]
    fn fit_photo_duration_edge_cases() {
        assert_eq!(fit_photo_duration(&[], Ticks::SECOND), None);
        let short = vec![photo(4), photo(4)];
        assert_eq!(
            fit_photo_duration(&short, Ticks::from_millis(100)),
            Some(MIN_FIT_PHOTO)
        );
        assert_eq!(
            fit_photo_duration(&short, Ticks::from_seconds(10)),
            Some(Ticks::from_seconds(5))
        );
    }

    #[test]
    fn cut_boundary_belongs_to_the_next_clip() {
        let clips = vec![photo(2), photo(2)];
        assert_eq!(
            frame_at(&clips, Ticks::from_seconds(2)).unwrap().current,
            (1, Ticks::ZERO)
        );
        assert_eq!(
            frame_at(&clips, Ticks::from_seconds(2) - Ticks::from_flicks(1))
                .unwrap()
                .current
                .0,
            0
        );
    }
}
