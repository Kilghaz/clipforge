//! Building the ffmpeg command line for an [`EncodePlan`].

use std::path::Path;

use crate::encoders::Encoder;
use crate::plan::{Codec, EncodePlan, PixelFormat};

/// Full argument list (without the program name) for streaming raw
/// `yuv420p` frames on stdin into an encoded file at `output`. `audio` is
/// an optional WAV mixed by [`crate::audio::mix`].
#[must_use]
pub fn build(
    plan: &EncodePlan,
    encoder: &Encoder,
    output: &Path,
    audio: Option<&Path>,
) -> Vec<String> {
    let fps = format!(
        "{}/{}",
        plan.frame_rate.numerator(),
        plan.frame_rate.denominator()
    );
    let mut a: Vec<String> = vec![
        "-hide_banner".into(),
        "-nostdin".into(),
        "-loglevel".into(),
        "error".into(),
        "-nostats".into(),
        "-y".into(),
        // Input: raw frames on stdin.
        "-f".into(),
        "rawvideo".into(),
        "-pix_fmt".into(),
        pix_fmt(plan.pixel_format).into(),
        "-s".into(),
        format!("{}x{}", plan.width, plan.height),
        "-r".into(),
        fps.clone(),
        "-color_range".into(),
        "tv".into(),
        "-colorspace".into(),
        plan.color.matrix.into(),
        "-color_primaries".into(),
        plan.color.primaries.into(),
        "-color_trc".into(),
        plan.color.transfer.into(),
        "-i".into(),
        "pipe:0".into(),
    ];
    match audio {
        Some(wav) => a.extend([
            "-i".into(),
            wav.to_string_lossy().into_owned(),
            "-map".into(),
            "0:v:0".into(),
            "-map".into(),
            "1:a:0".into(),
            "-c:a".into(),
            "aac".into(),
            "-b:a".into(),
            format!("{}k", plan.audio_bitrate_kbps),
            "-ar".into(),
            "48000".into(),
            "-shortest".into(),
        ]),
        None => a.push("-an".into()),
    }
    a.extend([
        // Video encoder.
        "-c:v".into(),
        encoder.name.into(),
        "-pix_fmt".into(),
        pix_fmt(plan.pixel_format).into(),
        "-b:v".into(),
        format!("{}k", plan.video_bitrate_kbps),
        "-maxrate".into(),
        format!("{}k", plan.video_bitrate_kbps * 3 / 2),
        "-bufsize".into(),
        format!("{}k", plan.video_bitrate_kbps * 2),
        "-g".into(),
        gop_frames(plan).to_string(),
        "-r".into(),
        fps,
        "-color_range".into(),
        "tv".into(),
        "-colorspace".into(),
        plan.color.matrix.into(),
        "-color_primaries".into(),
        plan.color.primaries.into(),
        "-color_trc".into(),
        plan.color.transfer.into(),
    ]);
    a.extend(encoder_specific(plan, encoder));
    if plan.closed_gop {
        // At most two consecutive B-frames, closed GOPs (YouTube).
        a.extend(["-bf".into(), "2".into(), "-flags".into(), "+cgop".into()]);
        if encoder.name == "libx265" {
            // x265 ignores +cgop; its own switch.
            if let Some(i) = a.iter().position(|s| s == "-x265-params") {
                a[i + 1].push_str(":open-gop=0");
            }
        }
    }
    if plan.faststart {
        a.extend(["-movflags".into(), "+faststart".into()]);
    }
    if plan.codec == Codec::Hevc {
        // Tag as hvc1 so QuickTime and iOS play it.
        a.extend(["-tag:v".into(), "hvc1".into()]);
    }
    a.extend(["-progress".into(), "pipe:1".into()]);
    a.push(output.to_string_lossy().into_owned());
    a
}

/// The frames on stdin are converted by [`crate::yuv::Yuv420`] in the
/// plan's own format, so input and output formats match.
fn pix_fmt(format: PixelFormat) -> &'static str {
    match format {
        PixelFormat::Yuv420p => "yuv420p",
        PixelFormat::Yuv420p10le => "yuv420p10le",
    }
}

fn gop_frames(plan: &EncodePlan) -> u32 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let g = (plan.frame_rate.as_f64() * f64::from(plan.gop_seconds)).round() as u32;
    g.max(1)
}

