//! Drives the ffmpeg sidecar.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use clipforge_jobs::{CancellationToken, Cancelled};

use crate::args;
use crate::encoders::{Encoder, EncoderCatalog};
use crate::frames::{FrameRef, FrameSource};
use crate::plan::EncodePlan;
use crate::progress::{ProgressLine, ProgressParser};
use crate::yuv::Yuv420;

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("no encoder available for {0:?}; the bundled ffmpeg lacks it")]
    NoEncoder(crate::plan::Codec),
    #[error("cannot start ffmpeg at {path}: {source}")]
    Spawn {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("writing frames to ffmpeg failed: {0}")]
    Pipe(std::io::Error),
    #[error("ffmpeg exited with {status}: {stderr}")]
    Failed { status: String, stderr: String },
    #[error("nothing to export: the timeline is empty")]
    Empty,
    #[error("export cancelled")]
    Cancelled,
    #[error("HDR frames need the GPU renderer, which is not available")]
    HdrUnavailable,
    #[error("output file was not written")]
    NoOutput,
    #[error("frame {index} is {got_w}x{got_h} but the plan is {want_w}x{want_h}")]
    FrameSize {
        index: u64,
        got_w: u32,
        got_h: u32,
        want_w: u32,
        want_h: u32,
    },
}

impl From<Cancelled> for ExportError {
    fn from(_: Cancelled) -> Self {
        ExportError::Cancelled
    }
}

/// What happened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportReport {
    pub output: PathBuf,
    pub frames: u64,
    pub encoder: &'static str,
    pub elapsed: Duration,
    pub bytes: u64,
}

/// Progress callback payload: frames written by us and what ffmpeg reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportProgress {
    pub frames_sent: u64,
    pub frames_total: u64,
    pub encoded: Option<ProgressLine>,
}

/// Encodes frames with ffmpeg.
#[derive(Clone, Debug)]
pub struct Exporter {
    ffmpeg: PathBuf,
    catalog: EncoderCatalog,
    pub prefer_hardware: bool,
}

impl Exporter {
    /// Probes the encoders of `ffmpeg`.
    pub fn new(ffmpeg: PathBuf) -> std::io::Result<Exporter> {
        let catalog = EncoderCatalog::probe(&ffmpeg)?;
        Ok(Exporter {
            ffmpeg,
            catalog,
            prefer_hardware: true,
        })
    }

    #[must_use]
    pub fn with_catalog(ffmpeg: PathBuf, catalog: EncoderCatalog) -> Exporter {
        Exporter {
            ffmpeg,
            catalog,
            prefer_hardware: true,
        }
    }

    /// The encoder that would be used for `plan`. Hardware encoders are
    /// test-encoded once per process (listed is not the same as present).
    #[must_use]
    pub fn encoder_for(&self, plan: &EncodePlan) -> Option<Encoder> {
        self.catalog
            .pick_verified(plan.codec, self.prefer_hardware, |e| {
                hardware_works(&self.ffmpeg, e, plan.pixel_format)
            })
    }

