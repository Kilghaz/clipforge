//! Streaming decoders on top of the ffmpeg executable (ADR-0008).
//!
//! A [`VideoReader`] streams raw RGBA frames at a requested size from a
//! start time; an [`AudioReader`] streams interleaved stereo `f32` PCM at
//! 48 kHz. Both kill their child process on drop.

use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

use clipforge_core::{FrameRate, Ticks};

use crate::error::{MediaError, Result};
use crate::ffmpeg_cli::FfmpegCli;
use crate::info::MediaInfo;
use crate::kind::ColorTransfer;
use crate::probe::{DecodedImage, fit_within};

/// A decoded video frame. SDR sources arrive as 8-bit RGBA ready to show;
/// HDR sources (HLG, PQ) as 16-bit BT.2020 RGB in their own transfer, for
/// the colour pipeline to tone map (SDR output) or keep (HDR output).
#[derive(Clone, PartialEq, Eq)]
pub enum VideoFrame {
    Sdr(DecodedImage),
    Hdr(HdrImage),
}

/// 16-bit RGB samples of an HDR frame, exactly as coded (BT.2020 primaries,
/// full range, HLG or PQ transfer).
#[derive(Clone, PartialEq, Eq)]
pub struct HdrImage {
    pub width: u32,
    pub height: u32,
    pub transfer: ColorTransfer,
    /// Interleaved R, G, B; `width * height * 3` samples.
    pub rgb48: Vec<u16>,
}

impl std::fmt::Debug for HdrImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HdrImage")
            .field("size", &(self.width, self.height))
            .field("transfer", &self.transfer)
            .finish()
    }
}

impl std::fmt::Debug for VideoFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (w, h) = self.size();
        f.debug_struct("VideoFrame")
            .field("size", &(w, h))
            .field("hdr", &matches!(self, VideoFrame::Hdr(_)))
            .finish()
    }
}

impl VideoFrame {
    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        match self {
            VideoFrame::Sdr(i) => (i.width, i.height),
            VideoFrame::Hdr(i) => (i.width, i.height),
        }
    }
}

/// Sample rate every audio stream is converted to.
pub const AUDIO_SAMPLE_RATE: u32 = 48_000;
/// Channels every audio stream is converted to.
pub const AUDIO_CHANNELS: usize = 2;

/// Hardware decoder to request on this platform. ffmpeg treats a missing
/// device as a fatal option error, so [`VideoReader`] retries in software
/// when a hardware run yields no frames and remembers that here.
static HWACCEL_BROKEN: AtomicBool = AtomicBool::new(false);

fn default_hwaccel() -> Option<&'static str> {
    if HWACCEL_BROKEN.load(Ordering::Relaxed) {
        return None;
    }
    if cfg!(target_os = "macos") {
        Some("videotoolbox")
    } else if cfg!(windows) {
        Some("d3d11va")
    } else {
        None
    }
}

fn seconds(t: Ticks) -> String {
    format!("{:.6}", t.as_seconds_f64().max(0.0))
}

fn spawn(cli: &FfmpegCli, args: &[String], path: &Path) -> Result<Child> {
    Command::new(&cli.location().ffmpeg)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| MediaError::ToolMissing {
            tool: "ffmpeg".into(),
            detail: format!("{e} ({})", path.display()),
        })
}

/// Streams video frames in presentation order from `start`.
pub struct VideoReader {
    child: Child,
    out: BufReader<ChildStdout>,
    width: u32,
    height: u32,
    frame_rate: FrameRate,
    start: Ticks,
    /// Everything needed to respawn without hardware decoding.
    respawn: Option<(FfmpegCli, PathBuf, MediaInfo, u32)>,
    /// Index of the next frame to be read (relative to `start`).
    next: i64,
    /// Last frame read, kept for `frame_at` when the wanted time falls
    /// between frames.
    last: Option<(Ticks, VideoFrame)>,
    finished: bool,
    /// HLG / PQ sources are read as 16-bit RGB.
    hdr: Option<ColorTransfer>,
}

impl std::fmt::Debug for VideoReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoReader")
            .field("size", &(self.width, self.height))
            .field("next", &self.next)
            .finish()
    }
}

