//! Checking a finished export: ffprobe the file and compare it with the
//! plan (PLAN.md §3.6 "verifies the output opens and has the expected
//! duration").

use std::path::Path;

use clipforge_core::Ticks;
use clipforge_media::{ColorTransfer, FfmpegCli, MediaInfo, Prober};

use crate::plan::{Codec, EncodePlan};

/// One way the output differs from the plan.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum Mismatch {
    #[error("the file cannot be opened: {0}")]
    Unreadable(String),
    #[error("video codec is {got}, expected {want}")]
    Codec { got: String, want: &'static str },
    #[error("picture is {got_w}x{got_h}, expected {want_w}x{want_h}")]
    Size {
        got_w: u32,
        got_h: u32,
        want_w: u32,
        want_h: u32,
    },
    #[error("video is {got:.2} s long, expected {want:.2} s")]
    Duration { got: f64, want: f64 },
    #[error("colour is tagged {got:?}, expected {want:?}")]
    Transfer {
        got: ColorTransfer,
        want: ColorTransfer,
    },
    #[error("the audio track is missing")]
    NoAudio,
}

/// Compares probed `info` with what `plan` asked for. `frames` is the
/// number of frames sent, `audio` whether a mix was muxed in.
#[must_use]
pub fn compare(plan: &EncodePlan, frames: u64, audio: bool, info: &MediaInfo) -> Vec<Mismatch> {
    let mut out = Vec::new();
    let want_codec = match plan.codec {
        Codec::H264 => "h264",
        Codec::Hevc => "hevc",
    };
    if info.codec != want_codec {
        out.push(Mismatch::Codec {
            got: info.codec.clone(),
            want: want_codec,
        });
    }
    let (got_w, got_h) = (info.width.unwrap_or(0), info.height.unwrap_or(0));
    if (got_w, got_h) != (plan.width, plan.height) {
        out.push(Mismatch::Size {
            got_w,
            got_h,
            want_w: plan.width,
            want_h: plan.height,
        });
    }
    #[allow(clippy::cast_possible_wrap)]
    let want = Ticks::from_frames(frames as i64, plan.frame_rate).as_seconds_f64();
    let got = info.duration.map_or(0.0, Ticks::as_seconds_f64);
    // Containers round to their timescale and AAC adds priming; two frames
    // (or 0.1 s) of slack.
    let slack = (2.0 / plan.frame_rate.as_f64()).max(0.1);
    if (got - want).abs() > slack {
        out.push(Mismatch::Duration { got, want });
    }
    let want_transfer = if plan.color.transfer == "arib-std-b67" {
        ColorTransfer::Hlg
    } else {
        ColorTransfer::Sdr
    };
    if info.transfer != want_transfer {
        out.push(Mismatch::Transfer {
            got: info.transfer,
            want: want_transfer,
        });
    }
    if audio && !info.has_audio {
        out.push(Mismatch::NoAudio);
    }
    out
}

/// Probes `output` and compares it with `plan`. Empty means all good.
#[must_use]
pub fn verify(
    cli: &FfmpegCli,
    plan: &EncodePlan,
    frames: u64,
    audio: bool,
    output: &Path,
) -> Vec<Mismatch> {
    match cli.probe(output) {
        Ok(info) => compare(plan, frames, audio, &info),
        Err(e) => vec![Mismatch::Unreadable(e.to_string())],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::ExportOptions;
    use clipforge_core::{Aspect, FrameRate};
    use clipforge_media::{MediaKind, Rotation};

    fn plan(hdr: bool) -> EncodePlan {
        EncodePlan::build(
            &ExportOptions {
                hdr,
                ..ExportOptions::default()
            },
            Aspect::Landscape16x9,
            FrameRate::FPS_30,
        )
    }

    fn info(codec: &str, seconds: f64, transfer: ColorTransfer) -> MediaInfo {
        MediaInfo {
            kind: MediaKind::Video,
            width: Some(1920),
            height: Some(1080),
            rotation: Rotation::None,
            duration: Some(Ticks::from_seconds_f64(seconds)),
            frame_rate: Some(FrameRate::FPS_30),
            has_audio: true,
            transfer,
            captured_at_ms: None,
            codec: codec.into(),
        }
    }

    #[test]
    fn matching_output_passes() {
        let i = info("h264", 10.02, ColorTransfer::Sdr);
        assert_eq!(compare(&plan(false), 300, true, &i), vec![]);
        let i = info("hevc", 10.0, ColorTransfer::Hlg);
        assert_eq!(compare(&plan(true), 300, true, &i), vec![]);
    }

    #[test]
    fn each_difference_is_reported() {
        let mut i = info("hevc", 8.0, ColorTransfer::Sdr);
        i.width = Some(1280);
        i.has_audio = false;
        let m = compare(&plan(false), 300, true, &i);
        assert!(matches!(m[0], Mismatch::Codec { want: "h264", .. }));
        assert!(matches!(m[1], Mismatch::Size { got_w: 1280, .. }));
        assert!(matches!(m[2], Mismatch::Duration { .. }));
        assert_eq!(m[3], Mismatch::NoAudio);
        assert_eq!(m.len(), 4);
        // HDR plan, SDR tags.
        let m = compare(
            &plan(true),
            300,
            false,
            &info("hevc", 10.0, ColorTransfer::Sdr),
        );
        assert!(matches!(
            m[..],
            [Mismatch::Transfer {
                want: ColorTransfer::Hlg,
                ..
            }]
        ));
    }

    #[test]
    fn unreadable_file_is_a_mismatch() {
        let Some(loc) = clipforge_media::FfmpegLocation::discover() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.mp4");
        std::fs::write(&path, b"not a video").unwrap();
        let m = verify(&FfmpegCli::new(loc), &plan(false), 30, false, &path);
        assert!(matches!(m[..], [Mismatch::Unreadable(_)]), "{m:?}");
    }
}