    /// Runs the export. Blocks; call from a job. `on_progress` is invoked
    /// at most every ~100 ms.
    pub fn run(
        &self,
        plan: &EncodePlan,
        frames: &mut dyn FrameSource,
        audio: Option<&Path>,
        output: &Path,
        token: &CancellationToken,
        mut on_progress: impl FnMut(ExportProgress),
    ) -> Result<ExportReport, ExportError> {
        let total = frames.len();
        if total == 0 {
            return Err(ExportError::Empty);
        }
        let encoder = self
            .encoder_for(plan)
            .ok_or(ExportError::NoEncoder(plan.codec))?;
        if let Some(parent) = output.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let argv = args::build(plan, &encoder, output, audio);
        tracing::info!(encoder = encoder.name, ?output, "starting ffmpeg");
        let mut child = Command::new(&self.ffmpeg)
            .args(&argv)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| ExportError::Spawn {
                path: self.ffmpeg.clone(),
                source,
            })?;

        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| ExportError::Pipe(std::io::Error::other("no stdin")))?;
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let (progress_tx, progress_rx) = std::sync::mpsc::channel::<ProgressLine>();
        let progress_thread = std::thread::spawn(move || {
            let Some(stdout) = stdout else { return };
            let mut parser = ProgressParser::new();
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Some(block) = parser.feed(&line)
                    && progress_tx.send(block).is_err()
                {
                    break;
                }
            }
        });
        let stderr_thread = std::thread::spawn(move || {
            let mut buf = String::new();
            if let Some(mut e) = stderr {
                let _ = std::io::Read::read_to_string(&mut e, &mut buf);
            }
            buf
        });

        let started = Instant::now();
        let mut last_report = Instant::now() - Duration::from_secs(1);
        let mut current: Option<Yuv420> = None;
        let mut latest_encoded: Option<ProgressLine> = None;
        let mut write_result: Result<(), std::io::Error> = Ok(());
        let mut cancelled = false;
        for i in 0..total {
            if token.is_cancelled() {
                cancelled = true;
                break;
            }
            let fresh = match frames.frame(i) {
                FrameRef::New(frame) => {
                    Some(((frame.width, frame.height), Yuv420::from_frame(&frame)))
                }
                FrameRef::Hdr(frame) => {
                    Some(((frame.width, frame.height), Yuv420::from_frame16(&frame)))
                }
                FrameRef::Unavailable => {
                    abandon(&mut child, output);
                    return Err(ExportError::HdrUnavailable);
                }
                FrameRef::SameAsPrevious => None,
            };
            if let Some(((got_w, got_h), yuv)) = fresh {
                if (got_w, got_h) != (plan.width, plan.height) {
                    abandon(&mut child, output);
                    return Err(ExportError::FrameSize {
                        index: i,
                        got_w,
                        got_h,
                        want_w: plan.width,
                        want_h: plan.height,
                    });
                }
                current = Some(yuv);
            }
            let Some(buf) = &current else {
                write_result = Err(std::io::Error::other(
                    "frame source returned SameAsPrevious first",
                ));
                break;
            };
            if let Err(e) = stdin.write_all(&buf.data) {
                write_result = Err(e);
                break;
            }
            while let Ok(p) = progress_rx.try_recv() {
                latest_encoded = Some(p);
            }
            if last_report.elapsed() >= Duration::from_millis(100) {
                last_report = Instant::now();
                on_progress(ExportProgress {
                    frames_sent: i + 1,
                    frames_total: total,
                    encoded: latest_encoded.clone(),
                });
            }
        }
        drop(stdin);

        if cancelled {
            let _ = child.kill();
            let _ = child.wait();
            let _ = progress_thread.join();
            let _ = stderr_thread.join();
            let _ = std::fs::remove_file(output);
            return Err(ExportError::Cancelled);
        }

        let status = child.wait().map_err(ExportError::Pipe)?;
        let _ = progress_thread.join();
        let stderr_text = stderr_thread.join().unwrap_or_default();
        if let Err(e) = write_result {
            // ffmpeg probably died; its stderr explains why.
            if !status.success() {
                return Err(ExportError::Failed {
                    status: status.to_string(),
                    stderr: stderr_text.trim().to_owned(),
                });
            }
            return Err(ExportError::Pipe(e));
        }
        if !status.success() {
            let _ = std::fs::remove_file(output);
            return Err(ExportError::Failed {
                status: status.to_string(),
                stderr: stderr_text.trim().to_owned(),
            });
        }
        let bytes = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
        if bytes == 0 {
            return Err(ExportError::NoOutput);
        }
        on_progress(ExportProgress {
            frames_sent: total,
            frames_total: total,
            encoded: latest_encoded,
        });
        Ok(ExportReport {
            output: output.to_path_buf(),
            frames: total,
            encoder: encoder.name,
            elapsed: started.elapsed(),
            bytes,
        })
    }
}

