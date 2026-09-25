//! Background music: a playlist of songs that plays from the start of the
//! show, back to back, optionally looped, and always ends with the show.
//!
//! Everything here is derived from the project: where each song sits on the
//! timeline and how loud the music is at any instant (fades and ducking
//! under video sound). The audio mixer in `export` and the preview both
//! read these functions so they agree.

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ids::MediaId;
use crate::project::{ClipSource, Project};
use crate::time::Ticks;
use crate::timeline::placements;

/// Identity of a song in the playlist. Survives reordering and undo.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SongId(Uuid);

impl SongId {
    #[must_use]
    pub fn new() -> Self {
        SongId(Uuid::new_v4())
    }
}

impl Default for SongId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for SongId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.hyphenated())
    }
}

/// One entry of the playlist.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Song {
    pub id: SongId,
    pub media: MediaId,
}

impl Song {
    #[must_use]
    pub fn new(media: MediaId) -> Song {
        Song {
            id: SongId::new(),
            media,
        }
    }
}

/// The project's music track.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Music {
    #[serde(default)]
    pub songs: Vec<Song>,
    /// Gain in percent (100 = unchanged, 0–200).
    #[serde(default = "default_volume")]
    pub volume_percent: u16,
    /// Fade in at the start of the show.
    #[serde(default)]
    pub fade_in: Ticks,
    /// Fade out before the music ends (the end of the show, or of the
    /// playlist when it is shorter and not looped).
    #[serde(default = "default_fade_out")]
    pub fade_out: Ticks,
    /// Start the playlist again when it ends before the show does.
    #[serde(default = "yes")]
    pub looped: bool,
    /// Lower the music while a video clip plays its own sound.
    #[serde(default = "yes")]
    pub duck: bool,
}

fn default_volume() -> u16 {
    100
}

fn default_fade_out() -> Ticks {
    Music::DEFAULT_FADE_OUT
}

fn yes() -> bool {
    true
}

impl Default for Music {
    fn default() -> Self {
        Music {
            songs: Vec::new(),
            volume_percent: 100,
            fade_in: Ticks::ZERO,
            fade_out: Music::DEFAULT_FADE_OUT,
            looped: true,
            duck: true,
        }
    }
}

impl Music {
    pub const MAX_VOLUME: u16 = 200;
    pub const DEFAULT_FADE_OUT: Ticks = Ticks::from_seconds(3);
    /// Music level while ducked: about -10 dB, so speech in a video clip
    /// stays clear while the music is still audible.
    pub const DUCK_LEVEL: f32 = 0.3;
    /// Time the level takes to go down before and come back after a
    /// video's sound.
    pub const DUCK_RAMP: Ticks = Ticks::from_millis(500);

    #[must_use]
    pub fn gain(&self) -> f32 {
        f32::from(self.volume_percent.min(Self::MAX_VOLUME)) / 100.0
    }
}

/// Where one song plays on the timeline.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct SongSpan {
    /// Index into `Music::songs`.
    pub song: usize,
    pub media: MediaId,
    /// Timeline start.
    pub start: Ticks,
    /// Position in the song file at `start`.
    pub source_start: Ticks,
    pub length: Ticks,
}

impl SongSpan {
    #[must_use]
    pub fn end(self) -> Ticks {
        self.start + self.length
    }
}

/// Natural length of the playlist (one pass). Songs whose duration is
/// unknown count as zero.
#[must_use]
pub fn playlist_length(project: &Project) -> Ticks {
    project
        .music
        .songs
        .iter()
        .map(|s| song_duration(project, s.media))
        .fold(Ticks::ZERO, |a, d| a + d)
}

fn song_duration(project: &Project, media: MediaId) -> Ticks {
    project
        .media_ref(media)
        .and_then(|m| m.duration)
        .unwrap_or(Ticks::ZERO)
        .max(Ticks::ZERO)
}

