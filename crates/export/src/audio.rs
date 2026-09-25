//! Mixing the timeline's audio into one stereo stream, for export and for
//! preview playback.
//!
//! Each video clip contributes its audio from `in_point`, scaled by the
//! clip gain, with linear fades across transition overlaps so that
//! dissolves also cross-fade the sound. The music playlist plays under it
//! with the fades and ducking of [`MusicEnvelope`]. [`Mixer`] produces the
//! result incrementally from any start time, so the preview can play it and
//! the export can write it to a WAV file without holding it in memory.

use std::io::Write;
use std::path::Path;

use clipforge_core::music::{MusicEnvelope, song_spans_from};
use clipforge_core::project::ClipSource;
use clipforge_core::timeline::{effective_overlap, placements, total_duration};
use clipforge_core::{MediaId, Project, Ticks};
use clipforge_media::{AUDIO_CHANNELS, AUDIO_SAMPLE_RATE};

/// Anything that yields interleaved stereo samples for a media item from
/// a source time. Implemented over `AudioReader` for real files and by
/// synthetic sources in tests.
pub trait AudioSourceFactory {
    /// Opens a stream at `start`; `None` if the item has no audio.
    fn open(&self, media: MediaId, start: Ticks) -> Option<Box<dyn AudioStream>>;
}

pub trait AudioStream: Send {
    /// Fills `buf` with interleaved stereo samples; returns values written
    /// (multiple of the channel count), zero at end of stream.
    fn read(&mut self, buf: &mut [f32]) -> usize;
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn samples_of(t: Ticks) -> usize {
    (t.as_seconds_f64() * f64::from(AUDIO_SAMPLE_RATE))
        .round()
        .max(0.0) as usize
}

/// Frames (stereo sample pairs) processed per envelope evaluation.
const BLOCK: usize = 256;

/// How a voice's level is shaped.
#[derive(Copy, Clone, Debug)]
enum Shape {
    /// A video clip: constant gain, linear fades at both ends (frames).
    Clip {
        gain: f32,
        fade_in: usize,
        fade_out: usize,
    },
    /// A song: level from the music envelope.
    Music,
}

/// One sound that plays over a range of the timeline.
struct Voice {
    media: MediaId,
    /// Source position at `start`.
    source_start: Ticks,
    /// Timeline frame where the voice starts and its length in frames.
    start: usize,
    len: usize,
    /// Frames of the voice skipped because playback started inside it
    /// (clip fades are relative to the voice's own start).
    skipped: usize,
    shape: Shape,
    stream: Option<Box<dyn AudioStream>>,
    opened: bool,
}

/// Streams the mix of a project from a start time.
pub struct Mixer<'a> {
    sources: &'a dyn AudioSourceFactory,
    /// Voices not yet finished, ordered by start.
    voices: Vec<Voice>,
    envelope: MusicEnvelope,
    /// Next timeline frame to produce, and the end of the show.
    pos: usize,
    end: usize,
    scratch: Vec<f32>,
}

impl std::fmt::Debug for Mixer<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mixer")
            .field("pos", &self.pos)
            .field("end", &self.end)
            .field("voices", &self.voices.len())
            .finish_non_exhaustive()
    }
}

impl<'a> Mixer<'a> {
    /// A mixer producing the project's audio from timeline time `from`.
    #[must_use]
    pub fn new(project: &Project, sources: &'a dyn AudioSourceFactory, from: Ticks) -> Mixer<'a> {
        let clips = &project.clips;
        let places = placements(clips);
        let show_end = total_duration(clips);
        let from = from.max(Ticks::ZERO);
        let pos = samples_of(from);
        let mut voices = Vec::new();

        for (i, clip) in clips.iter().enumerate() {
            let ClipSource::Video { in_point, .. } = clip.source else {
                continue;
            };
            let gain = clip.gain();
            if gain <= 0.0 {
                continue;
            }
            let start = samples_of(places[i].start);
            let len = samples_of(clip.duration());
            if start + len <= pos {
                continue;
            }
            let fade_in = samples_of(effective_overlap(clips, i));
            let fade_out = if i + 1 < clips.len() {
                samples_of(effective_overlap(clips, i + 1))
            } else {
                0
            };
            let skipped = pos.saturating_sub(start);
            voices.push(Voice {
                media: clip.media,
                source_start: in_point + Ticks::from_samples(skipped as i64, AUDIO_SAMPLE_RATE),
                start: start + skipped,
                len: len - skipped,
                skipped,
                shape: Shape::Clip {
                    gain,
                    fade_in,
                    fade_out,
                },
                stream: None,
                opened: false,
            });
        }
        for span in song_spans_from(project, show_end, from) {
            voices.push(Voice {
                media: span.media,
                source_start: span.source_start,
                start: samples_of(span.start),
                len: samples_of(span.length),
                skipped: 0,
                shape: Shape::Music,
                stream: None,
                opened: false,
            });
        }
        voices.sort_by_key(|v| v.start);
        Mixer {
            sources,
            voices,
            envelope: MusicEnvelope::new(project, show_end),
            pos,
            end: samples_of(show_end),
            scratch: vec![0.0; BLOCK * AUDIO_CHANNELS],
        }
    }

    /// Frames left until the end of the show.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.end.saturating_sub(self.pos)
    }

