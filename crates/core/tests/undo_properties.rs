//! Property tests: any sequence of valid commands is exactly undone by
//! replaying the inverses, and invariants hold after every step.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use clipforge_core::{
    Clip, Command, Fit, History, MediaId, MediaRef, Motion, Music, Project, ProjectSettings,
    Quarter, RefKind, Song, Ticks, Transition, TransitionKind,
};
use proptest::prelude::*;

fn media(kind: RefKind, duration: Option<Ticks>) -> MediaRef {
    MediaRef {
        id: MediaId::new(),
        kind,
        path: "/m".into(),
        fingerprint_hash: 7,
        size: 7,
        pixel_size: Some((1600, 900)),
        duration,
        captured_at_ms: None,
        name: "m".into(),
    }
}

/// A project with `photos` photo clips and `videos` video clips.
fn seed(photos: usize, videos: usize) -> Project {
    let mut p = Project::new();
    let photo = media(RefKind::Photo, None);
    let video = media(RefKind::Video, Some(Ticks::from_seconds(10)));
    let mut entries = Vec::new();
    for i in 0..photos {
        entries.push((i, Clip::photo(photo.id, Ticks::from_seconds(3))));
    }
    for i in 0..videos {
        entries.push((photos + i, Clip::video(video.id, Ticks::from_seconds(10))));
    }
    Command::InsertClips {
        entries,
        media: vec![photo, video],
    }
    .apply(&mut p)
    .unwrap();
    p
}

/// Abstract operations; turned into concrete commands against the current
/// project length so they are always valid.
#[derive(Debug, Clone)]
enum Op {
    Insert(u8),
    Remove(Vec<u8>),
    Move(Vec<u8>, u8),
    Duration(Vec<u8>, u8),
    Fit(Vec<u8>, bool),
    Rotate(Vec<u8>),
    Transition(Vec<u8>, u8),
    Trim(u8, u8, u8),
    Settings(u8),
    Motion(Vec<u8>, u8),
    ShuffleTransitions(Vec<u8>, u64),
    ShuffleMotion(Vec<u8>, u64),
    Music(u8, u8, bool, bool),
}

fn op() -> impl Strategy<Value = Op> {
    let idx = proptest::collection::vec(0u8..8, 1..4);
    prop_oneof![
        (0u8..8).prop_map(Op::Insert),
        idx.clone().prop_map(Op::Remove),
        (idx.clone(), 0u8..9).prop_map(|(i, t)| Op::Move(i, t)),
        (idx.clone(), 1u8..10).prop_map(|(i, d)| Op::Duration(i, d)),
        (idx.clone(), any::<bool>()).prop_map(|(i, c)| Op::Fit(i, c)),
        idx.clone().prop_map(Op::Rotate),
        (idx.clone(), 0u8..3).prop_map(|(i, t)| Op::Transition(i, t)),
        (0u8..8, 0u8..5, 1u8..6).prop_map(|(i, a, b)| Op::Trim(i, a, b)),
        (1u8..10).prop_map(Op::Settings),
        (idx.clone(), 0u8..7).prop_map(|(i, m)| Op::Motion(i, m)),
        (idx.clone(), any::<u64>()).prop_map(|(i, s)| Op::ShuffleTransitions(i, s)),
        (idx, any::<u64>()).prop_map(|(i, s)| Op::ShuffleMotion(i, s)),
        (0u8..4, 0u8..=200, any::<bool>(), any::<bool>())
            .prop_map(|(n, v, l, d)| Op::Music(n, v, l, d)),
    ]
}

