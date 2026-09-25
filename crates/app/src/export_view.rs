//! Pure view logic for the export dialog (docs/ux/decisions/m6-export.md):
//! dialog state → `ExportOptions`, the HDR availability, the summary line
//! and the time-left estimate. No Slint here; wording lives in `.slint`.

use std::time::Duration;

use clipforge_core::{FrameRate, Project, Resolution};
use clipforge_export::{Advanced, Codec, Container, EncodePlan, ExportOptions, Quality};

/// Frame rates offered under Advanced, after "Project".
pub(crate) const FRAME_RATES: [FrameRate; 5] = [
    FrameRate::FPS_24,
    FrameRate::FPS_25,
    FrameRate::FPS_30,
    FrameRate::FPS_50,
    FrameRate::FPS_60,
];
/// Video bitrates (Mbit/s) offered under Advanced, after "Automatic".
pub(crate) const VIDEO_MBPS: [u32; 7] = [8, 12, 16, 20, 35, 50, 80];
/// Audio bitrates (kbit/s) offered under Advanced, after "Automatic".
pub(crate) const AUDIO_KBPS: [u32; 5] = [128, 192, 256, 320, 384];

/// The dialog's controls as indices, as Slint holds them. Index 0 of every
/// Advanced list is "Automatic" / "Project".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DialogState {
    pub resolution: usize,
    pub quality: usize,
    pub youtube: bool,
    pub hdr: bool,
    pub codec: usize,
    pub frame_rate: usize,
    pub video_bitrate: usize,
    pub audio_bitrate: usize,
    pub container: usize,
}

/// What the dialog's controls ask for.
#[must_use]
pub(crate) fn options(state: &DialogState) -> ExportOptions {
    let pick = |list: &[u32], i: usize| i.checked_sub(1).and_then(|i| list.get(i)).copied();
    ExportOptions {
        resolution: if state.resolution == 1 {
            Resolution::Uhd4k
        } else {
            Resolution::FullHd
        },
        quality: match state.quality {
            0 => Quality::Good,
            2 => Quality::Best,
            _ => Quality::Better,
        },
        hdr: state.hdr,
        optimize_for_youtube: state.youtube,
        advanced: Advanced {
            codec: match state.codec {
                1 => Some(Codec::H264),
                2 => Some(Codec::Hevc),
                _ => None,
            },
            frame_rate: state
                .frame_rate
                .checked_sub(1)
                .and_then(|i| FRAME_RATES.get(i))
                .copied(),
            video_bitrate_kbps: pick(&VIDEO_MBPS, state.video_bitrate).map(|m| m * 1000),
            audio_bitrate_kbps: pick(&AUDIO_KBPS, state.audio_bitrate),
            container: if state.container == 1 {
                Container::Mov
            } else {
                Container::Mp4
            },
        },
    }
}

/// Whether the HDR switch can be turned on, and if not, why.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HdrAvailability {
    Available,
    NoHdrSources,
    NoGpu,
}

impl HdrAvailability {
    /// For Slint: 0 available, 1 no HDR videos, 2 no GPU.
    #[must_use]
    pub(crate) fn code(self) -> i32 {
        match self {
            HdrAvailability::Available => 0,
            HdrAvailability::NoHdrSources => 1,
            HdrAvailability::NoGpu => 2,
        }
    }
}

#[must_use]
pub(crate) fn hdr_availability(project: &Project, gpu: bool) -> HdrAvailability {
    if !project.has_hdr_sources() {
        HdrAvailability::NoHdrSources
    } else if !gpu {
        HdrAvailability::NoGpu
    } else {
        HdrAvailability::Available
    }
}

/// The facts behind the "About 240 MB · H.264 · 12 Mbit/s" line.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Summary {
    pub bytes: u64,
    pub codec: &'static str,
    pub video_mbps: f32,
    pub audio_kbps: u32,
    pub frame_rate: f32,
    pub extension: &'static str,
    /// The automatic values, for the Advanced "Automatic (…)" entries.
    pub auto_video_mbps: f32,
    pub auto_audio_kbps: u32,
}

#[must_use]
pub(crate) fn summary(options: &ExportOptions, project: &Project) -> Summary {
    let settings = &project.settings;
    let plan = EncodePlan::build(options, settings.aspect, settings.frame_rate);
    let auto = EncodePlan::build(
        &ExportOptions {
            advanced: Advanced {
                video_bitrate_kbps: None,
                audio_bitrate_kbps: None,
                ..options.advanced.clone()
            },
            ..options.clone()
        },
        settings.aspect,
        settings.frame_rate,
    );
    let seconds = clipforge_core::timeline::total_duration(&project.clips).as_seconds_f64();
    // The audio track exists only if something makes sound.
    let audio_kbps = if clipforge_export::audio::has_audio(project) {
        plan.audio_bitrate_kbps
    } else {
        0
    };
    let kbps = f64::from(plan.video_bitrate_kbps + audio_kbps);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let bytes = (kbps * 1000.0 / 8.0 * seconds.max(0.0)) as u64;
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    Summary {
        bytes,
        codec: match plan.codec {
            Codec::H264 => "H.264",
            Codec::Hevc => "HEVC",
        },
        video_mbps: plan.video_bitrate_kbps as f32 / 1000.0,
        audio_kbps: plan.audio_bitrate_kbps,
        frame_rate: plan.frame_rate.as_f64() as f32,
        extension: plan.container_extension,
        auto_video_mbps: auto.video_bitrate_kbps as f32 / 1000.0,
        auto_audio_kbps: auto.audio_bitrate_kbps,
    }
}

