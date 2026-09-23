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
    #[error("output file was not written")]
    NoOutput,
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

    /// The encoder that would be used for `plan`.
    #[must_use]
    pub fn encoder_for(&self, plan: &EncodePlan) -> Option<Encoder> {
        self.catalog.pick(plan.codec, self.prefer_hardware)
    }

    /// Runs the export. Blocks; call from a job. `on_progress` is invoked
    /// at most every ~100 ms.
    pub fn run(
        &self,
        plan: &EncodePlan,
        frames: &mut dyn FrameSource,
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
        let argv = args::build(plan, &encoder, output);
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
            match frames.frame(i) {
                FrameRef::New(frame) => current = Some(Yuv420::from_frame(&frame)),
                FrameRef::SameAsPrevious => {}
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

#[cfg(test)]
#[allow(clippy::print_stderr)]
mod tests {
    use super::*;
    use crate::options::ExportOptions;
    use clipforge_core::{Aspect, FrameRate};
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
            .run(&plan, &mut frames, &out, &CancellationToken::new(), |p| {
                reports.push(p)
            })
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
            .run(&small_plan(), &mut frames, &out, &token, |_| {
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
                Path::new("/tmp/x.mp4"),
                &CancellationToken::new(),
                |_| {}
            ),
            Err(ExportError::NoEncoder(_))
        ));
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
                &dir.path().join("x.mp4"),
                &CancellationToken::new(),
                |_| {}
            ),
            Err(ExportError::Spawn { .. })
        ));
    }
}