/// Lays the songs back to back from zero until the show ends at
/// `show_end` (looping when enabled). The last span is cut at `show_end`.
#[must_use]
pub fn song_spans(project: &Project, show_end: Ticks) -> Vec<SongSpan> {
    let music = &project.music;
    let mut out = Vec::new();
    if playlist_length(project) <= Ticks::ZERO {
        return out;
    }
    let mut cursor = Ticks::ZERO;
    'passes: loop {
        for (i, song) in music.songs.iter().enumerate() {
            if cursor >= show_end {
                break 'passes;
            }
            let natural = song_duration(project, song.media);
            if natural <= Ticks::ZERO {
                continue;
            }
            let length = natural.min(show_end - cursor);
            out.push(SongSpan {
                song: i,
                media: song.media,
                start: cursor,
                source_start: Ticks::ZERO,
                length,
            });
            cursor += length;
        }
        if !music.looped {
            break;
        }
    }
    out
}

/// The spans still (partly) ahead of `from`, with the first one starting
/// at `from`. For starting playback in the middle of the show.
#[must_use]
pub fn song_spans_from(project: &Project, show_end: Ticks, from: Ticks) -> Vec<SongSpan> {
    song_spans(project, show_end)
        .into_iter()
        .filter(|s| s.end() > from)
        .map(|s| {
            if s.start >= from {
                s
            } else {
                let skip = from - s.start;
                SongSpan {
                    start: from,
                    source_start: s.source_start + skip,
                    length: s.length - skip,
                    ..s
                }
            }
        })
        .collect()
}

/// Music level over time: volume, fades and ducking. Built once per mix;
/// `gain_at` is cheap enough to call every few samples.
#[derive(Clone, Debug, PartialEq)]
pub struct MusicEnvelope {
    volume: f32,
    /// Seconds.
    fade_in: f64,
    fade_out: f64,
    /// When the music stops (seconds).
    music_end: f64,
    /// Merged intervals (seconds) where a video plays sound; empty when
    /// ducking is off.
    ducked: Vec<(f64, f64)>,
    duck_ramp: f64,
}

impl MusicEnvelope {
    #[must_use]
    pub fn new(project: &Project, show_end: Ticks) -> MusicEnvelope {
        let music = &project.music;
        let music_end = song_spans(project, show_end)
            .last()
            .map_or(Ticks::ZERO, |s| s.end());
        let ducked = if music.duck {
            sound_intervals(project)
        } else {
            Vec::new()
        };
        MusicEnvelope {
            volume: music.gain(),
            fade_in: music.fade_in.max(Ticks::ZERO).as_seconds_f64(),
            fade_out: music.fade_out.max(Ticks::ZERO).as_seconds_f64(),
            music_end: music_end.as_seconds_f64(),
            ducked,
            duck_ramp: Music::DUCK_RAMP.as_seconds_f64(),
        }
    }

    /// Linear gain at timeline time `t` (seconds).
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn gain_at(&self, t: f64) -> f32 {
        if t < 0.0 || t >= self.music_end {
            return 0.0;
        }
        let mut g = f64::from(self.volume);
        if self.fade_in > 0.0 && t < self.fade_in {
            g *= t / self.fade_in;
        }
        let left = self.music_end - t;
        if self.fade_out > 0.0 && left < self.fade_out {
            g *= left / self.fade_out;
        }
        g *= self.duck_factor(t);
        g as f32
    }

    fn duck_factor(&self, t: f64) -> f64 {
        // Closeness 1 inside an interval, falling linearly to 0 over the
        // ramp on either side.
        let mut closeness: f64 = 0.0;
        for &(a, b) in &self.ducked {
            let c = if t >= a && t < b {
                1.0
            } else if t < a {
                1.0 - (a - t) / self.duck_ramp
            } else {
                1.0 - (t - b) / self.duck_ramp
            };
            closeness = closeness.max(c);
            if a > t + self.duck_ramp {
                break;
            }
        }
        let closeness = closeness.clamp(0.0, 1.0);
        1.0 - (1.0 - f64::from(Music::DUCK_LEVEL)) * closeness
    }
}

