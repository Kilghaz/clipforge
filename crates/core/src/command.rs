//! Commands: the only way to change a [`Project`]. Each `apply` returns the
//! inverse command, which is what undo replays.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::music::Music;
use crate::project::{
    Caption, Clip, ClipSource, Fit, MediaRef, Motion, Project, ProjectSettings, Quarter,
    TitleBackground, Transition,
};
use crate::time::Ticks;

/// Error for a command that cannot be applied to the current project.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CommandError {
    #[error("clip index {index} out of range (len {len})")]
    IndexOutOfRange { index: usize, len: usize },
    #[error("duplicate index {0}")]
    DuplicateIndex(usize),
    #[error("permutation must contain every index exactly once")]
    BadPermutation,
    #[error("duration must be positive")]
    NonPositiveDuration,
    #[error("trim range is invalid")]
    InvalidTrim,
    #[error("command does not apply to clip {index}: {reason}")]
    NotApplicable { index: usize, reason: &'static str },
    #[error("clip references unknown media")]
    UnknownMedia,
    #[error("music settings are invalid: {0}")]
    InvalidMusic(String),
}

/// Where a command's clips came from, for undo labels in the UI.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CommandLabel {
    Insert,
    Remove,
    Reorder,
    Duration,
    Fit,
    Rotate,
    Transition,
    Trim,
    Mute,
    Motion,
    Music,
    Caption,
    Title,
    Settings,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Command {
    /// Inserts clips so that each ends up at its given index (ascending).
    /// `media` is merged into the project's media table first.
    InsertClips {
        entries: Vec<(usize, Clip)>,
        media: Vec<MediaRef>,
    },
    /// Removes the clips at these indices.
    RemoveClips {
        indices: Vec<usize>,
    },
    /// Reorders all clips: `order[new_index] = old_index`.
    Reorder {
        order: Vec<usize>,
    },
    /// Sets the duration of photo clips and title cards. Video clips in
    /// the selection are rejected.
    SetPhotoDuration {
        indices: Vec<usize>,
        duration: Ticks,
    },
    /// Sets the trim range of a video clip.
    SetTrim {
        index: usize,
        in_point: Ticks,
        out_point: Ticks,
    },
    SetFit {
        indices: Vec<usize>,
        fit: Fit,
    },
    SetRotate {
        indices: Vec<usize>,
        rotate: Quarter,
    },
    SetTransition {
        indices: Vec<usize>,
        transition: Transition,
    },
    SetMuted {
        indices: Vec<usize>,
        muted: bool,
    },
    /// Sets the audio gain in percent (0–300).
    SetVolume {
        indices: Vec<usize>,
        percent: u16,
    },
    /// Sets the same Ken Burns movement on photo clips.
    SetMotion {
        indices: Vec<usize>,
        motion: Motion,
    },
    /// Sets an individual transition per clip (shuffle). One undo step.
    SetTransitionEach {
        entries: Vec<(usize, Transition)>,
    },
    /// Sets an individual movement per photo clip (shuffle). One undo step.
    SetMotionEach {
        entries: Vec<(usize, Motion)>,
    },
    SetSettings {
        settings: ProjectSettings,
    },
    /// Sets (or with `None` removes) the caption of each listed clip. One
    /// undo step for typing, style changes, auto-fill and removal.
    SetCaptions {
        entries: Vec<(usize, Option<Caption>)>,
    },
    /// Sets the background of title cards.
    SetTitleBackground {
        indices: Vec<usize>,
        background: TitleBackground,
    },
    /// Replaces the music track (songs and their settings). `media` is
    /// merged into the project's media table first.
    SetMusic {
        music: Music,
        media: Vec<MediaRef>,
    },
    /// Replays several commands as one undo step.
    Batch {
        commands: Vec<Command>,
    },
    /// Restores per-clip values (inverse of the bulk setters).
    RestoreClips {
        entries: Vec<(usize, Clip)>,
    },
}

