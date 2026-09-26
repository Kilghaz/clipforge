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
        output_pix_fmt(encoder, plan.pixel_format).into(),
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

/// What the encoder is fed: hardware encoders take the semi-planar
/// layouts their drivers use (NV12 / P010), software ones planar YUV.
#[must_use]
pub fn output_pix_fmt(encoder: &Encoder, format: PixelFormat) -> &'static str {
    match (encoder.hardware, format) {
        (true, PixelFormat::Yuv420p) => "nv12",
        (true, PixelFormat::Yuv420p10le) => "p010le",
        (false, f) => pix_fmt(f),
    }
}

/// A test encode for `encoder`: three frames of a generated picture into
/// the null muxer, with the options a real export would use. Hardware
/// encoders listed without a matching GPU fail here within a second.
#[must_use]
pub fn probe(encoder: &Encoder, format: PixelFormat) -> Vec<String> {
    let plan = EncodePlan {
        width: 256,
        height: 144,
        pixel_format: format,
        video_bitrate_kbps: 1_000,
        ..EncodePlan::build(
            &crate::options::ExportOptions {
                hdr: format == PixelFormat::Yuv420p10le,
                ..crate::options::ExportOptions::default()
            },
            clipforge_core::Aspect::Landscape16x9,
            clipforge_core::FrameRate::FPS_30,
        )
    };
    let mut a: Vec<String> = [
        "-hide_banner",
        "-nostdin",
        "-loglevel",
        "error",
        "-f",
        "lavfi",
        "-i",
        "color=c=gray:s=256x144:r=30:d=0.1",
        "-frames:v",
        "3",
        "-c:v",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    a.push(encoder.name.into());
    a.extend(["-pix_fmt".into(), output_pix_fmt(encoder, format).into()]);
    a.extend(["-b:v".into(), format!("{}k", plan.video_bitrate_kbps)]);
    a.extend(encoder_specific(&plan, encoder));
    a.extend(["-f".into(), "null".into(), "-".into()]);
    a
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
            let mut v: Vec<String> = ["-preset", "p5", "-tune", "hq", "-rc", "vbr"]
                .map(String::from)
                .to_vec();
            v.extend(profile(plan, encoder));
            v
        }
        "h264_qsv" | "hevc_qsv" => {
            let mut v = vec!["-preset".into(), "medium".into()];
            v.extend(profile(plan, encoder));
            v
        }
        "h264_amf" | "hevc_amf" => {
            let mut v: Vec<String> = ["-quality", "balanced", "-rc", "vbr_peak"]
                .map(String::from)
                .to_vec();
            v.extend(profile(plan, encoder));
            v
        }
        // Media Foundation picks Main10 from the P010 input itself.
        "h264_mf" | "hevc_mf" => vec!["-rate_control".into(), "pc_vbr".into()],
        _ => Vec::new(),
    }
}

/// `-profile:v` for the GPU encoders: High for H.264, Main10 for 10-bit HEVC.
fn profile(plan: &EncodePlan, encoder: &Encoder) -> Vec<String> {
    if encoder.name.starts_with("h264") {
        vec!["-profile:v".into(), "high".into()]
    } else if plan.pixel_format == PixelFormat::Yuv420p10le {
        vec!["-profile:v".into(), "main10".into()]
    } else {
        vec!["-profile:v".into(), "main".into()]
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
    fn gpu_encoders_get_nv12_or_p010_and_their_own_options() {
        let hw = |name| Encoder {
            name,
            hardware: true,
        };
        let sdr = build(&plan(false), &hw("h264_nvenc"), Path::new("o.mp4"), None);
        assert!(has_pair(&sdr, "-pix_fmt", "nv12"));
        assert!(has_pair(&sdr, "-rc", "vbr") && has_pair(&sdr, "-profile:v", "high"));
        // The piped input stays planar: ffmpeg converts for the encoder.
        let input = sdr.iter().position(|s| s == "pipe:0").unwrap();
        assert!(has_pair(&sdr[..input], "-pix_fmt", "yuv420p"));
        for name in ["hevc_nvenc", "hevc_qsv", "hevc_amf"] {
            let a = build(&plan(true), &hw(name), Path::new("o.mp4"), None);
            assert!(has_pair(&a, "-pix_fmt", "p010le"), "{name}");
            assert!(has_pair(&a, "-profile:v", "main10"), "{name}");
        }
        let mf = build(&plan(true), &hw("hevc_mf"), Path::new("o.mp4"), None);
        assert!(has_pair(&mf, "-pix_fmt", "p010le") && has_pair(&mf, "-rate_control", "pc_vbr"));
        assert!(has_pair(
            &build(&plan(false), &hw("h264_amf"), Path::new("o.mp4"), None),
            "-quality",
            "balanced"
        ));
        // Software stays planar.
        let sw = build(
            &plan(true),
            &Encoder {
                name: "libx265",
                hardware: false,
            },
            Path::new("o.mp4"),
            None,
        );
        let out = sw.iter().rposition(|s| s == "-pix_fmt").unwrap();
        assert_eq!(sw[out + 1], "yuv420p10le");
    }

    #[test]
    fn probe_is_a_short_null_encode_with_the_real_options() {
        let a = probe(
            &Encoder {
                name: "hevc_nvenc",
                hardware: true,
            },
            PixelFormat::Yuv420p10le,
        );
        assert!(has_pair(&a, "-f", "lavfi") && has_pair(&a, "-frames:v", "3"));
        assert!(has_pair(&a, "-c:v", "hevc_nvenc"));
        assert!(has_pair(&a, "-pix_fmt", "p010le") && has_pair(&a, "-profile:v", "main10"));
        assert_eq!(a[a.len() - 3..], ["-f", "null", "-"]);
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