impl VideoReader {
    /// Starts decoding `path` at `start`, scaled so the longer displayed
    /// edge is at most `max_edge`. `info` must come from probing the file
    /// (display size, frame rate). Frames are delivered at a constant rate
    /// even for variable-frame-rate sources.
    pub fn open(
        cli: &FfmpegCli,
        path: &Path,
        info: &MediaInfo,
        start: Ticks,
        max_edge: u32,
    ) -> Result<VideoReader> {
        Self::open_with(cli, path, info, start, max_edge, default_hwaccel())
    }

    /// Like [`VideoReader::open`] with an explicit hardware decoder
    /// (`None` = software). A hardware run that produces no frames is
    /// retried in software once.
    pub fn open_with(
        cli: &FfmpegCli,
        path: &Path,
        info: &MediaInfo,
        start: Ticks,
        max_edge: u32,
        hwaccel: Option<&str>,
    ) -> Result<VideoReader> {
        let (dw, dh) = info
            .display_size()
            .ok_or_else(|| MediaError::Unsupported("no picture".into()))?;
        let (width, height) = fit_within(dw, dh, max_edge);
        // Even dimensions keep every downstream consumer (yuv420, encoders) happy.
        let (width, height) = (width.max(2) & !1, height.max(2) & !1);
        let frame_rate = info.frame_rate.unwrap_or(FrameRate::FPS_30);
        // HDR video keeps its full precision and its own transfer: 16-bit
        // RGB with the BT.2020 matrix, full range. Converting it for display
        // is the colour pipeline's job (ffmpeg's default would flatten it).
        let hdr = matches!(info.transfer, ColorTransfer::Hlg | ColorTransfer::Pq)
            .then_some(info.transfer);
        let (filter, pix_fmt) = if hdr.is_some() {
            (
                format!(
                    "scale={width}:{height}:flags=bicubic:in_color_matrix=bt2020:in_range=tv:out_range=pc,format=rgb48le"
                ),
                "rgb48le",
            )
        } else {
            (
                format!("scale={width}:{height}:flags=bicubic,format=rgba"),
                "rgba",
            )
        };
        let fps = format!("{}/{}", frame_rate.numerator(), frame_rate.denominator());
        let mut args: Vec<String> = vec!["-nostdin".into(), "-loglevel".into(), "error".into()];
        if let Some(hw) = hwaccel {
            args.extend(["-hwaccel".into(), hw.to_owned()]);
        }
        args.extend([
            "-ss".into(),
            seconds(start),
            "-i".into(),
            path.to_string_lossy().into_owned(),
            "-an".into(),
            "-sn".into(),
            "-vf".into(),
            filter,
            "-fps_mode".into(),
            "cfr".into(),
            "-r".into(),
            fps,
            "-f".into(),
            "rawvideo".into(),
            "-pix_fmt".into(),
            pix_fmt.into(),
            "pipe:1".into(),
        ]);
        let mut child = spawn(cli, &args, path)?;
        let stdout = child.stdout.take().ok_or_else(|| MediaError::ToolFailed {
            tool: "ffmpeg".into(),
            detail: "no stdout".into(),
        })?;
        Ok(VideoReader {
            child,
            out: BufReader::with_capacity(1 << 20, stdout),
            width,
            height,
            frame_rate,
            start,
            respawn: hwaccel.map(|_| (cli.clone(), path.to_path_buf(), info.clone(), max_edge)),
            next: 0,
            last: None,
            finished: false,
            hdr,
        })
    }

    /// Replaces this reader's process with a software-decoding one if the
    /// hardware attempt ended before delivering a single frame.
    fn fall_back_to_software(&mut self) -> Result<bool> {
        let Some((cli, path, info, max_edge)) = self.respawn.take() else {
            return Ok(false);
        };
        if self.next != 0 {
            return Ok(false);
        }
        tracing::warn!(path = %path.display(), "hardware decoding produced no frames; using software");
        HWACCEL_BROKEN.store(true, Ordering::Relaxed);
        let fresh = Self::open_with(&cli, &path, &info, self.start, max_edge, None)?;
        let _ = self.child.kill();
        let _ = self.child.wait();
        // Take the new process over; `fresh` must not kill it on drop.
        let mut fresh = std::mem::ManuallyDrop::new(fresh);
        std::mem::swap(&mut self.child, &mut fresh.child);
        std::mem::swap(&mut self.out, &mut fresh.out);
        // The old (dead) child now sits in `fresh`; reap it explicitly.
        let _ = fresh.child.wait();
        self.finished = false;
        Ok(true)
    }

    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Presentation time of the next frame to be read.
    #[must_use]
    pub fn next_pts(&self) -> Ticks {
        self.start + Ticks::from_frames(self.next, self.frame_rate)
    }