impl Command {
    #[must_use]
    pub fn label(&self) -> CommandLabel {
        match self {
            Command::InsertClips { .. } => CommandLabel::Insert,
            Command::RemoveClips { .. } => CommandLabel::Remove,
            Command::Reorder { .. } => CommandLabel::Reorder,
            Command::SetPhotoDuration { .. } => CommandLabel::Duration,
            Command::SetTrim { .. } => CommandLabel::Trim,
            Command::SetFit { .. } => CommandLabel::Fit,
            Command::SetRotate { .. } => CommandLabel::Rotate,
            Command::SetTransition { .. } => CommandLabel::Transition,
            Command::SetMuted { .. } | Command::SetVolume { .. } => CommandLabel::Mute,
            Command::SetMotion { .. } | Command::SetMotionEach { .. } => CommandLabel::Motion,
            Command::SetTransitionEach { .. } => CommandLabel::Transition,
            Command::SetSettings { .. } => CommandLabel::Settings,
            Command::SetMusic { .. } => CommandLabel::Music,
            Command::SetCaptions { .. } => CommandLabel::Caption,
            Command::SetTitleBackground { .. } => CommandLabel::Title,
            Command::Batch { commands } => commands
                .first()
                .map_or(CommandLabel::Settings, Command::label),
            Command::RestoreClips { .. } => CommandLabel::Fit,
        }
    }

    /// Builds a `Reorder` that moves `indices` (any order) so that they sit,
    /// in their current relative order, before the clip currently at `to`
    /// (`to == len` appends). Returns `None` if nothing would change.
    #[must_use]
    pub fn move_clips(len: usize, indices: &[usize], to: usize) -> Option<Command> {
        let moving: BTreeSet<usize> = indices.iter().copied().filter(|i| *i < len).collect();
        if moving.is_empty() || to > len {
            return None;
        }
        let staying: Vec<usize> = (0..len).filter(|i| !moving.contains(i)).collect();
        // Position in `staying` where the moved block goes: number of
        // staying clips originally before `to`.
        let split = staying.iter().filter(|i| **i < to).count();
        let mut order: Vec<usize> = Vec::with_capacity(len);
        order.extend_from_slice(&staying[..split]);
        order.extend(moving.iter().copied());
        order.extend_from_slice(&staying[split..]);
        if order.iter().enumerate().all(|(i, o)| i == *o) {
            None
        } else {
            Some(Command::Reorder { order })
        }
    }

