//! Mixing the timeline's audio into one stereo track for export.
//!
//! Each video clip contributes its audio from `in_point`, scaled by the
//! clip gain, with linear fades across transition overlaps so that
//! dissolves also cross-fade the sound. The mix is written as a 32-bit
//! float WAV that ffmpeg reads as a second input.

use std::io::Write;
use std::path::Path;

use clipforge_core::project::ClipSource;
use clipforge_core::timeline::{effective_overlap, placements};
use clipforge_core::{MediaId, Project, Ticks};
use clipforge_media::{AUDIO_CHANNELS, AUDIO_SAMPLE_RATE};

/// Anything that yields interleaved stereo samples for a media item from
/// a source time. Implemented over `AudioReader` for real files and by
/// synthetic sources in tests.
pub trait AudioSourceFactory {
    /// Opens a stream at `start`; `None` if the item has no audio.
    fn open(&self, media: MediaId, start: Ticks) -> Option<Box<dyn AudioStream>>;
}

pub trait AudioStream {
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

/// Renders the whole timeline's audio. Returns interleaved stereo samples
/// covering the full duration (silence where nothing plays).
pub fn mix(project: &Project, sources: &dyn AudioSourceFactory) -> Vec<f32> {
    let clips = &project.clips;
    let places = placements(clips);
    let total = places.last().map_or(Ticks::ZERO, |p| p.end);
    let total_samples = samples_of(total);
    let mut out = vec![0.0f32; total_samples * AUDIO_CHANNELS];

    for (i, clip) in clips.iter().enumerate() {
        let ClipSource::Video { in_point, .. } = clip.source else {
            continue;
        };
        let gain = clip.gain();
        if gain <= 0.0 {
            continue;
        }
        let Some(mut stream) = sources.open(clip.media, in_point) else {
            continue;
        };
        let place = places[i];
        let start_s = samples_of(place.start);
        let len_s = samples_of(clip.duration()).min(total_samples.saturating_sub(start_s));
        // Fade in over the transition into this clip, fade out over the
        // transition into the next clip.
        let fade_in = samples_of(effective_overlap(clips, i));
        let fade_out = if i + 1 < clips.len() {
            samples_of(effective_overlap(clips, i + 1))
        } else {
            0
        };

        let mut buf = vec![0.0f32; 4096 * AUDIO_CHANNELS];
        let mut done = 0usize;
        while done < len_s {
            let want = (len_s - done).min(4096) * AUDIO_CHANNELS;
            let n = stream.read(&mut buf[..want]);
            if n == 0 {
                break;
            }
            let frames = n / AUDIO_CHANNELS;
            for f in 0..frames {
                let pos = done + f;
                let mut g = gain;
                if fade_in > 0 && pos < fade_in {
                    g *= pos as f32 / fade_in as f32;
                }
                if fade_out > 0 && pos + fade_out >= len_s {
                    g *= (len_s - pos) as f32 / fade_out as f32;
                }
                let o = (start_s + pos) * AUDIO_CHANNELS;
                for c in 0..AUDIO_CHANNELS {
                    out[o + c] += buf[f * AUDIO_CHANNELS + c] * g;
                }
            }
            done += frames;
        }
    }
    // Soft clip to avoid wrap-around when clips overlap loudly.
    for v in &mut out {
        *v = v.clamp(-1.0, 1.0);
    }
    out
}

/// True if any clip would contribute audio.
#[must_use]
pub fn has_audio(project: &Project) -> bool {
    project
        .clips
        .iter()
        .any(|c| matches!(c.source, ClipSource::Video { .. }) && c.gain() > 0.0)
}

/// Writes interleaved stereo `f32` samples as a WAV file (format 3 = IEEE float).
pub fn write_wav(path: &Path, samples: &[f32]) -> std::io::Result<()> {
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    let data_len = u32::try_from(samples.len() * 4)
        .map_err(|_| std::io::Error::other("audio too long for WAV"))?;
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
    f.write_all(&data_len.to_le_bytes())?;
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