    /// Whether frames come as 16-bit HDR samples.
    #[must_use]
    pub fn is_hdr(&self) -> bool {
        self.hdr.is_some()
    }

    /// Reads the next frame. `None` at end of stream.
    pub fn next_frame(&mut self) -> Result<Option<VideoFrame>> {
        if self.finished {
            return Ok(None);
        }
        let pixels = self.width as usize * self.height as usize;
        let len = if self.hdr.is_some() {
            pixels * 6
        } else {
            pixels * 4
        };
        let mut bytes = vec![0u8; len];
        match self.out.read_exact(&mut bytes) {
            Ok(()) => {
                let pts = self.next_pts();
                self.next += 1;
                let img = match self.hdr {
                    Some(transfer) => VideoFrame::Hdr(HdrImage {
                        width: self.width,
                        height: self.height,
                        transfer,
                        rgb48: bytes
                            .as_chunks::<2>()
                            .0
                            .iter()
                            .map(|b| u16::from_le_bytes(*b))
                            .collect(),
                    }),
                    None => VideoFrame::Sdr(DecodedImage {
                        width: self.width,
                        height: self.height,
                        rgba: bytes,
                    }),
                };
                self.last = Some((pts, img.clone()));
                Ok(Some(img))
            }
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                if self.fall_back_to_software()? {
                    return self.next_frame();
                }
                self.finished = true;
                Ok(None)
            }
            Err(e) => Err(MediaError::ToolFailed {
                tool: "ffmpeg".into(),
                detail: e.to_string(),
            }),
        }
    }

    /// The frame shown at source time `t` (>= the last request). Advances
    /// the stream, skipping frames as needed; returns the last frame when
    /// the stream ended before `t`.
    pub fn frame_at(&mut self, t: Ticks) -> Result<Option<VideoFrame>> {
        // Already have a frame that covers t?
        if let Some((pts, img)) = &self.last
            && *pts <= t
            && t < *pts + self.frame_rate.frame_duration()
        {
            return Ok(Some(img.clone()));
        }
        while self.next_pts() + self.frame_rate.frame_duration() <= t {
            if self.next_frame()?.is_none() {
                break;
            }
        }
        if self.finished {
            return Ok(self.last.as_ref().map(|(_, img)| img.clone()));
        }
        self.next_frame()
    }

    /// True if `t` lies at or after the current position, i.e. the stream
    /// can reach it without seeking backwards.
    #[must_use]
    pub fn can_reach(&self, t: Ticks) -> bool {
        match &self.last {
            Some((pts, _)) => t >= *pts,
            None => t >= self.start,
        }
    }
}

impl Drop for VideoReader {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Streams interleaved stereo `f32` samples at 48 kHz from `start`.
pub struct AudioReader {
    child: Child,
    out: BufReader<ChildStdout>,
    finished: bool,
    /// Samples (per channel) delivered so far.
    position: u64,
    start: Ticks,
}

impl std::fmt::Debug for AudioReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioReader")
            .field("position", &self.position)
            .finish()
    }
}

impl AudioReader {
    pub fn open(cli: &FfmpegCli, path: &Path, start: Ticks) -> Result<AudioReader> {
        let args: Vec<String> = vec![
            "-nostdin".into(),
            "-loglevel".into(),
            "error".into(),
            "-ss".into(),
            seconds(start),
            "-i".into(),
            path.to_string_lossy().into_owned(),
            "-vn".into(),
            "-sn".into(),
            "-ac".into(),
            AUDIO_CHANNELS.to_string(),
            "-ar".into(),
            AUDIO_SAMPLE_RATE.to_string(),
            "-f".into(),
            "f32le".into(),
            "pipe:1".into(),
        ];
        let mut child = spawn(cli, &args, path)?;
        let stdout = child.stdout.take().ok_or_else(|| MediaError::ToolFailed {
            tool: "ffmpeg".into(),
            detail: "no stdout".into(),
        })?;
        Ok(AudioReader {
            child,
            out: BufReader::with_capacity(1 << 16, stdout),
            finished: false,
            position: 0,
            start,
        })
    }

