//! End to end (PLAN.md §5 "e2e"): a project from fixtures exported as SDR
//! and as HDR, checked with ffprobe and by decoding the result.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr
)]

use std::path::PathBuf;

use clipforge_core::project::{MediaRef, RefKind, TitleBackground};
use clipforge_core::{Clip, Command, FrameRate, MediaId, Project, Ticks};
use clipforge_export::{EncodePlan, ExportOptions, Exporter, FileSources, TimelineFrames, verify};
use clipforge_jobs::CancellationToken;
use clipforge_media::{Backends, FfmpegCli, FfmpegLocation, Prober, VideoFrame, VideoReader};
use clipforge_render::{Compositor, FrameRenderer, GpuCompositor, RenderQuality, colour};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

/// A white colour card (1 s), then the HLG iPhone-style clip (1 s).
fn project() -> Project {
    let id = MediaId::new();
    let video = MediaRef {
        id,
        kind: RefKind::Video,
        path: fixture("video_hlg_hevc.mp4"),
        fingerprint_hash: 1,
        size: 1,
        pixel_size: None,
        duration: Some(Ticks::SECOND),
        captured_at_ms: None,
        hdr: true,
        name: "hlg".into(),
    };
    let mut p = Project::new();
    Command::InsertClips {
        entries: vec![
            (0, Clip::title(TitleBackground::White, Ticks::SECOND)),
            (1, Clip::video(id, Ticks::SECOND)),
        ],
        media: vec![video],
    }
    .apply(&mut p)
    .unwrap();
    assert!(p.has_hdr_sources());
    p
}

fn export(
    loc: &FfmpegLocation,
    p: &Project,
    renderer: &dyn FrameRenderer,
    hdr: bool,
    out: &std::path::Path,
) -> EncodePlan {
    let mut plan = EncodePlan::build(
        &ExportOptions {
            hdr,
            optimize_for_youtube: true,
            ..ExportOptions::default()
        },
        p.settings.aspect,
        p.settings.frame_rate,
    );
    let (w, h) = RenderQuality::Preview.frame_size(p.settings.aspect);
    (plan.width, plan.height, plan.video_bitrate_kbps) = (w, h, 3_000);
    let mut exporter = Exporter::new(loc.ffmpeg.clone()).unwrap();
    exporter.prefer_hardware = false;
    let sources = FileSources::for_project(p, Backends::discover());
    let mut frames = TimelineFrames::new(p, renderer, &sources, RenderQuality::Preview).hdr(hdr);
    let report = exporter
        .run(
            &plan,
            &mut frames,
            None,
            out,
            &CancellationToken::new(),
            |_| {},
        )
        .unwrap();
    let problems = verify(
        &FfmpegCli::new(loc.clone()),
        &plan,
        report.frames,
        false,
        out,
    );
    assert!(problems.is_empty(), "{problems:?}");
    plan
}

#[test]
fn sdr_export_of_an_hlg_timeline() {
    let Some(loc) = FfmpegLocation::discover() else {
        eprintln!("ffmpeg not installed; skipping");
        return;
    };
    let p = project();
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("sdr.mp4");
    let plan = export(&loc, &p, &Compositor::new(), false, &out);
    assert_eq!(plan.color.transfer, "bt709");
    let cli = FfmpegCli::new(loc);
    let info = cli.probe(&out).unwrap();
    assert_eq!(info.codec, "h264");
    assert!(!info.is_hdr());
}

#[test]
fn hdr_export_keeps_hlg_and_puts_sdr_white_at_reference_level() {
    let Some(loc) = FfmpegLocation::discover() else {
        eprintln!("ffmpeg not installed; skipping");
        return;
    };
    let Some(gpu) = GpuCompositor::new() else {
        eprintln!("no GPU adapter; skipping");
        return;
    };
    let p = project();
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("hdr.mp4");
    let plan = export(&loc, &p, &gpu, true, &out);
    assert_eq!(plan.codec, clipforge_export::Codec::Hevc);
    let cli = FfmpegCli::new(loc);
    let info = cli.probe(&out).unwrap();
    assert_eq!(info.codec, "hevc");
    assert_eq!(info.transfer, clipforge_media::ColorTransfer::Hlg);
    assert_eq!(info.frame_rate, Some(FrameRate::FPS_30));

    // The colour card: its SDR colour lands where sdr_to_hlg puts it
    // (white ≈ HLG 0.75, BT.2408), not at peak.
    let sdr = Compositor::new().render(
        &p,
        Ticks::from_millis(500),
        RenderQuality::Preview,
        &clipforge_render::source::MapProvider::default(),
    );
    let (cx, cy) = (sdr.width / 2, sdr.height / 2);
    let px = sdr.pixel(cx, cy);
    let want = colour::sdr_to_hlg([px[0], px[1], px[2]].map(|v| f32::from(v) / 255.0));
    let want_green = want[1];
    let mut reader = VideoReader::open(&cli, &out, &info, Ticks::ZERO, 2_000).unwrap();
    let Some(VideoFrame::Hdr(img)) = reader.frame_at(Ticks::from_millis(500)).unwrap() else {
        panic!("HLG output decodes as HDR");
    };
    let i = ((cy * img.width + cx) * 3) as usize;
    for (c, want) in want.iter().enumerate() {
        let got = f32::from(img.rgb48[i + c]) / 65_535.0;
        assert!((got - want).abs() < 0.02, "channel {c}: {got} vs {want}");
    }
    assert!(
        want[1] > 0.7 && want_green < 0.76,
        "white card near reference white"
    );
}