/// Approximate minutes left: `None` until there is enough to go on (3 s
/// and 2 %), `Some(0)` for "less than a minute", else whole minutes
/// rounded up (NN/G: approximate, never a seconds countdown).
#[must_use]
pub(crate) fn minutes_left(elapsed: Duration, done: f32) -> Option<u32> {
    if elapsed < Duration::from_secs(3) || !(0.02..1.0).contains(&done) {
        return None;
    }
    let left = elapsed.as_secs_f64() * f64::from(1.0 - done) / f64::from(done);
    if left < 60.0 {
        return Some(0);
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Some((left / 60.0).ceil() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipforge_core::project::{MediaRef, RefKind};
    use clipforge_core::{Clip, MediaId, Ticks};

    fn project(seconds: i64, hdr: bool) -> Project {
        let mut p = Project::new();
        let id = MediaId::new();
        p.media.insert(
            id,
            MediaRef {
                id,
                kind: RefKind::Video,
                path: "/v.mov".into(),
                fingerprint_hash: 1,
                size: 1,
                pixel_size: Some((1920, 1080)),
                duration: Some(Ticks::from_seconds(seconds)),
                captured_at_ms: None,
                hdr,
                name: "v".into(),
            },
        );
        p.clips.push(Clip::video(id, Ticks::from_seconds(seconds)));
        p
    }

    #[test]
    fn options_follow_the_dialog_state() {
        let o = options(&DialogState::default());
        assert_eq!(o.resolution, Resolution::FullHd);
        assert_eq!(o.quality, Quality::Good);
        assert_eq!(o.advanced, Advanced::default(), "index 0 is automatic");
        let o = options(&DialogState {
            resolution: 1,
            quality: 2,
            youtube: true,
            hdr: true,
            codec: 2,
            frame_rate: 5,
            video_bitrate: 1,
            audio_bitrate: 5,
            container: 1,
        });
        assert_eq!(o.resolution, Resolution::Uhd4k);
        assert_eq!(o.quality, Quality::Best);
        assert!(o.hdr && o.optimize_for_youtube);
        assert_eq!(o.advanced.codec, Some(Codec::Hevc));
        assert_eq!(o.advanced.frame_rate, Some(FrameRate::FPS_60));
        assert_eq!(o.advanced.video_bitrate_kbps, Some(8_000));
        assert_eq!(o.advanced.audio_bitrate_kbps, Some(384));
        assert_eq!(o.advanced.container, Container::Mov);
        // Out-of-range indices fall back to automatic.
        let o = options(&DialogState {
            frame_rate: 99,
            video_bitrate: 99,
            ..DialogState::default()
        });
        assert_eq!(o.advanced.frame_rate, None);
        assert_eq!(o.advanced.video_bitrate_kbps, None);
    }

    #[test]
    fn hdr_availability_explains_why_it_is_off() {
        assert_eq!(
            hdr_availability(&project(5, false), true),
            HdrAvailability::NoHdrSources
        );
        assert_eq!(
            hdr_availability(&project(5, true), false),
            HdrAvailability::NoGpu
        );
        assert_eq!(
            hdr_availability(&project(5, true), true),
            HdrAvailability::Available
        );
        assert_eq!(HdrAvailability::NoGpu.code(), 2);
    }

    #[test]
    fn summary_shows_size_codec_and_bitrate() {
        let p = project(100, true);
        let mut state = DialogState {
            quality: 1,
            ..DialogState::default()
        };
        let s = summary(&options(&state), &p);
        assert_eq!(s.codec, "H.264");
        assert!((s.video_mbps - 12.0).abs() < 0.01);
        assert_eq!(s.audio_kbps, 256);
        // (12 000 + 256) kbit/s × 100 s / 8 = 153.2 MB.
        assert_eq!(s.bytes, 153_200_000);
        assert_eq!(s.extension, "mp4");
        state.hdr = true;
        state.container = 1;
        let s = summary(&options(&state), &p);
        assert_eq!(s.codec, "HEVC");
        assert_eq!(s.extension, "mov");
        state.youtube = true;
        state.video_bitrate = 2;
        let s = summary(&options(&state), &p);
        assert_eq!(s.extension, "mp4", "YouTube is always MP4");
        assert!((s.video_mbps - 12.0).abs() < 0.01, "chosen bitrate");
        assert!((s.auto_video_mbps - 8.0).abs() < 0.01, "HEVC ladder");
        assert_eq!(s.auto_audio_kbps, 384);
    }

    #[test]
    fn a_silent_show_is_estimated_without_audio() {
        let mut p = project(100, false);
        p.clips[0].muted = true;
        let s = summary(&options(&DialogState::default()), &p);
        // Good 1080p: 8 000 kbit/s × 100 s / 8, no audio track.
        assert_eq!(s.bytes, 100_000_000);
    }

    #[test]
    fn time_left_is_approximate_and_waits_for_data() {
        let s = Duration::from_secs;
        assert_eq!(minutes_left(s(1), 0.5), None, "too early");
        assert_eq!(minutes_left(s(10), 0.01), None, "too little done");
        assert_eq!(minutes_left(s(10), 0.5), Some(0));
        // 30 s for 10 % → 270 s left → 5 min.
        assert_eq!(minutes_left(s(30), 0.1), Some(5));
        assert_eq!(minutes_left(s(30), 1.0), None, "done");
    }
}