    /// Fills `out` (interleaved stereo) with the next samples; returns the
    /// number of values written, zero at the end of the show.
    pub fn fill(&mut self, out: &mut [f32]) -> usize {
        let frames = (out.len() / AUDIO_CHANNELS).min(self.remaining());
        let mut done = 0;
        while done < frames {
            let n = (frames - done).min(BLOCK);
            let block = &mut out[done * AUDIO_CHANNELS..(done + n) * AUDIO_CHANNELS];
            self.mix_block(block, n);
            done += n;
        }
        frames * AUDIO_CHANNELS
    }

    #[allow(clippy::cast_precision_loss)]
    fn mix_block(&mut self, out: &mut [f32], n: usize) {
        out.fill(0.0);
        let block_start = self.pos;
        let block_end = block_start + n;
        let rate = f64::from(AUDIO_SAMPLE_RATE);
        let g0 = self.envelope.gain_at(block_start as f64 / rate);
        let g1 = self.envelope.gain_at(block_end as f64 / rate);
        for v in &mut self.voices {
            if v.start >= block_end {
                break;
            }
            let v_end = v.start + v.len;
            if v_end <= block_start {
                continue;
            }
            if !v.opened {
                v.opened = true;
                v.stream = self.sources.open(v.media, v.source_start);
            }
            let Some(stream) = v.stream.as_mut() else {
                continue;
            };
            // Part of this block the voice covers.
            let from = v.start.max(block_start);
            let to = v_end.min(block_end);
            let want = (to - from) * AUDIO_CHANNELS;
            let buf = &mut self.scratch[..want];
            let mut got = 0;
            while got < want {
                let r = stream.read(&mut buf[got..]);
                if r == 0 {
                    break;
                }
                got += r;
            }
            buf[got..].fill(0.0);
            for f in 0..(to - from) {
                let frame = from + f;
                let g = match v.shape {
                    Shape::Clip {
                        gain,
                        fade_in,
                        fade_out,
                    } => {
                        // Position within the whole clip.
                        let pos = frame - v.start + v.skipped;
                        let len = v.len + v.skipped;
                        let mut g = gain;
                        if fade_in > 0 && pos < fade_in {
                            g *= pos as f32 / fade_in as f32;
                        }
                        if fade_out > 0 && pos + fade_out >= len {
                            g *= (len - pos) as f32 / fade_out as f32;
                        }
                        g
                    }
                    Shape::Music => {
                        let t = (frame - block_start) as f32 / n as f32;
                        g0 + (g1 - g0) * t
                    }
                };
                let o = (frame - block_start) * AUDIO_CHANNELS;
                for c in 0..AUDIO_CHANNELS {
                    out[o + c] += buf[f * AUDIO_CHANNELS + c] * g;
                }
            }
        }
        // Clamp to avoid wrap-around when sounds overlap loudly.
        for s in out.iter_mut() {
            *s = s.clamp(-1.0, 1.0);
        }
        self.pos = block_end;
        // Drop finished voices (and their decoder processes).
        self.voices.retain(|v| v.start + v.len > block_end);
    }
}

/// Renders the whole timeline's audio into memory. Returns interleaved
/// stereo samples covering the full duration (silence where nothing
/// plays). For tests and short shows; export streams with [`write_mix`].
pub fn mix(project: &Project, sources: &dyn AudioSourceFactory) -> Vec<f32> {
    let mut mixer = Mixer::new(project, sources, Ticks::ZERO);
    let mut out = vec![0.0f32; mixer.remaining() * AUDIO_CHANNELS];
    let mut done = 0;
    while done < out.len() {
        let n = mixer.fill(&mut out[done..]);
        if n == 0 {
            break;
        }
        done += n;
    }
    out
}