    /// Applies the command, returning its inverse.
    pub fn apply(self, project: &mut Project) -> Result<Command, CommandError> {
        match self {
            Command::InsertClips { entries, media } => {
                let mut sorted = entries;
                sorted.sort_by_key(|(i, _)| *i);
                for m in media {
                    project.media.insert(m.id, m);
                }
                for (_, clip) in &sorted {
                    if !clip.is_title() && !project.media.contains_key(&clip.media) {
                        return Err(CommandError::UnknownMedia);
                    }
                    check_clip(clip)?;
                }
                let mut indices = Vec::with_capacity(sorted.len());
                for (i, clip) in sorted {
                    if i > project.clips.len() {
                        return Err(CommandError::IndexOutOfRange {
                            index: i,
                            len: project.clips.len(),
                        });
                    }
                    project.clips.insert(i, clip);
                    indices.push(i);
                }
                Ok(Command::RemoveClips { indices })
            }
            Command::RemoveClips { indices } => {
                let indices = unique_sorted(&indices, project.clips.len())?;
                let mut removed = Vec::with_capacity(indices.len());
                for &i in indices.iter().rev() {
                    removed.push((i, project.clips.remove(i)));
                }
                removed.reverse();
                Ok(Command::InsertClips {
                    entries: removed,
                    media: Vec::new(),
                })
            }
            Command::Reorder { order } => {
                let len = project.clips.len();
                if order.len() != len {
                    return Err(CommandError::BadPermutation);
                }
                let mut seen = vec![false; len];
                for &o in &order {
                    if o >= len || seen[o] {
                        return Err(CommandError::BadPermutation);
                    }
                    seen[o] = true;
                }
                let old = std::mem::take(&mut project.clips);
                let mut slots: Vec<Option<Clip>> = old.into_iter().map(Some).collect();
                project.clips = order
                    .iter()
                    .map(|&o| slots[o].take().unwrap_or_else(unreachable_clip))
                    .collect();
                let mut inverse = vec![0; len];
                for (new_i, &old_i) in order.iter().enumerate() {
                    inverse[old_i] = new_i;
                }
                Ok(Command::Reorder { order: inverse })
            }
            Command::SetPhotoDuration { indices, duration } => {
                if duration <= Ticks::ZERO {
                    return Err(CommandError::NonPositiveDuration);
                }
                let indices = unique_sorted(&indices, project.clips.len())?;
                for &i in &indices {
                    if !project.clips[i].is_still() {
                        return Err(CommandError::NotApplicable {
                            index: i,
                            reason: "not a photo or title",
                        });
                    }
                }
                let before = snapshot(project, &indices);
                for &i in &indices {
                    match &mut project.clips[i].source {
                        ClipSource::Photo { duration: d }
                        | ClipSource::Title { duration: d, .. } => {
                            *d = duration;
                        }
                        ClipSource::Video { .. } => {}
                    }
                }
                Ok(Command::RestoreClips { entries: before })
            }
            Command::SetTrim {
                index,
                in_point,
                out_point,
            } => {
                let len = project.clips.len();
                if index >= len {
                    return Err(CommandError::IndexOutOfRange { index, len });
                }
                if in_point < Ticks::ZERO || out_point <= in_point {
                    return Err(CommandError::InvalidTrim);
                }
                let clip = &project.clips[index];
                if !clip.is_video() {
                    return Err(CommandError::NotApplicable {
                        index,
                        reason: "not a video",
                    });
                }
                if let Some(natural) = project.media.get(&clip.media).and_then(|m| m.duration)
                    && out_point > natural
                {
                    return Err(CommandError::InvalidTrim);
                }
                let before = snapshot(project, &[index]);
                project.clips[index].source = ClipSource::Video {
                    in_point,
                    out_point,
                };
                Ok(Command::RestoreClips { entries: before })
            }
            Command::SetFit { indices, fit } => set_field(project, &indices, |c| c.fit = fit),
            Command::SetRotate { indices, rotate } => {
                set_field(project, &indices, |c| c.rotate = rotate)
            }
            Command::SetTransition {
                indices,
                transition,
            } => {
                if transition.duration < Ticks::ZERO {
                    return Err(CommandError::NonPositiveDuration);
                }
                set_field(project, &indices, |c| c.transition_in = transition)
            }
            Command::SetMuted { indices, muted } => {
                set_field(project, &indices, |c| c.muted = muted)
            }
            Command::SetVolume { indices, percent } => {
                set_field(project, &indices, |c| c.volume_percent = percent.min(300))
            }
            Command::SetMotion { indices, motion } => {
                let indices = unique_sorted(&indices, project.clips.len())?;
                require_photos(project, &indices)?;
                set_field(project, &indices, |c| c.motion = motion)
            }
            Command::SetTransitionEach { entries } => {
                if entries.iter().any(|(_, t)| t.duration < Ticks::ZERO) {
                    return Err(CommandError::NonPositiveDuration);
                }
                set_each(project, &entries, |c, t| c.transition_in = *t)
            }
            Command::SetMotionEach { entries } => {
                let indices: Vec<usize> = entries.iter().map(|(i, _)| *i).collect();
                let indices = unique_sorted(&indices, project.clips.len())?;
                require_photos(project, &indices)?;
                set_each(project, &entries, |c, m| c.motion = *m)
            }
            Command::SetSettings { settings } => {
                let before = std::mem::replace(&mut project.settings, settings);
                Ok(Command::SetSettings { settings: before })
            }
            Command::SetCaptions { entries } => set_each(project, &entries, |c, caption| {
                c.caption.clone_from(caption)
            }),
            Command::SetTitleBackground {
                indices,
                background,
            } => {
                let indices = unique_sorted(&indices, project.clips.len())?;
                for &i in &indices {
                    if !project.clips[i].is_title() {
                        return Err(CommandError::NotApplicable {
                            index: i,
                            reason: "not a title card",
                        });
                    }
                }
                set_field(project, &indices, |c| {
                    if let ClipSource::Title { background: b, .. } = &mut c.source {
                        *b = background;
                    }
                })
            }
            Command::SetMusic { music, media } => {
                for m in media {
                    project.media.insert(m.id, m);
                }
                let before = std::mem::replace(&mut project.music, music);
                if let Err(e) = project.validate_music() {
                    project.music = before;
                    return Err(CommandError::InvalidMusic(e));
                }
                Ok(Command::SetMusic {
                    music: before,
                    media: Vec::new(),
                })
            }
            Command::Batch { commands } => {
                let mut inverses = Vec::with_capacity(commands.len());
                for cmd in commands {
                    match cmd.apply(project) {
                        Ok(inv) => inverses.push(inv),
                        Err(e) => {
                            // Roll back what was applied so far.
                            for inv in inverses.into_iter().rev() {
                                let _ = inv.apply(project);
                            }
                            return Err(e);
                        }
                    }
                }
                inverses.reverse();
                Ok(Command::Batch { commands: inverses })
            }
            Command::RestoreClips { entries } => {
                let len = project.clips.len();
                for (i, _) in &entries {
                    if *i >= len {
                        return Err(CommandError::IndexOutOfRange { index: *i, len });
                    }
                }
                let indices: Vec<usize> = entries.iter().map(|(i, _)| *i).collect();
                let before = snapshot(project, &indices);
                for (i, clip) in entries {
                    project.clips[i] = clip;
                }
                Ok(Command::RestoreClips { entries: before })
            }
        }
    }
}

