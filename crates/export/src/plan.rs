//! Translating export options into concrete encoder settings.

use clipforge_core::{Aspect, FrameRate, Resolution};
use serde::{Deserialize, Serialize};

use crate::options::{Container, ExportOptions, Quality};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Codec {
    H264,
    Hevc,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PixelFormat {
    /// 8-bit 4:2:0.
    Yuv420p,
    /// 10-bit 4:2:0.
    Yuv420p10le,
}

/// Colour signalling written into the output stream.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorTags {
    pub primaries: &'static str,
    pub transfer: &'static str,
    pub matrix: &'static str,
}

/// Fully resolved encoding parameters, independent of which ffmpeg encoder
/// ends up executing them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncodePlan {
    pub width: u32,
    pub height: u32,
    pub frame_rate: FrameRate,
    pub codec: Codec,
    pub pixel_format: PixelFormat,
    pub color: ColorTags,
    pub video_bitrate_kbps: u32,
    pub audio_bitrate_kbps: u32,
    pub faststart: bool,
    /// Keyframe interval in seconds.
    pub gop_seconds: u32,
    /// Every GOP decodable on its own (YouTube's recommendation).
    pub closed_gop: bool,
    pub container_extension: &'static str,
}

impl EncodePlan {
    /// Builds the plan for `options` on a project with `aspect` and `fps`
    /// (the project's frame rate; Advanced may override it).
    #[must_use]
    pub fn build(options: &ExportOptions, aspect: Aspect, fps: FrameRate) -> EncodePlan {
        let advanced = &options.advanced;
        let (width, height) = options.resolution.dimensions(aspect);
        let fps = advanced.frame_rate.unwrap_or(fps);
        // HDR is HEVC Main10 whatever Advanced says; YouTube SDR is H.264
        // High unless Advanced picks HEVC explicitly.
        let codec = if options.hdr {
            Codec::Hevc
        } else {
            advanced.codec.unwrap_or(Codec::H264)
        };
        let pixel_format = if options.hdr {
            PixelFormat::Yuv420p10le
        } else {
            PixelFormat::Yuv420p
        };
        let color = if options.hdr {
            ColorTags {
                primaries: "bt2020",
                transfer: "arib-std-b67",
                matrix: "bt2020nc",
            }
        } else {
            ColorTags {
                primaries: "bt709",
                transfer: "bt709",
                matrix: "bt709",
            }
        };
        let high_fps = fps.as_f64() > 40.0;
        let video_bitrate_kbps = advanced
            .video_bitrate_kbps
            .unwrap_or_else(|| bitrate_kbps(options.resolution, options.quality, codec, high_fps));
        let youtube = options.optimize_for_youtube;
        EncodePlan {
            width,
            height,
            frame_rate: fps,
            codec,
            pixel_format,
            color,
            video_bitrate_kbps,
            audio_bitrate_kbps: advanced.audio_bitrate_kbps.unwrap_or(if youtube {
                384
            } else {
                256
            }),
            faststart: true,
            gop_seconds: 2,
            closed_gop: youtube,
            container_extension: if youtube {
                Container::Mp4.extension()
            } else {
                advanced.container.extension()
            },
        }
    }
}