/// True if any clip or song would contribute audio.
#[must_use]
pub fn has_audio(project: &Project) -> bool {
    let clips = project
        .clips
        .iter()
        .any(|c| matches!(c.source, ClipSource::Video { .. }) && c.gain() > 0.0);
    let music = project.music.gain() > 0.0
        && !song_spans_from(project, total_duration(&project.clips), Ticks::ZERO).is_empty();
    clips || music
}

/// Mixes the project straight into a WAV file. `keep_going` is polled
/// between chunks; returning `false` stops early with an error.
pub fn write_mix(
    path: &Path,
    project: &Project,
    sources: &dyn AudioSourceFactory,
    keep_going: &dyn Fn() -> bool,
) -> std::io::Result<()> {
    let mut mixer = Mixer::new(project, sources, Ticks::ZERO);
    let total = mixer.remaining() * AUDIO_CHANNELS;
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    write_wav_header(&mut f, total)?;
    let mut buf = vec![0.0f32; 8192 * AUDIO_CHANNELS];
    let mut bytes = Vec::with_capacity(buf.len() * 4);
    loop {
        if !keep_going() {
            return Err(std::io::Error::other("cancelled"));
        }
        let n = mixer.fill(&mut buf);
        if n == 0 {
            break;
        }
        bytes.clear();
        for s in &buf[..n] {
            bytes.extend_from_slice(&s.to_le_bytes());
        }
        f.write_all(&bytes)?;
    }
    f.flush()
}

fn write_wav_header(f: &mut impl Write, samples: usize) -> std::io::Result<()> {
    let data_len =
        u32::try_from(samples * 4).map_err(|_| std::io::Error::other("audio too long for WAV"))?;
    let channels = u16::try_from(AUDIO_CHANNELS).unwrap_or(2);
    let block_align = channels * 4;
    let byte_rate = AUDIO_SAMPLE_RATE * u32::from(block_align);
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data_len).to_le_bytes())?;
    f.write_all(b"WAVEfmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&3u16.to_le_bytes())?; // IEEE float
    f.write_all(&channels.to_le_bytes())?;
    f.write_all(&AUDIO_SAMPLE_RATE.to_le_bytes())?;
    f.write_all(&byte_rate.to_le_bytes())?;
    f.write_all(&block_align.to_le_bytes())?;
    f.write_all(&32u16.to_le_bytes())?;
    f.write_all(b"data")?;
    f.write_all(&data_len.to_le_bytes())
}