/// Timeline intervals (seconds, merged, ascending) where a video clip plays
/// sound.
#[must_use]
pub fn sound_intervals(project: &Project) -> Vec<(f64, f64)> {
    let places = placements(&project.clips);
    let mut spans: Vec<(f64, f64)> = project
        .clips
        .iter()
        .zip(&places)
        .filter(|(c, _)| matches!(c.source, ClipSource::Video { .. }) && c.gain() > 0.0)
        .map(|(_, p)| (p.start.as_seconds_f64(), p.end.as_seconds_f64()))
        .collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut merged: Vec<(f64, f64)> = Vec::with_capacity(spans.len());
    for (a, b) in spans {
        match merged.last_mut() {
            Some(last) if a <= last.1 => last.1 = last.1.max(b),
            _ => merged.push((a, b)),
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::project::{Clip, MediaRef, RefKind};

    fn media(kind: RefKind, secs: Option<i64>) -> MediaRef {
        MediaRef {
            id: MediaId::new(),
            kind,
            path: "/m".into(),
            fingerprint_hash: 1,
            size: 1,
            pixel_size: None,
            duration: secs.map(Ticks::from_seconds),
            captured_at_ms: None,
            name: "m".into(),
        }
    }

    /// A show of `photos` four-second photos with songs of the given
    /// lengths.
    fn show(photos: usize, songs: &[i64]) -> Project {
        let mut p = Project::new();
        let photo = media(RefKind::Photo, None);
        Command::InsertClips {
            entries: (0..photos)
                .map(|i| (i, Clip::photo(photo.id, Ticks::from_seconds(4))))
                .collect(),
            media: vec![photo],
        }
        .apply(&mut p)
        .unwrap();
        let refs: Vec<MediaRef> = songs
            .iter()
            .map(|s| media(RefKind::Audio, Some(*s)))
            .collect();
        let music = Music {
            songs: refs.iter().map(|m| Song::new(m.id)).collect(),
            ..Music::default()
        };
        Command::SetMusic { music, media: refs }
            .apply(&mut p)
            .unwrap();
        p
    }

    fn secs(t: Ticks) -> i64 {
        t.flicks() / crate::time::FLICKS_PER_SECOND
    }

    #[test]
    fn songs_play_back_to_back_and_stop_with_the_show() {
        let p = show(5, &[7, 5]); // show 20 s, playlist 12 s, looped
        let spans = song_spans(&p, Ticks::from_seconds(20));
        let summary: Vec<(usize, i64, i64)> = spans
            .iter()
            .map(|s| (s.song, secs(s.start), secs(s.length)))
            .collect();
        assert_eq!(summary, vec![(0, 0, 7), (1, 7, 5), (0, 12, 7), (1, 19, 1)]);
        assert_eq!(playlist_length(&p), Ticks::from_seconds(12));
    }

    #[test]
    fn without_loop_the_playlist_plays_once() {
        let mut p = show(5, &[7, 5]);
        p.music.looped = false;
        let spans = song_spans(&p, Ticks::from_seconds(20));
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[1].end(), Ticks::from_seconds(12));
        // A long song is cut at the end of the show.
        let p = show(1, &[60]);
        let spans = song_spans(&p, Ticks::from_seconds(4));
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].length, Ticks::from_seconds(4));
    }

    #[test]
    fn unknown_durations_are_skipped_and_never_loop_forever() {
        let mut p = show(2, &[]);
        let silent = media(RefKind::Audio, None);
        let mut music = Music::default();
        music.songs.push(Song::new(silent.id));
        Command::SetMusic {
            music,
            media: vec![silent],
        }
        .apply(&mut p)
        .unwrap();
        assert!(song_spans(&p, Ticks::from_seconds(8)).is_empty());
        assert_eq!(playlist_length(&p), Ticks::ZERO);
    }

    #[test]
    fn spans_from_the_middle_start_inside_a_song() {
        let p = show(5, &[7, 5]);
        let spans = song_spans_from(&p, Ticks::from_seconds(20), Ticks::from_seconds(9));
        assert_eq!(spans[0].song, 1);
        assert_eq!(spans[0].start, Ticks::from_seconds(9));
        assert_eq!(spans[0].source_start, Ticks::from_seconds(2));
        assert_eq!(spans[0].length, Ticks::from_seconds(3));
        assert_eq!(spans.len(), 3);
    }

    #[test]
    fn envelope_fades_in_and_out_at_the_music_ends() {
        let mut p = show(5, &[60]); // music plays 0..20 s
        p.music.fade_in = Ticks::from_seconds(2);
        p.music.fade_out = Ticks::from_seconds(4);
        p.music.volume_percent = 50;
        let env = MusicEnvelope::new(&p, Ticks::from_seconds(20));
        assert!(env.gain_at(0.0).abs() < 1e-6);
        assert!((env.gain_at(1.0) - 0.25).abs() < 1e-6);
        assert!((env.gain_at(10.0) - 0.5).abs() < 1e-6);
        assert!((env.gain_at(18.0) - 0.25).abs() < 1e-6);
        assert!(env.gain_at(20.0).abs() < 1e-6);
        assert!(env.gain_at(-1.0).abs() < 1e-6);
        // A playlist shorter than the show, not looped, fades at its own end.
        let mut p = show(5, &[10]);
        p.music.looped = false;
        let env = MusicEnvelope::new(&p, Ticks::from_seconds(20));
        assert!(
            (env.gain_at(8.5) - 0.5).abs() < 1e-6,
            "3 s fade out from 7 s"
        );
        assert!(env.gain_at(12.0).abs() < 1e-6);
    }

    #[test]
    fn music_ducks_under_video_sound_with_ramps() {
        let mut p = show(2, &[60]); // photos 0..8 s
        let video = media(RefKind::Video, Some(4));
        Command::InsertClips {
            entries: vec![(1, Clip::video(video.id, Ticks::from_seconds(4)))],
            media: vec![video],
        }
        .apply(&mut p)
        .unwrap(); // photo 0..4, video 4..8, photo 8..12
        p.music.fade_out = Ticks::ZERO;
        let env = MusicEnvelope::new(&p, Ticks::from_seconds(12));
        let level = Music::DUCK_LEVEL;
        assert!((env.gain_at(2.0) - 1.0).abs() < 1e-6);
        assert!(
            (env.gain_at(3.75) - (1.0 + level) / 2.0).abs() < 1e-3,
            "ramp down"
        );
        assert!((env.gain_at(6.0) - level).abs() < 1e-6);
        assert!(
            (env.gain_at(8.25) - (1.0 + level) / 2.0).abs() < 1e-3,
            "ramp up"
        );
        assert!((env.gain_at(10.0) - 1.0).abs() < 1e-6);
        // Off, or a muted video: no ducking.
        p.music.duck = false;
        let env = MusicEnvelope::new(&p, Ticks::from_seconds(12));
        assert!((env.gain_at(6.0) - 1.0).abs() < 1e-6);
        p.music.duck = true;
        p.clips[1].muted = true;
        let env = MusicEnvelope::new(&p, Ticks::from_seconds(12));
        assert!((env.gain_at(6.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn overlapping_sound_intervals_merge() {
        let mut p = Project::new();
        let v = media(RefKind::Video, Some(4));
        let mut b = Clip::video(v.id, Ticks::from_seconds(4));
        b.transition_in = crate::project::Transition {
            kind: crate::project::TransitionKind::CrossDissolve,
            duration: Ticks::SECOND,
        };
        Command::InsertClips {
            entries: vec![(0, Clip::video(v.id, Ticks::from_seconds(4))), (1, b)],
            media: vec![v],
        }
        .apply(&mut p)
        .unwrap();
        assert_eq!(sound_intervals(&p), vec![(0.0, 7.0)]);
    }

    #[test]
    fn old_project_files_get_default_music() {
        let m: Music = serde_json::from_str("{}").unwrap();
        assert_eq!(m, Music::default());
        assert!(m.looped && m.duck);
    }
}