    /// Fills `buf` (interleaved stereo) as far as possible. Returns the
    /// number of `f32` values written (a multiple of the channel count);
    /// zero at end of stream.
    pub fn read(&mut self, buf: &mut [f32]) -> Result<usize> {
        if self.finished || buf.is_empty() {
            return Ok(0);
        }
        let want = (buf.len() / AUDIO_CHANNELS) * AUDIO_CHANNELS;
        let mut bytes = vec![0u8; want * 4];
        let mut filled = 0;
        while filled < bytes.len() {
            match self.out.read(&mut bytes[filled..]) {
                Ok(0) => {
                    self.finished = true;
                    break;
                }
                Ok(n) => filled += n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => {
                    return Err(MediaError::ToolFailed {
                        tool: "ffmpeg".into(),
                        detail: e.to_string(),
                    });
                }
            }
        }
        let values = filled / 4 / AUDIO_CHANNELS * AUDIO_CHANNELS;
        for (i, chunk) in bytes[..values * 4].as_chunks::<4>().0.iter().enumerate() {
            buf[i] = f32::from_le_bytes(*chunk);
        }
        self.position += (values / AUDIO_CHANNELS) as u64;
        Ok(values)
    }

    /// Source time of the next sample.
    #[must_use]
    pub fn position(&self) -> Ticks {
        #[allow(clippy::cast_possible_wrap)]
        let samples = self.position as i64;
        self.start + Ticks::from_samples(samples, AUDIO_SAMPLE_RATE)
    }
}

impl Drop for AudioReader {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
#[allow(clippy::print_stderr)]
mod tests {
    use super::*;
    use crate::probe::Prober;
    use crate::test_support::fixture;

    fn cli() -> Option<FfmpegCli> {
        let c = FfmpegCli::discover();
        if c.is_none() {
            eprintln!("ffmpeg not installed; skipping");
        }
        c
    }

    #[test]
    fn streams_frames_at_constant_rate_from_a_start_time() {
        let Some(cli) = cli() else { return };
        let path = fixture("video_sdr_h264.mp4");
        let info = cli.probe(&path).unwrap();
        let mut r = VideoReader::open(&cli, &path, &info, Ticks::from_millis(500), 96).unwrap();
        assert_eq!(r.size(), (96, 54));
        assert_eq!(r.next_pts(), Ticks::from_millis(500));
        let mut n = 0;
        while let Some(f) = r.next_frame().unwrap() {
            let VideoFrame::Sdr(f) = f else {
                panic!("SDR source decodes to 8-bit")
            };
            assert_eq!(f.rgba.len(), 96 * 54 * 4);
            n += 1;
        }
        // 2 s at 25 fps minus 0.5 s = 37 or 38 frames depending on rounding.
        assert!((36..=39).contains(&n), "{n}");
        assert!(r.next_frame().unwrap().is_none(), "stays finished");
    }

    #[test]
    fn frame_at_skips_forward_and_holds_the_last_frame() {
        let Some(cli) = cli() else { return };
        let path = fixture("video_sdr_h264.mp4");
        let info = cli.probe(&path).unwrap();
        let mut r = VideoReader::open(&cli, &path, &info, Ticks::ZERO, 64).unwrap();
        let a = r.frame_at(Ticks::from_millis(1_000)).unwrap().unwrap();
        assert_eq!(
            r.next_pts(),
            Ticks::from_millis(1_040),
            "25 frames consumed"
        );
        let b = r.frame_at(Ticks::from_millis(1_010)).unwrap().unwrap();
        assert_eq!(a, b, "same source frame within one frame duration");
        assert!(r.can_reach(Ticks::from_millis(1_500)));
        assert!(!r.can_reach(Ticks::from_millis(100)));
        let end = r.frame_at(Ticks::from_seconds(10)).unwrap();
        assert!(end.is_some(), "past the end returns the last frame");
    }