/// Bitrate ladder in kbit/s, loosely following YouTube's upload guidance.
fn bitrate_kbps(resolution: Resolution, quality: Quality, codec: Codec, high_fps: bool) -> u32 {
    let base = match (resolution, quality) {
        (Resolution::FullHd, Quality::Good) => 8_000,
        (Resolution::FullHd, Quality::Better) => 12_000,
        (Resolution::FullHd, Quality::Best) => 20_000,
        (Resolution::Uhd4k, Quality::Good) => 35_000,
        (Resolution::Uhd4k, Quality::Better) => 50_000,
        (Resolution::Uhd4k, Quality::Best) => 80_000,
    };
    let base = if high_fps { base * 3 / 2 } else { base };
    match codec {
        Codec::H264 => base,
        // HEVC reaches similar quality at roughly two thirds of the bitrate.
        Codec::Hevc => base * 2 / 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::Advanced;

    fn opts(hdr: bool, youtube: bool) -> ExportOptions {
        ExportOptions {
            resolution: Resolution::FullHd,
            quality: Quality::Better,
            hdr,
            optimize_for_youtube: youtube,
            advanced: Advanced::default(),
        }
    }

    fn build(o: &ExportOptions) -> EncodePlan {
        EncodePlan::build(o, Aspect::Landscape16x9, FrameRate::FPS_30)
    }

    #[test]
    fn youtube_is_h264_high_mp4_with_closed_two_second_gops() {
        let plan = build(&opts(false, true));
        assert_eq!(plan.codec, Codec::H264);
        assert!(plan.closed_gop && plan.faststart);
        assert_eq!(plan.gop_seconds, 2);
        assert_eq!(plan.container_extension, "mp4");
        assert!(!build(&opts(false, false)).closed_gop);
        // YouTube HDR stays HEVC HLG.
        let hdr = build(&opts(true, true));
        assert_eq!(hdr.codec, Codec::Hevc);
        assert_eq!(hdr.color.transfer, "arib-std-b67");
    }

    #[test]
    fn advanced_overrides_codec_frame_rate_bitrates_and_container() {
        let mut o = opts(false, false);
        o.advanced = Advanced {
            codec: Some(Codec::Hevc),
            frame_rate: Some(FrameRate::FPS_60),
            video_bitrate_kbps: Some(7_000),
            audio_bitrate_kbps: Some(160),
            container: Container::Mov,
        };
        let plan = build(&o);
        assert_eq!(plan.codec, Codec::Hevc);
        // SDR HEVC stays 8-bit BT.709.
        assert_eq!(plan.pixel_format, PixelFormat::Yuv420p);
        assert_eq!(plan.color.transfer, "bt709");
        assert_eq!(plan.frame_rate, FrameRate::FPS_60);
        assert_eq!(plan.video_bitrate_kbps, 7_000);
        assert_eq!(plan.audio_bitrate_kbps, 160);
        assert_eq!(plan.container_extension, "mov");
    }

    #[test]
    fn hdr_and_youtube_win_over_conflicting_advanced_choices() {
        let mut o = opts(true, true);
        o.advanced.codec = Some(Codec::H264);
        o.advanced.container = Container::Mov;
        let plan = build(&o);
        assert_eq!(plan.codec, Codec::Hevc);
        assert_eq!(plan.container_extension, "mp4");
    }

    #[test]
    fn advanced_frame_rate_drives_the_high_fps_ladder() {
        let mut o = opts(false, false);
        let base = build(&o).video_bitrate_kbps;
        o.advanced.frame_rate = Some(FrameRate::FPS_50);
        assert!(build(&o).video_bitrate_kbps > base);
    }

    #[test]
    fn sdr_defaults_are_h264_8bit_bt709() {
        let plan = EncodePlan::build(
            &opts(false, false),
            Aspect::Landscape16x9,
            FrameRate::FPS_30,
        );
        assert_eq!((plan.width, plan.height), (1920, 1080));
        assert_eq!(plan.codec, Codec::H264);
        assert_eq!(plan.pixel_format, PixelFormat::Yuv420p);
        assert_eq!(plan.color.transfer, "bt709");
        assert_eq!(plan.container_extension, "mp4");
        assert!(plan.faststart);
    }

    #[test]
    fn hdr_means_hevc_10bit_hlg_bt2020() {
        let plan = EncodePlan::build(&opts(true, false), Aspect::Landscape16x9, FrameRate::FPS_30);
        assert_eq!(plan.codec, Codec::Hevc);
        assert_eq!(plan.pixel_format, PixelFormat::Yuv420p10le);
        assert_eq!(plan.color.primaries, "bt2020");
        assert_eq!(plan.color.transfer, "arib-std-b67");
        assert_eq!(plan.color.matrix, "bt2020nc");
    }

    #[test]
    fn youtube_raises_audio_bitrate() {
        assert_eq!(
            EncodePlan::build(&opts(false, true), Aspect::Landscape16x9, FrameRate::FPS_30)
                .audio_bitrate_kbps,
            384
        );
        assert_eq!(
            EncodePlan::build(
                &opts(false, false),
                Aspect::Landscape16x9,
                FrameRate::FPS_30
            )
            .audio_bitrate_kbps,
            256
        );
    }

    #[test]
    fn bitrate_ladder_is_monotonic() {
        for res in [Resolution::FullHd, Resolution::Uhd4k] {
            let mut last = 0;
            for q in [Quality::Good, Quality::Better, Quality::Best] {
                let o = ExportOptions {
                    resolution: res,
                    quality: q,
                    ..ExportOptions::default()
                };
                let b = EncodePlan::build(&o, Aspect::Landscape16x9, FrameRate::FPS_30)
                    .video_bitrate_kbps;
                assert!(b > last, "{res:?} {q:?}: {b} <= {last}");
                last = b;
            }
        }
        let fhd = EncodePlan::build(
            &opts(false, false),
            Aspect::Landscape16x9,
            FrameRate::FPS_30,
        )
        .video_bitrate_kbps;
        let uhd = EncodePlan::build(
            &ExportOptions {
                resolution: Resolution::Uhd4k,
                ..opts(false, false)
            },
            Aspect::Landscape16x9,
            FrameRate::FPS_30,
        )
        .video_bitrate_kbps;
        assert!(uhd > fhd);
    }

    #[test]
    fn high_frame_rate_and_hevc_adjust_bitrate() {
        let base = EncodePlan::build(
            &opts(false, false),
            Aspect::Landscape16x9,
            FrameRate::FPS_30,
        )
        .video_bitrate_kbps;
        let sixty = EncodePlan::build(
            &opts(false, false),
            Aspect::Landscape16x9,
            FrameRate::FPS_60,
        )
        .video_bitrate_kbps;
        let hdr = EncodePlan::build(&opts(true, false), Aspect::Landscape16x9, FrameRate::FPS_30)
            .video_bitrate_kbps;
        assert!(sixty > base);
        assert!(hdr < base);
    }

    #[test]
    fn portrait_swaps_dimensions() {
        let plan = EncodePlan::build(&opts(false, false), Aspect::Portrait9x16, FrameRate::FPS_30);
        assert_eq!((plan.width, plan.height), (1080, 1920));
    }
}