/// Test encodes already run in this process, keyed by binary, encoder and
/// pixel format (a GPU may do 8-bit H.264 but no 10-bit HEVC).
type ProbeKey = (PathBuf, &'static str, crate::plan::PixelFormat);
static PROBES: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<ProbeKey, bool>>> =
    std::sync::OnceLock::new();

/// How long a test encode may take before the encoder counts as broken.
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

fn hardware_works(ffmpeg: &Path, encoder: &Encoder, format: crate::plan::PixelFormat) -> bool {
    let key = (ffmpeg.to_path_buf(), encoder.name, format);
    let cache = PROBES.get_or_init(Default::default);
    if let Some(known) = cache.lock().ok().and_then(|c| c.get(&key).copied()) {
        return known;
    }
    let ok = run_probe(ffmpeg, &args::probe(encoder, format));
    tracing::info!(encoder = encoder.name, ?format, ok, "encoder test encode");
    if let Ok(mut c) = cache.lock() {
        c.insert(key, ok);
    }
    ok
}

fn run_probe(ffmpeg: &Path, argv: &[String]) -> bool {
    let Ok(mut child) = Command::new(ffmpeg)
        .args(argv)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if started.elapsed() < PROBE_TIMEOUT => {
                std::thread::sleep(Duration::from_millis(20));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

/// Stops ffmpeg and removes the partial file.
fn abandon(child: &mut std::process::Child, output: &Path) {
    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(output);
}

#[cfg(test)]
#[allow(clippy::print_stderr)]
mod tests {
    use super::*;
    use crate::options::ExportOptions;
    use clipforge_core::{Aspect, FrameRate, Ticks};
    use clipforge_media::FfmpegLocation;
    use clipforge_render::Frame;

    struct Solid {
        n: u64,
        w: u32,
        h: u32,
    }

    impl FrameSource for Solid {
        fn len(&self) -> u64 {
            self.n
        }
        fn frame(&mut self, index: u64) -> FrameRef {
            if index.is_multiple_of(15) {
                FrameRef::New(Frame::solid(
                    self.w,
                    self.h,
                    [(index * 5 % 255) as u8, 80, 160],
                ))
            } else {
                FrameRef::SameAsPrevious
            }
        }
    }

    fn small_plan() -> EncodePlan {
        let mut p = EncodePlan::build(
            &ExportOptions::default(),
            Aspect::Landscape16x9,
            FrameRate::FPS_30,
        );
        p.width = 320;
        p.height = 180;
        p.video_bitrate_kbps = 800;
        p
    }

    fn ffmpeg() -> Option<FfmpegLocation> {
        let loc = FfmpegLocation::discover();
        if loc.is_none() {
            eprintln!("ffmpeg not installed; skipping");
        }
        loc
    }

    #[test]
    fn exports_a_short_clip_and_reports_progress() {
        let Some(loc) = ffmpeg() else { return };
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("nested").join("out.mp4");
        let mut exporter = Exporter::new(loc.ffmpeg.clone()).unwrap();
        exporter.prefer_hardware = false; // deterministic on CI
        let plan = small_plan();
        let mut frames = Solid {
            n: 45,
            w: 320,
            h: 180,
        };
        let mut reports = Vec::new();
        let report = exporter
            .run(
                &plan,
                &mut frames,
                None,
                &out,
                &CancellationToken::new(),
                |p| reports.push(p),
            )
            .unwrap();
        assert_eq!(report.frames, 45);
        assert!(report.bytes > 0);
        assert!(out.is_file());
        assert_eq!(reports.last().unwrap().frames_sent, 45);
        // Verify with ffprobe.
        let cli = clipforge_media::FfmpegCli::new(loc);
        let info = clipforge_media::Prober::probe(&cli, &out).unwrap();
        assert_eq!((info.width, info.height), (Some(320), Some(180)));
        assert_eq!(info.frame_rate, Some(FrameRate::FPS_30));
        let d = info.duration.unwrap().as_seconds_f64();
        assert!((1.4..=1.6).contains(&d), "{d}");
        assert_eq!(info.codec, "h264");
    }

    #[test]
    fn cancel_removes_partial_output() {
        let Some(loc) = ffmpeg() else { return };
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out.mp4");
        let mut exporter = Exporter::new(loc.ffmpeg).unwrap();
        exporter.prefer_hardware = false;
        let token = CancellationToken::new();
        let mut frames = Solid {
            n: 3000,
            w: 320,
            h: 180,
        };
        let t = token.clone();
        let mut n = 0;
        let err = exporter
            .run(&small_plan(), &mut frames, None, &out, &token, |_| {
                n += 1;
                if n >= 2 {
                    t.cancel();
                }
            })
            .unwrap_err();
        assert!(matches!(err, ExportError::Cancelled));
        assert!(!out.exists());
    }

    #[test]
    fn empty_source_and_missing_encoder_fail_early() {
        let exporter = Exporter::with_catalog(PathBuf::from("ffmpeg"), EncoderCatalog::default());
        let mut frames = Solid { n: 0, w: 2, h: 2 };
        assert!(matches!(
            exporter.run(
                &small_plan(),
                &mut frames,
                None,
                Path::new("/tmp/x.mp4"),
                &CancellationToken::new(),
                |_| {}
            ),
            Err(ExportError::Empty)
        ));
        let mut frames = Solid { n: 5, w: 2, h: 2 };
        assert!(matches!(
            exporter.run(
                &small_plan(),
                &mut frames,
                None,
                Path::new("/tmp/x.mp4"),
                &CancellationToken::new(),
                |_| {}
            ),
            Err(ExportError::NoEncoder(_))
        ));
    }

    #[test]
    fn wrong_frame_size_is_rejected() {
        let Some(loc) = ffmpeg() else { return };
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("x.mp4");
        let mut exporter = Exporter::new(loc.ffmpeg).unwrap();
        exporter.prefer_hardware = false;
        let mut frames = Solid {
            n: 30,
            w: 100,
            h: 100,
        };
        let err = exporter
            .run(
                &small_plan(),
                &mut frames,
                None,
                &out,
                &CancellationToken::new(),
                |_| {},
            )
            .unwrap_err();
        assert!(
            matches!(
                err,
                ExportError::FrameSize {
                    got_w: 100,
                    want_w: 320,
                    ..
                }
            ),
            "{err}"
        );
        assert!(!out.exists());
    }

    /// HLG frames: the left half reference white (signal 0.75), the right
    /// half a bright highlight (signal 0.95).
    struct Hlg {
        n: u64,
    }

    impl FrameSource for Hlg {
        fn len(&self) -> u64 {
            self.n
        }
        fn frame(&mut self, index: u64) -> FrameRef {
            if index > 0 {
                return FrameRef::SameAsPrevious;
            }
            let (w, h) = (320usize, 180usize);
            let mut rgba16 = Vec::with_capacity(w * h * 4);
            for _ in 0..h {
                for x in 0..w {
                    let v = if x < w / 2 { 49_151 } else { 62_258 };
                    rgba16.extend_from_slice(&[v, v, v, u16::MAX]);
                }
            }
            FrameRef::Hdr(clipforge_render::Frame16 {
                width: 320,
                height: 180,
                rgba16,
            })
        }
    }

    #[test]
    fn hdr_export_writes_10_bit_hlg_hevc() {
        let Some(loc) = ffmpeg() else { return };
        let mut exporter = Exporter::new(loc.ffmpeg.clone()).unwrap();
        exporter.prefer_hardware = false;
        let mut plan = EncodePlan::build(
            &ExportOptions {
                hdr: true,
                ..ExportOptions::default()
            },
            Aspect::Landscape16x9,
            FrameRate::FPS_30,
        );
        (plan.width, plan.height, plan.video_bitrate_kbps) = (320, 180, 2_000);
        if exporter.encoder_for(&plan).is_none() {
            eprintln!("no HEVC encoder; skipping");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("hdr.mp4");
        exporter
            .run(
                &plan,
                &mut Hlg { n: 15 },
                None,
                &out,
                &CancellationToken::new(),
                |_| {},
            )
            .unwrap();
        let cli = clipforge_media::FfmpegCli::new(loc);
        let info = clipforge_media::Prober::probe(&cli, &out).unwrap();
        assert_eq!(info.codec, "hevc");
        assert_eq!(info.transfer, clipforge_media::ColorTransfer::Hlg);
        // Decoded back, the levels survive the 10-bit round trip.
        let mut reader =
            clipforge_media::VideoReader::open(&cli, &out, &info, Ticks::ZERO, 320).unwrap();
        let Some(clipforge_media::VideoFrame::Hdr(img)) = reader.next_frame().unwrap() else {
            panic!("HLG output decodes as HDR");
        };
        let at = |x: usize| f32::from(img.rgb48[(90 * 320 + x) * 3 + 1]) / 65_535.0;
        assert!((at(80) - 0.75).abs() < 0.01, "white {}", at(80));
        assert!((at(240) - 0.95).abs() < 0.01, "highlight {}", at(240));
    }

    #[test]
    fn test_encodes_accept_real_encoders_and_reject_absent_ones() {
        let Some(loc) = ffmpeg() else { return };
        let sw = Encoder {
            name: "libx264",
            hardware: true, // pretend, to force the probe
        };
        assert!(hardware_works(
            &loc.ffmpeg,
            &sw,
            crate::plan::PixelFormat::Yuv420p
        ));
        // An encoder this machine cannot have (NVENC on a Mac, or a name
        // that does not exist) fails quickly and is remembered.
        let absent = Encoder {
            name: if cfg!(target_os = "macos") {
                "h264_nvenc"
            } else {
                "h264_videotoolbox"
            },
            hardware: true,
        };
        let started = Instant::now();
        assert!(!hardware_works(
            &loc.ffmpeg,
            &absent,
            crate::plan::PixelFormat::Yuv420p
        ));
        assert!(started.elapsed() < PROBE_TIMEOUT);
        let again = Instant::now();
        assert!(!hardware_works(
            &loc.ffmpeg,
            &absent,
            crate::plan::PixelFormat::Yuv420p
        ));
        assert!(again.elapsed() < Duration::from_millis(50), "cached");
    }

    #[test]
    fn missing_binary_is_a_spawn_error() {
        let exporter = Exporter::with_catalog(
            PathBuf::from("/nonexistent/ffmpeg"),
            EncoderCatalog::from_names(["libx264"]),
        );
        let mut frames = Solid { n: 5, w: 2, h: 2 };
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            exporter.run(
                &small_plan(),
                &mut frames,
                None,
                &dir.path().join("x.mp4"),
                &CancellationToken::new(),
                |_| {}
            ),
            Err(ExportError::Spawn { .. })
        ));
    }
}