fn unreachable_clip() -> Clip {
    // Only reachable if the permutation check above is wrong.
    Clip::photo(crate::ids::MediaId::new(), Ticks::SECOND)
}

fn check_clip(clip: &Clip) -> Result<(), CommandError> {
    match clip.source {
        ClipSource::Photo { duration } | ClipSource::Title { duration, .. }
            if duration <= Ticks::ZERO =>
        {
            Err(CommandError::NonPositiveDuration)
        }
        ClipSource::Video {
            in_point,
            out_point,
        } if in_point < Ticks::ZERO || out_point <= in_point => Err(CommandError::InvalidTrim),
        _ => Ok(()),
    }
}

fn unique_sorted(indices: &[usize], len: usize) -> Result<Vec<usize>, CommandError> {
    let mut out: Vec<usize> = indices.to_vec();
    out.sort_unstable();
    for w in out.windows(2) {
        if w[0] == w[1] {
            return Err(CommandError::DuplicateIndex(w[0]));
        }
    }
    if let Some(&max) = out.last()
        && max >= len
    {
        return Err(CommandError::IndexOutOfRange { index: max, len });
    }
    Ok(out)
}

fn snapshot(project: &Project, indices: &[usize]) -> Vec<(usize, Clip)> {
    indices
        .iter()
        .map(|&i| (i, project.clips[i].clone()))
        .collect()
}

fn require_photos(project: &Project, indices: &[usize]) -> Result<(), CommandError> {
    for &i in indices {
        if !project.clips[i].is_photo() {
            return Err(CommandError::NotApplicable {
                index: i,
                reason: "not a photo",
            });
        }
    }
    Ok(())
}

/// Applies a per-clip value; the inverse restores the previous clips.
fn set_each<T>(
    project: &mut Project,
    entries: &[(usize, T)],
    f: impl Fn(&mut Clip, &T),
) -> Result<Command, CommandError> {
    let indices: Vec<usize> = entries.iter().map(|(i, _)| *i).collect();
    let indices = unique_sorted(&indices, project.clips.len())?;
    let before = snapshot(project, &indices);
    for (i, v) in entries {
        f(&mut project.clips[*i], v);
    }
    Ok(Command::RestoreClips { entries: before })
}