fn pick(indices: &[u8], len: usize) -> Vec<usize> {
    let mut v: Vec<usize> = indices
        .iter()
        .map(|i| usize::from(*i) % len.max(1))
        .filter(|i| *i < len)
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// Converts an op into a command valid for `p`, or `None` to skip.
fn concrete(op: &Op, p: &Project) -> Option<Command> {
    let len = p.clips.len();
    let photo_id = p.media.values().find(|m| m.kind == RefKind::Photo)?.id;
    Some(match op {
        Op::Insert(at) => Command::InsertClips {
            entries: vec![(
                usize::from(*at) % (len + 1),
                Clip::photo(photo_id, Ticks::from_seconds(2)),
            )],
            media: vec![],
        },
        Op::Remove(i) => {
            let idx = pick(i, len);
            if idx.is_empty() {
                return None;
            }
            Command::RemoveClips { indices: idx }
        }
        Op::Move(i, to) => Command::move_clips(len, &pick(i, len), usize::from(*to) % (len + 1))?,
        Op::Duration(i, d) => {
            let idx: Vec<usize> = pick(i, len)
                .into_iter()
                .filter(|&k| p.clips[k].is_photo())
                .collect();
            if idx.is_empty() {
                return None;
            }
            Command::SetPhotoDuration {
                indices: idx,
                duration: Ticks::from_seconds(i64::from(*d)),
            }
        }
        Op::Fit(i, cover) => {
            let idx = pick(i, len);
            if idx.is_empty() {
                return None;
            }
            Command::SetFit {
                indices: idx,
                fit: if *cover { Fit::Cover } else { Fit::Contain },
            }
        }
        Op::Rotate(i) => {
            let idx = pick(i, len);
            if idx.is_empty() {
                return None;
            }
            Command::SetRotate {
                indices: idx,
                rotate: Quarter::Cw90,
            }
        }
        Op::Transition(i, t) => {
            let idx = pick(i, len);
            if idx.is_empty() {
                return None;
            }
            let kind = [
                TransitionKind::Cut,
                TransitionKind::CrossDissolve,
                TransitionKind::FadeThroughBlack,
            ][usize::from(*t)];
            Command::SetTransition {
                indices: idx,
                transition: Transition {
                    kind,
                    duration: Ticks::from_millis(700),
                },
            }
        }
        Op::Trim(i, a, b) => {
            if len == 0 {
                return None;
            }
            let k = usize::from(*i) % len;
            if p.clips[k].is_photo() {
                return None;
            }
            let in_point = Ticks::from_seconds(i64::from(*a));
            Command::SetTrim {
                index: k,
                in_point,
                out_point: in_point + Ticks::from_seconds(i64::from(*b)),
            }
        }
        Op::Settings(d) => Command::SetSettings {
            settings: ProjectSettings {
                default_photo_duration: Ticks::from_seconds(i64::from(*d)),
                ..ProjectSettings::default()
            },
        },
        Op::Motion(i, m) => {
            let idx: Vec<usize> = pick(i, len)
                .into_iter()
                .filter(|&k| p.clips[k].is_photo())
                .collect();
            if idx.is_empty() {
                return None;
            }
            Command::SetMotion {
                indices: idx,
                motion: Motion::from_index(usize::from(*m)),
            }
        }
        Op::ShuffleTransitions(i, seed) => {
            let idx = pick(i, len);
            if idx.is_empty() {
                return None;
            }
            Command::SetTransitionEach {
                entries: clipforge_core::shuffle::transitions(&idx, Ticks::from_millis(600), *seed),
            }
        }
        Op::ShuffleMotion(i, seed) => {
            let idx: Vec<usize> = pick(i, len)
                .into_iter()
                .filter(|&k| p.clips[k].is_photo())
                .collect();
            if idx.is_empty() {
                return None;
            }
            Command::SetMotionEach {
                entries: clipforge_core::shuffle::motions(&idx, *seed),
            }
        }
        Op::Music(songs, volume, looped, duck) => Command::SetMusic {
            music: Music {
                songs: (0..*songs).map(|_| Song::new(photo_id)).collect(),
                volume_percent: u16::from(*volume),
                fade_in: Ticks::from_millis(i64::from(*volume) * 10),
                looped: *looped,
                duck: *duck,
                ..Music::default()
            },
            media: vec![],
        },
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn inverses_restore_the_original(photos in 0usize..5, videos in 0usize..3, ops in proptest::collection::vec(op(), 0..25)) {
        let mut p = seed(photos, videos);
        let original = p.clone();
        let mut inverses = Vec::new();
        for op in &ops {
            let Some(cmd) = concrete(op, &p) else { continue };
            let inv = cmd.apply(&mut p).expect("concrete commands are valid");
            prop_assert!(p.validate().is_ok(), "invariant broken after {op:?}: {:?}", p.validate());
            inverses.push(inv);
        }
        for inv in inverses.into_iter().rev() {
            inv.apply(&mut p).unwrap();
        }
        prop_assert_eq!(p, original);
    }

    #[test]
    fn history_undo_all_then_redo_all_is_identity(photos in 1usize..5, ops in proptest::collection::vec(op(), 1..15)) {
        let mut p = seed(photos, 1);
        let original = p.clone();
        let mut h = History::new();
        let mut applied = 0;
        for op in &ops {
            if let Some(cmd) = concrete(op, &p) {
                h.apply(&mut p, cmd).unwrap();
                applied += 1;
            }
        }
        let edited = p.clone();
        for _ in 0..applied {
            prop_assert!(h.undo(&mut p).is_some());
        }
        prop_assert_eq!(&p, &original);
        prop_assert!(!h.is_dirty());
        for _ in 0..applied {
            prop_assert!(h.redo(&mut p).is_some());
        }
        prop_assert_eq!(p, edited);
    }

    #[test]
    fn json_round_trip_after_edits(photos in 0usize..4, ops in proptest::collection::vec(op(), 0..10)) {
        let mut p = seed(photos, 1);
        for op in &ops {
            if let Some(cmd) = concrete(op, &p) {
                cmd.apply(&mut p).unwrap();
            }
        }
        let json = p.to_json().unwrap();
        let back = Project::from_json(&json).unwrap();
        p.prune_media();
        prop_assert_eq!(back, p);
    }
}