/// Writes interleaved stereo `f32` samples as a WAV file (format 3 = IEEE float).
pub fn write_wav(path: &Path, samples: &[f32]) -> std::io::Result<()> {
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    write_wav_header(&mut f, samples.len())?;
    let mut bytes = Vec::with_capacity(samples.len() * 4);
    for s in samples {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    f.write_all(&bytes)?;
    f.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipforge_core::project::{MediaRef, RefKind, Transition, TransitionKind};
    use clipforge_core::{Clip, Command};

    /// Constant-value source: every sample equals `level`.
    struct Constant(f32);
    impl AudioStream for Constant {
        fn read(&mut self, buf: &mut [f32]) -> usize {
            buf.fill(self.0);
            buf.len()
        }
    }
    struct Factory;
    impl AudioSourceFactory for Factory {
        fn open(&self, _: MediaId, _: Ticks) -> Option<Box<dyn AudioStream>> {
            Some(Box::new(Constant(0.5)))
        }
    }

    fn video_project(secs: &[i64], dissolve_ms: i64) -> Project {
        let mut p = Project::new();
        let mut entries = Vec::new();
        let mut media = Vec::new();
        for (i, s) in secs.iter().enumerate() {
            let id = MediaId::new();
            media.push(MediaRef {
                id,
                kind: RefKind::Video,
                path: "/v".into(),
                fingerprint_hash: 1,
                size: 1,
                pixel_size: Some((16, 9)),
                duration: Some(Ticks::from_seconds(*s)),
                captured_at_ms: None,
                name: "v".into(),
            });
            let mut c = Clip::video(id, Ticks::from_seconds(*s));
            if i > 0 && dissolve_ms > 0 {
                c.transition_in = Transition {
                    kind: TransitionKind::CrossDissolve,
                    duration: Ticks::from_millis(dissolve_ms),
                };
            }
            entries.push((i, c));
        }
        Command::InsertClips { entries, media }
            .apply(&mut p)
            .unwrap();
        p
    }

    fn at(out: &[f32], t_ms: usize) -> f32 {
        out[(t_ms * AUDIO_SAMPLE_RATE as usize / 1000) * AUDIO_CHANNELS]
    }

    #[test]
    fn single_clip_is_copied_with_gain() {
        let mut p = video_project(&[2], 0);
        Command::SetVolume {
            indices: vec![0],
            percent: 50,
        }
        .apply(&mut p)
        .unwrap();
        let out = mix(&p, &Factory);
        assert_eq!(out.len(), 2 * AUDIO_SAMPLE_RATE as usize * AUDIO_CHANNELS);
        assert!((at(&out, 500) - 0.25).abs() < 1e-6);
        assert!((at(&out, 1_999) - 0.25).abs() < 1e-6);
        assert!(has_audio(&p));
        Command::SetMuted {
            indices: vec![0],
            muted: true,
        }
        .apply(&mut p)
        .unwrap();
        assert!(!has_audio(&p));
        assert!(mix(&p, &Factory).iter().all(|v| *v == 0.0));
    }

    #[test]
    fn dissolve_crossfades_and_sums_to_the_same_level() {
        let p = video_project(&[2, 2], 1000);
        let out = mix(&p, &Factory);
        // total = 3 s; overlap 1.0..2.0 s.
        assert_eq!(out.len(), 3 * AUDIO_SAMPLE_RATE as usize * AUDIO_CHANNELS);
        assert!(
            (at(&out, 500) - 0.5).abs() < 1e-3,
            "before overlap: clip 1 only"
        );
        assert!(
            (at(&out, 1_500) - 0.5).abs() < 2e-2,
            "mid overlap: 0.25 + 0.25"
        );
        assert!(
            (at(&out, 2_500) - 0.5).abs() < 1e-3,
            "after overlap: clip 2 only"
        );
        assert!(at(&out, 1_005) > 0.49 && at(&out, 1_005) < 0.51);
    }

    /// A stream whose samples encode the source time (seconds / 100), so
    /// tests can check where a stream was opened.
    struct Clock {
        next: usize,
    }
    impl AudioStream for Clock {
        fn read(&mut self, buf: &mut [f32]) -> usize {
            for frame in buf.chunks_mut(AUDIO_CHANNELS) {
                let v = self.next as f32 / AUDIO_SAMPLE_RATE as f32 / 100.0;
                frame.fill(v);
                self.next += 1;
            }
            buf.len() / AUDIO_CHANNELS * AUDIO_CHANNELS
        }
    }
    /// Constant 0.5 for videos, a clock for songs; logs every open.
    struct Mixed {
        songs: Vec<MediaId>,
        opens: std::sync::Mutex<Vec<(MediaId, Ticks)>>,
    }
    impl AudioSourceFactory for Mixed {
        fn open(&self, media: MediaId, start: Ticks) -> Option<Box<dyn AudioStream>> {
            if let Ok(mut o) = self.opens.lock() {
                o.push((media, start));
            }
            if self.songs.contains(&media) {
                Some(Box::new(Clock {
                    next: samples_of(start),
                }))
            } else {
                Some(Box::new(Constant(0.5)))
            }
        }
    }

    fn audio_ref(secs: i64) -> MediaRef {
        MediaRef {
            id: MediaId::new(),
            kind: RefKind::Audio,
            path: "/a".into(),
            fingerprint_hash: 1,
            size: 1,
            pixel_size: None,
            duration: Some(Ticks::from_seconds(secs)),
            captured_at_ms: None,
            name: "a".into(),
        }
    }

    fn with_music(mut p: Project, songs: &[i64]) -> (Project, Mixed) {
        let refs: Vec<MediaRef> = songs.iter().map(|s| audio_ref(*s)).collect();
        let ids: Vec<MediaId> = refs.iter().map(|r| r.id).collect();
        let music = clipforge_core::Music {
            songs: ids.iter().map(|m| clipforge_core::Song::new(*m)).collect(),
            fade_out: Ticks::ZERO,
            ..clipforge_core::Music::default()
        };
        Command::SetMusic { music, media: refs }
            .apply(&mut p)
            .unwrap();
        let f = Mixed {
            songs: ids,
            opens: std::sync::Mutex::new(Vec::new()),
        };
        (p, f)
    }

    #[test]
    fn music_loops_under_the_show_and_ducks_under_video_sound() {
        // A 4 s video, then 2 s of a 1 s song looping, ducking over the video.
        let (mut p, f) = with_music(video_project(&[4], 0), &[1]);
        let photo = MediaRef {
            kind: RefKind::Photo,
            duration: None,
            ..audio_ref(0)
        };
        Command::InsertClips {
            entries: vec![(1, Clip::photo(photo.id, Ticks::from_seconds(4)))],
            media: vec![photo],
        }
        .apply(&mut p)
        .unwrap();
        let out = mix(&p, &f);
        assert_eq!(out.len(), 8 * AUDIO_SAMPLE_RATE as usize * AUDIO_CHANNELS);
        let duck = clipforge_core::Music::DUCK_LEVEL;
        // At 2.5 s: video 0.5 + song (0.5 s into the loop -> 0.005) ducked.
        assert!((at(&out, 2_500) - (0.5 + 0.005 * duck)).abs() < 1e-3);
        // At 6.25 s: photo, song 0.25 s into its 7th pass, full level.
        assert!(
            (at(&out, 6_250) - 0.0025).abs() < 1e-4,
            "{}",
            at(&out, 6_250)
        );
        assert!(has_audio(&p));
    }

    #[test]
    fn music_alone_counts_as_audio_and_muted_music_does_not() {
        let mut p = Project::new();
        let photo = MediaRef {
            kind: RefKind::Photo,
            duration: None,
            ..audio_ref(0)
        };
        Command::InsertClips {
            entries: vec![(0, Clip::photo(photo.id, Ticks::from_seconds(2)))],
            media: vec![photo],
        }
        .apply(&mut p)
        .unwrap();
        assert!(!has_audio(&p));
        let (mut p, _) = with_music(p, &[30]);
        assert!(has_audio(&p));
        p.music.volume_percent = 0;
        assert!(!has_audio(&p));
    }

    #[test]
    fn mixing_from_the_middle_matches_the_full_mix() {
        let (p, f) = with_music(video_project(&[2, 3, 2], 700), &[3, 2]);
        let full = mix(&p, &f);
        let from = Ticks::from_millis(2_400);
        let mut m = Mixer::new(&p, &f, from);
        let mut tail = vec![0.0f32; m.remaining() * AUDIO_CHANNELS];
        let mut done = 0;
        while done < tail.len() {
            done += m.fill(&mut tail[done..]);
        }
        let offset = samples_of(from) * AUDIO_CHANNELS;
        assert_eq!(tail.len(), full.len() - offset);
        let worst = tail
            .iter()
            .zip(&full[offset..])
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        // Music gain is interpolated per block, so block edges differ a hair.
        assert!(worst < 1e-3, "worst difference {worst}");
        // Streams start where playback starts, not at their beginning.
        let opens = f.opens.lock().unwrap();
        assert!(opens.iter().any(|(_, t)| *t > Ticks::ZERO));
    }

    #[test]
    fn write_mix_streams_a_valid_wav_and_can_stop() {
        let (p, f) = with_music(video_project(&[2], 0), &[5]);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("m.wav");
        write_mix(&path, &p, &f, &|| true).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes.len(), 44 + 2 * 48_000 * 2 * 4);
        let first = f32::from_le_bytes([
            bytes[44 + 400],
            bytes[45 + 400],
            bytes[46 + 400],
            bytes[47 + 400],
        ]);
        assert!(first > 0.4, "{first}");
        assert!(write_mix(&path, &p, &f, &|| false).is_err());
    }

    #[test]
    fn wav_header_is_valid() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.wav");
        write_wav(&path, &[0.0, 0.5, -0.5, 1.0]).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..16], b"WAVEfmt ");
        assert_eq!(u16::from_le_bytes([bytes[20], bytes[21]]), 3);
        assert_eq!(u16::from_le_bytes([bytes[22], bytes[23]]), 2);
        assert_eq!(
            u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]),
            48_000
        );
        assert_eq!(bytes.len(), 44 + 16);
    }
}