    #[test]
    fn rotated_video_streams_in_display_orientation() {
        let Some(cli) = cli() else { return };
        let path = fixture("video_rotated_90.mov");
        let info = cli.probe(&path).unwrap();
        let mut r = VideoReader::open(&cli, &path, &info, Ticks::ZERO, 64).unwrap();
        assert_eq!(r.size(), (36, 64));
        let f = r.next_frame().unwrap().unwrap();
        assert_eq!(f.size(), (36, 64));
    }

    #[test]
    fn unusable_hardware_decoder_falls_back_to_software() {
        let Some(cli) = cli() else { return };
        let path = fixture("video_sdr_h264.mp4");
        let info = cli.probe(&path).unwrap();
        let mut r = VideoReader::open_with(
            &cli,
            &path,
            &info,
            Ticks::ZERO,
            64,
            Some("definitely-not-a-hwaccel"),
        )
        .unwrap();
        let mut n = 0;
        while r.next_frame().unwrap().is_some() {
            n += 1;
        }
        assert!(
            (48..=51).contains(&n),
            "software fallback must deliver the clip: {n}"
        );
        assert!(HWACCEL_BROKEN.load(Ordering::Relaxed));
        HWACCEL_BROKEN.store(false, Ordering::Relaxed);
    }

    #[test]
    fn audio_reader_delivers_stereo_48k_and_reports_position() {
        let Some(cli) = cli() else { return };
        let path = fixture("video_sdr_h264.mp4");
        let mut r = AudioReader::open(&cli, &path, Ticks::from_millis(500)).unwrap();
        let mut total = 0usize;
        let mut energy = 0.0f64;
        let mut buf = vec![0.0f32; 4096];
        loop {
            let n = r.read(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            total += n;
            energy += buf[..n]
                .iter()
                .map(|s| f64::from(*s) * f64::from(*s))
                .sum::<f64>();
        }
        let per_channel = total / AUDIO_CHANNELS;
        // 1.5 s of 48 kHz = 72 000 samples, allow codec padding.
        assert!((70_000..=74_000).contains(&per_channel), "{per_channel}");
        let rms = (energy / total as f64).sqrt();
        assert!(rms > 0.05, "the 440 Hz tone must be audible: rms {rms}");
        let pos = r.position().as_seconds_f64();
        assert!((1.95..=2.05).contains(&pos), "{pos}");
    }

    #[test]
    fn silent_video_yields_no_audio_and_photos_have_no_video_stream() {
        let Some(cli) = cli() else { return };
        let mut r = AudioReader::open(&cli, &fixture("video_hlg_hevc.mp4"), Ticks::ZERO).unwrap();
        let mut buf = vec![0.0f32; 1024];
        assert_eq!(r.read(&mut buf).unwrap(), 0);
        let info = cli.probe(&fixture("audio_mono.wav")).unwrap();
        assert!(matches!(
            VideoReader::open(&cli, &fixture("audio_mono.wav"), &info, Ticks::ZERO, 64),
            Err(MediaError::Unsupported(_))
        ));
    }

    #[test]
    fn hdr_video_decodes_to_16_bit_samples_in_its_own_transfer() {
        let Some(cli) = cli() else { return };
        let path = fixture("video_hlg_hevc.mp4");
        let info = cli.probe(&path).unwrap();
        let mut r = VideoReader::open(&cli, &path, &info, Ticks::ZERO, 96).unwrap();
        assert!(r.is_hdr());
        let f = r.frame_at(Ticks::from_millis(500)).unwrap().unwrap();
        let VideoFrame::Hdr(img) = f else {
            panic!("HLG source decodes to 16-bit")
        };
        assert_eq!(img.transfer, ColorTransfer::Hlg);
        assert_eq!(img.rgb48.len(), (img.width * img.height * 3) as usize);
        // Real 16-bit values: more distinct levels than 8 bits could hold
        // in the (smooth) test pattern, and within range.
        let mut levels: Vec<u16> = img.rgb48.iter().map(|v| v >> 4).collect();
        levels.sort_unstable();
        levels.dedup();
        assert!(
            levels.len() > 256,
            "{} distinct 12-bit levels",
            levels.len()
        );
    }
}