fn encoder_specific(plan: &EncodePlan, encoder: &Encoder) -> Vec<String> {
    match encoder.name {
        "libx264" => vec![
            "-preset".into(),
            "medium".into(),
            "-profile:v".into(),
            "high".into(),
        ],
        "libx265" => {
            let mut v = vec!["-preset".into(), "medium".into()];
            if plan.pixel_format == PixelFormat::Yuv420p10le {
                v.extend(["-profile:v".into(), "main10".into()]);
            }
            v.extend(["-x265-params".into(), "log-level=error".into()]);
            v
        }
        "h264_videotoolbox" => vec![
            "-profile:v".into(),
            "high".into(),
            "-allow_sw".into(),
            "1".into(),
        ],
        "hevc_videotoolbox" => {
            let mut v = vec!["-allow_sw".into(), "1".into()];
            if plan.pixel_format == PixelFormat::Yuv420p10le {
                v.extend(["-profile:v".into(), "main10".into()]);
            }
            v
        }
        "h264_nvenc" | "hevc_nvenc" => {
            vec!["-preset".into(), "p5".into(), "-rc".into(), "vbr".into()]
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::ExportOptions;
    use clipforge_core::{Aspect, FrameRate, Resolution};

    fn plan(hdr: bool) -> EncodePlan {
        let opts = ExportOptions {
            resolution: Resolution::FullHd,
            hdr,
            ..ExportOptions::default()
        };
        EncodePlan::build(&opts, Aspect::Landscape16x9, FrameRate::FPS_30)
    }

    fn has_pair(args: &[String], k: &str, v: &str) -> bool {
        args.windows(2).any(|w| w[0] == k && w[1] == v)
    }

    #[test]
    fn sdr_h264_command_line() {
        let a = build(
            &plan(false),
            &Encoder {
                name: "libx264",
                hardware: false,
            },
            Path::new("/out/movie.mp4"),
            None,
        );
        assert_eq!(a.last().unwrap(), "/out/movie.mp4");
        assert!(has_pair(&a, "-f", "rawvideo"));
        assert!(has_pair(&a, "-s", "1920x1080"));
        assert!(has_pair(&a, "-r", "30/1"));
        assert!(has_pair(&a, "-i", "pipe:0"));
        assert!(has_pair(&a, "-pix_fmt", "yuv420p"));
        assert!(has_pair(&a, "-c:v", "libx264"));
        assert!(has_pair(&a, "-g", "60"));
        assert!(has_pair(&a, "-color_trc", "bt709"));
        assert!(has_pair(&a, "-movflags", "+faststart"));
        assert!(has_pair(&a, "-progress", "pipe:1"));
        assert!(has_pair(&a, "-preset", "medium"));
        assert!(!a.iter().any(|s| s == "-tag:v"));
        assert!(a.contains(&"-an".to_owned()));
    }

    #[test]
    fn hdr_hevc_command_line() {
        let a = build(
            &plan(true),
            &Encoder {
                name: "hevc_videotoolbox",
                hardware: true,
            },
            Path::new("o.mp4"),
            None,
        );
        assert!(has_pair(&a, "-c:v", "hevc_videotoolbox"));
        assert!(has_pair(&a, "-pix_fmt", "yuv420p10le"));
        // The piped input is 10-bit as well.
        let input = a.iter().position(|s| s == "pipe:0").unwrap();
        assert!(has_pair(&a[..input], "-pix_fmt", "yuv420p10le"));
        assert!(has_pair(&a, "-profile:v", "main10"));
        assert!(has_pair(&a, "-color_trc", "arib-std-b67"));
        assert!(has_pair(&a, "-tag:v", "hvc1"));
    }

    #[test]
    fn youtube_closes_gops_on_every_encoder() {
        let mut p = plan(false);
        p.closed_gop = true;
        for name in ["libx264", "h264_videotoolbox", "libx265"] {
            let a = build(
                &p,
                &Encoder {
                    name,
                    hardware: false,
                },
                Path::new("o.mp4"),
                None,
            );
            assert!(has_pair(&a, "-flags", "+cgop"), "{name}");
            assert!(has_pair(&a, "-bf", "2"), "{name}");
        }
        let a = build(
            &p,
            &Encoder {
                name: "libx265",
                hardware: false,
            },
            Path::new("o.mp4"),
            None,
        );
        assert!(has_pair(&a, "-x265-params", "log-level=error:open-gop=0"));
        assert!(
            !build(
                &plan(false),
                &Encoder {
                    name: "libx264",
                    hardware: false
                },
                Path::new("o.mp4"),
                None
            )
            .contains(&"+cgop".to_owned())
        );
    }

    #[test]
    fn audio_input_is_mapped_and_encoded() {
        let a = build(
            &plan(false),
            &Encoder {
                name: "libx264",
                hardware: false,
            },
            Path::new("o.mp4"),
            Some(Path::new("/tmp/mix.wav")),
        );
        assert!(has_pair(&a, "-i", "/tmp/mix.wav"));
        assert!(has_pair(&a, "-map", "0:v:0") && has_pair(&a, "-map", "1:a:0"));
        assert!(has_pair(&a, "-c:a", "aac"));
        assert!(has_pair(&a, "-b:a", "256k"));
        assert!(a.contains(&"-shortest".to_owned()));
        assert!(!a.contains(&"-an".to_owned()));
        let video_in = a.iter().position(|s| s == "pipe:0").unwrap();
        let audio_in = a.iter().position(|s| s == "/tmp/mix.wav").unwrap();
        assert!(video_in < audio_in, "video is input 0");
    }

    #[test]
    fn gop_follows_frame_rate() {
        let opts = ExportOptions::default();
        let p = EncodePlan::build(&opts, Aspect::Landscape16x9, FrameRate::FPS_23_976);
        assert_eq!(gop_frames(&p), 48);
        let p = EncodePlan::build(&opts, Aspect::Landscape16x9, FrameRate::FPS_60);
        assert_eq!(gop_frames(&p), 120);
    }
}