fn set_field(
    project: &mut Project,
    indices: &[usize],
    f: impl Fn(&mut Clip),
) -> Result<Command, CommandError> {
    let indices = unique_sorted(indices, project.clips.len())?;
    let before = snapshot(project, &indices);
    for &i in &indices {
        f(&mut project.clips[i]);
    }
    Ok(Command::RestoreClips { entries: before })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::MediaId;
    use crate::project::{RefKind, TransitionKind};
    use std::path::PathBuf;

    fn media_ref(kind: RefKind, duration: Option<Ticks>) -> MediaRef {
        MediaRef {
            id: MediaId::new(),
            kind,
            path: PathBuf::from("/p"),
            fingerprint_hash: 1,
            size: 1,
            pixel_size: Some((100, 50)),
            duration,
            captured_at_ms: None,
            name: "p".into(),
        }
    }

    fn project_with(n: usize) -> Project {
        let mut p = Project::new();
        let m = media_ref(RefKind::Photo, None);
        let entries = (0..n)
            .map(|i| (i, Clip::photo(m.id, Ticks::from_seconds(i as i64 + 1))))
            .collect();
        Command::InsertClips {
            entries,
            media: vec![m],
        }
        .apply(&mut p)
        .unwrap();
        p
    }

    fn durations(p: &Project) -> Vec<i64> {
        p.clips
            .iter()
            .map(|c| c.duration().flicks() / crate::time::FLICKS_PER_SECOND)
            .collect()
    }

    #[test]
    fn insert_then_inverse_restores() {
        let mut p = Project::new();
        let original = p.clone();
        let m = media_ref(RefKind::Photo, None);
        let inv = Command::InsertClips {
            entries: vec![
                (0, Clip::photo(m.id, Ticks::SECOND)),
                (1, Clip::photo(m.id, Ticks::SECOND)),
            ],
            media: vec![m],
        }
        .apply(&mut p)
        .unwrap();
        assert_eq!(p.clips.len(), 2);
        assert_eq!(
            inv,
            Command::RemoveClips {
                indices: vec![0, 1]
            }
        );
        inv.apply(&mut p).unwrap();
        p.prune_media();
        assert_eq!(p, original);
    }

    #[test]
    fn insert_rejects_unknown_media_and_bad_clips() {
        let mut p = Project::new();
        let err = Command::InsertClips {
            entries: vec![(0, Clip::photo(MediaId::new(), Ticks::SECOND))],
            media: vec![],
        }
        .apply(&mut p)
        .unwrap_err();
        assert_eq!(err, CommandError::UnknownMedia);
        let m = media_ref(RefKind::Photo, None);
        let err = Command::InsertClips {
            entries: vec![(0, Clip::photo(m.id, Ticks::ZERO))],
            media: vec![m],
        }
        .apply(&mut p)
        .unwrap_err();
        assert_eq!(err, CommandError::NonPositiveDuration);
        assert!(p.clips.is_empty());
    }

    #[test]
    fn remove_non_contiguous_and_undo() {
        let mut p = project_with(5);
        let before = p.clone();
        let inv = Command::RemoveClips {
            indices: vec![3, 0, 4],
        }
        .apply(&mut p)
        .unwrap();
        assert_eq!(durations(&p), [2, 3]);
        inv.apply(&mut p).unwrap();
        assert_eq!(p, before);
        assert_eq!(
            Command::RemoveClips { indices: vec![9] }
                .apply(&mut p)
                .unwrap_err(),
            CommandError::IndexOutOfRange { index: 9, len: 5 }
        );
        assert_eq!(
            Command::RemoveClips {
                indices: vec![1, 1]
            }
            .apply(&mut p)
            .unwrap_err(),
            CommandError::DuplicateIndex(1)
        );
    }

    #[test]
    fn reorder_and_inverse() {
        let mut p = project_with(4);
        let before = p.clone();
        let inv = Command::Reorder {
            order: vec![3, 1, 0, 2],
        }
        .apply(&mut p)
        .unwrap();
        assert_eq!(durations(&p), [4, 2, 1, 3]);
        inv.apply(&mut p).unwrap();
        assert_eq!(p, before);
        assert_eq!(
            Command::Reorder {
                order: vec![0, 0, 1, 2]
            }
            .apply(&mut p)
            .unwrap_err(),
            CommandError::BadPermutation
        );
        assert_eq!(
            Command::Reorder { order: vec![0] }
                .apply(&mut p)
                .unwrap_err(),
            CommandError::BadPermutation
        );
    }

    #[test]
    fn move_clips_builds_the_right_permutation() {
        // [0 1 2 3 4], move {1,3} before 0 -> [1 3 0 2 4]
        assert_eq!(
            Command::move_clips(5, &[3, 1], 0),
            Some(Command::Reorder {
                order: vec![1, 3, 0, 2, 4]
            })
        );
        // move {0} to end -> [1 2 3 4 0]
        assert_eq!(
            Command::move_clips(5, &[0], 5),
            Some(Command::Reorder {
                order: vec![1, 2, 3, 4, 0]
            })
        );
        // move {1,2} before 3 (i.e. no change)
        assert_eq!(Command::move_clips(5, &[1, 2], 3), None);
        // move {1,2} before 4 -> [0 3 1 2 4]
        assert_eq!(
            Command::move_clips(5, &[1, 2], 4),
            Some(Command::Reorder {
                order: vec![0, 3, 1, 2, 4]
            })
        );
        assert_eq!(Command::move_clips(5, &[], 0), None);
        assert_eq!(Command::move_clips(5, &[1], 6), None);
    }

    #[test]
    fn bulk_duration_is_one_step_and_undoes_exactly() {
        let mut p = project_with(4);
        let before = p.clone();
        let inv = Command::SetPhotoDuration {
            indices: vec![0, 2, 3],
            duration: Ticks::from_seconds(7),
        }
        .apply(&mut p)
        .unwrap();
        assert_eq!(durations(&p), [7, 2, 7, 7]);
        inv.apply(&mut p).unwrap();
        assert_eq!(p, before);
        assert_eq!(
            Command::SetPhotoDuration {
                indices: vec![0],
                duration: Ticks::ZERO
            }
            .apply(&mut p)
            .unwrap_err(),
            CommandError::NonPositiveDuration
        );
    }

    #[test]
    fn video_trim_rules() {
        let mut p = Project::new();
        let m = media_ref(RefKind::Video, Some(Ticks::from_seconds(10)));
        let clip = Clip::video(m.id, Ticks::from_seconds(10));
        Command::InsertClips {
            entries: vec![(0, clip)],
            media: vec![m],
        }
        .apply(&mut p)
        .unwrap();
        let before = p.clone();
        let inv = Command::SetTrim {
            index: 0,
            in_point: Ticks::from_seconds(2),
            out_point: Ticks::from_seconds(5),
        }
        .apply(&mut p)
        .unwrap();
        assert_eq!(p.clips[0].duration(), Ticks::from_seconds(3));
        inv.apply(&mut p).unwrap();
        assert_eq!(p, before);
        assert_eq!(
            Command::SetTrim {
                index: 0,
                in_point: Ticks::from_seconds(2),
                out_point: Ticks::from_seconds(11)
            }
            .apply(&mut p)
            .unwrap_err(),
            CommandError::InvalidTrim
        );
        assert_eq!(
            Command::SetPhotoDuration {
                indices: vec![0],
                duration: Ticks::SECOND
            }
            .apply(&mut p)
            .unwrap_err(),
            CommandError::NotApplicable {
                index: 0,
                reason: "not a photo or title"
            }
        );
    }

    #[test]
    fn setters_and_settings_round_trip() {
        let mut p = project_with(3);
        let before = p.clone();
        let t = Transition {
            kind: TransitionKind::CrossDissolve,
            duration: Ticks::SECOND,
        };
        let inv = Command::Batch {
            commands: vec![
                Command::SetFit {
                    indices: vec![0, 1],
                    fit: Fit::Cover,
                },
                Command::SetRotate {
                    indices: vec![2],
                    rotate: Quarter::Cw90,
                },
                Command::SetTransition {
                    indices: vec![1, 2],
                    transition: t,
                },
                Command::SetMuted {
                    indices: vec![0],
                    muted: true,
                },
                Command::SetVolume {
                    indices: vec![1],
                    percent: 50,
                },
                Command::SetSettings {
                    settings: ProjectSettings {
                        default_photo_duration: Ticks::SECOND,
                        ..ProjectSettings::default()
                    },
                },
            ],
        }
        .apply(&mut p)
        .unwrap();
        assert_eq!(p.clips[0].fit, Fit::Cover);
        assert_eq!(p.clips[2].rotate, Quarter::Cw90);
        assert_eq!(p.clips[1].transition_in, t);
        assert!(p.clips[0].muted);
        assert_eq!(p.clips[0].gain(), 0.0);
        assert_eq!(p.clips[1].volume_percent, 50);
        assert!((p.clips[1].gain() - 0.5).abs() < 1e-6);
        assert_eq!(p.settings.default_photo_duration, Ticks::SECOND);
        inv.apply(&mut p).unwrap();
        assert_eq!(p, before);
    }

    #[test]
    fn motion_commands_apply_to_photos_and_undo() {
        let mut p = project_with(3);
        let before = p.clone();
        let inv = Command::SetMotion {
            indices: vec![0, 2],
            motion: Motion::ZoomIn,
        }
        .apply(&mut p)
        .unwrap();
        assert_eq!(p.clips[0].motion, Motion::ZoomIn);
        assert_eq!(p.clips[1].motion, Motion::None);
        inv.apply(&mut p).unwrap();
        assert_eq!(p, before);
        let inv = Command::SetMotionEach {
            entries: vec![(0, Motion::PanLeft), (1, Motion::PanUp)],
        }
        .apply(&mut p)
        .unwrap();
        assert_eq!(
            (p.clips[0].motion, p.clips[1].motion),
            (Motion::PanLeft, Motion::PanUp)
        );
        inv.apply(&mut p).unwrap();
        assert_eq!(p, before);
    }

    #[test]
    fn motion_is_rejected_for_videos() {
        let mut p = Project::new();
        let m = media_ref(RefKind::Video, Some(Ticks::from_seconds(5)));
        Command::InsertClips {
            entries: vec![(0, Clip::video(m.id, Ticks::from_seconds(5)))],
            media: vec![m],
        }
        .apply(&mut p)
        .unwrap();
        let before = p.clone();
        assert_eq!(
            Command::SetMotion {
                indices: vec![0],
                motion: Motion::ZoomIn
            }
            .apply(&mut p)
            .unwrap_err(),
            CommandError::NotApplicable {
                index: 0,
                reason: "not a photo"
            }
        );
        assert!(
            Command::SetMotionEach {
                entries: vec![(0, Motion::ZoomIn)]
            }
            .apply(&mut p)
            .is_err()
        );
        assert_eq!(p, before);
    }

    #[test]
    fn transition_each_sets_individual_values_in_one_step() {
        let mut p = project_with(3);
        let before = p.clone();
        let a = Transition {
            kind: TransitionKind::WipeLeft,
            duration: Ticks::from_millis(500),
        };
        let b = Transition {
            kind: TransitionKind::Zoom,
            duration: Ticks::from_millis(800),
        };
        let inv = Command::SetTransitionEach {
            entries: vec![(1, a), (2, b)],
        }
        .apply(&mut p)
        .unwrap();
        assert_eq!((p.clips[1].transition_in, p.clips[2].transition_in), (a, b));
        inv.apply(&mut p).unwrap();
        assert_eq!(p, before);
        let bad = Transition {
            kind: TransitionKind::Zoom,
            duration: Ticks::from_flicks(-1),
        };
        assert!(
            Command::SetTransitionEach {
                entries: vec![(0, bad)]
            }
            .apply(&mut p)
            .is_err()
        );
        assert!(
            Command::SetTransitionEach {
                entries: vec![(0, a), (0, b)]
            }
            .apply(&mut p)
            .is_err(),
            "duplicate index"
        );
    }

    #[test]
    fn batch_rolls_back_on_failure() {
        let mut p = project_with(3);
        let before = p.clone();
        let err = Command::Batch {
            commands: vec![
                Command::SetFit {
                    indices: vec![0],
                    fit: Fit::Cover,
                },
                Command::RemoveClips { indices: vec![42] },
            ],
        }
        .apply(&mut p)
        .unwrap_err();
        assert!(matches!(err, CommandError::IndexOutOfRange { .. }));
        assert_eq!(p, before);
    }

    #[test]
    fn title_cards_need_no_media_and_take_a_duration() {
        use crate::project::{CaptionStyle, TitleBackground};
        let mut p = project_with(2);
        let title = Clip::title("Summer", TitleBackground::Blue, Ticks::from_seconds(3));
        Command::InsertClips {
            entries: vec![(0, title)],
            media: vec![],
        }
        .apply(&mut p)
        .unwrap();
        assert!(p.validate().is_ok());
        assert!(p.clips[0].is_title() && p.clips[0].is_still());
        assert_eq!(
            p.clips[0].caption.as_ref().map(|c| c.style),
            Some(CaptionStyle::Headline)
        );
        Command::SetPhotoDuration {
            indices: vec![0, 1],
            duration: Ticks::from_seconds(5),
        }
        .apply(&mut p)
        .unwrap();
        assert_eq!(durations(&p), vec![5, 5, 2]);
        assert!(matches!(
            p.clips[0].source,
            ClipSource::Title {
                background: TitleBackground::Blue,
                ..
            }
        ));
        // Background: titles only, undoable.
        let inv = Command::SetTitleBackground {
            indices: vec![0],
            background: TitleBackground::White,
        }
        .apply(&mut p)
        .unwrap();
        assert!(matches!(
            p.clips[0].source,
            ClipSource::Title {
                background: TitleBackground::White,
                ..
            }
        ));
        inv.apply(&mut p).unwrap();
        assert!(matches!(
            p.clips[0].source,
            ClipSource::Title {
                background: TitleBackground::Blue,
                ..
            }
        ));
        assert!(
            Command::SetTitleBackground {
                indices: vec![1],
                background: TitleBackground::Red,
            }
            .apply(&mut p)
            .is_err()
        );
        // Motion stays photo-only.
        assert!(
            Command::SetMotion {
                indices: vec![0],
                motion: Motion::ZoomIn,
            }
            .apply(&mut p)
            .is_err()
        );
    }

    #[test]
    fn captions_are_set_per_clip_and_undo_in_one_step() {
        use crate::project::{Caption, CaptionStyle};
        let mut p = project_with(3);
        let original = p.clone();
        let inv = Command::SetCaptions {
            entries: vec![
                (0, Some(Caption::new("Rome", CaptionStyle::Classic))),
                (2, Some(Caption::new("Paris", CaptionStyle::Banner))),
            ],
        }
        .apply(&mut p)
        .unwrap();
        assert_eq!(p.clips[0].caption.as_ref().unwrap().text, "Rome");
        assert!(p.clips[1].caption.is_none());
        assert_eq!(
            p.clips[2].caption.as_ref().unwrap().style,
            CaptionStyle::Banner
        );
        let remove = Command::SetCaptions {
            entries: vec![(0, None)],
        }
        .apply(&mut p)
        .unwrap();
        assert!(p.clips[0].caption.is_none());
        remove.apply(&mut p).unwrap();
        inv.apply(&mut p).unwrap();
        assert_eq!(p, original);
        assert!(
            Command::SetCaptions {
                entries: vec![(9, None)]
            }
            .apply(&mut p)
            .is_err()
        );
    }

    #[test]
    fn titles_and_captions_survive_a_save() {
        use crate::project::{Caption, CaptionStyle, TitleBackground};
        let mut p = project_with(1);
        Command::InsertClips {
            entries: vec![(
                1,
                Clip::title("The end", TitleBackground::Red, Ticks::SECOND),
            )],
            media: vec![],
        }
        .apply(&mut p)
        .unwrap();
        Command::SetCaptions {
            entries: vec![(
                0,
                Some(Caption::new("Line one\nLine two", CaptionStyle::Corner)),
            )],
        }
        .apply(&mut p)
        .unwrap();
        let back = Project::from_json(&p.to_json().unwrap()).unwrap();
        assert_eq!(back, p);
    }
}
