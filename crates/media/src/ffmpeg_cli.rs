//! Backend that shells out to `ffprobe` and `ffmpeg`.
//!
//! Used for video and audio metadata, for representative video frames, and
//! as the decoder of last resort for photo formats the `image` crate cannot
//! read (HEIC/HEIF, AVIF, camera RAW previews).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use clipforge_core::{FrameRate, Ticks};
use serde::Deserialize;

use crate::error::{MediaError, Result};
use crate::exif;
use crate::image_backend::scale_and_rotate;
use crate::info::{MediaInfo, Rotation};
use crate::kind::{ColorTransfer, MediaKind};
use crate::probe::{DecodedImage, Prober, StillDecoder};

/// Where the executables live.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FfmpegLocation {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}

impl FfmpegLocation {
    /// Looks in `CLIPFORGE_FFMPEG_DIR`, then next to the running executable,
    /// then on `PATH`. Returns `None` if `ffprobe` cannot be executed.
    #[must_use]
    pub fn discover() -> Option<FfmpegLocation> {
        let exe =
            |dir: &Path, name: &str| dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Ok(dir) = std::env::var("CLIPFORGE_FFMPEG_DIR") {
            candidates.push(PathBuf::from(dir));
        }
        if let Ok(current) = std::env::current_exe()
            && let Some(dir) = current.parent()
        {
            candidates.push(dir.to_path_buf());
            candidates.push(dir.join("ffmpeg"));
        }
        for dir in candidates {
            let loc = FfmpegLocation {
                ffmpeg: exe(&dir, "ffmpeg"),
                ffprobe: exe(&dir, "ffprobe"),
            };
            if loc.works() {
                return Some(loc);
            }
        }
        let on_path = FfmpegLocation {
            ffmpeg: PathBuf::from("ffmpeg"),
            ffprobe: PathBuf::from("ffprobe"),
        };
        on_path.works().then_some(on_path)
    }

    fn works(&self) -> bool {
        Command::new(&self.ffprobe)
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }
}

/// Prober and still decoder driving the ffmpeg executables.
#[derive(Clone, Debug)]
pub struct FfmpegCli {
    location: FfmpegLocation,
    /// Upper bound for one invocation; a hung tool must not hang the app.
    pub timeout: Duration,
}

impl FfmpegCli {
    #[must_use]
    pub fn new(location: FfmpegLocation) -> Self {
        FfmpegCli {
            location,
            timeout: Duration::from_secs(30),
        }
    }

    /// Discovers the executables; `None` if they are not installed.
    #[must_use]
    pub fn discover() -> Option<Self> {
        FfmpegLocation::discover().map(Self::new)
    }

    #[must_use]
    pub fn location(&self) -> &FfmpegLocation {
        &self.location
    }

    fn run(&self, program: &Path, args: &[&str], input: &Path) -> Result<Vec<u8>> {
        let tool = program
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("ffmpeg")
            .to_owned();
        let mut cmd = Command::new(program);
        cmd.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let child = cmd.spawn().map_err(|e| MediaError::ToolMissing {
            tool: tool.clone(),
            detail: e.to_string(),
        })?;
        let output = wait_with_timeout(child, self.timeout, &tool)?;
        if output.status.success() {
            Ok(output.stdout)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let detail = stderr
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("no output")
                .to_owned();
            Err(MediaError::ToolFailed {
                tool,
                detail: format!("{detail} ({})", input.display()),
            })
        }
    }

    fn ffprobe_json(&self, path: &Path) -> Result<ProbeOutput> {
        let p = path.to_string_lossy();
        let out = self.run(
            &self.location.ffprobe,
            &[
                "-v",
                "error",
                "-print_format",
                "json",
                "-show_format",
                "-show_streams",
                "-i",
                &p,
            ],
            path,
        )?;
        serde_json::from_slice(&out).map_err(|e| MediaError::corrupt("ffprobe json", e))
    }
}

fn wait_with_timeout(
    mut child: std::process::Child,
    timeout: Duration,
    tool: &str,
) -> Result<std::process::Output> {
    // Read pipes on helper threads to avoid deadlock on large outputs, and
    // poll for exit so a stuck process can be killed.
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let out_thread = std::thread::spawn(move || read_all(stdout));
    let err_thread = std::thread::spawn(move || read_all(stderr));
    let start = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(MediaError::ToolFailed {
                    tool: tool.to_owned(),
                    detail: format!("timed out after {timeout:?}"),
                });
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(e) => {
                return Err(MediaError::ToolFailed {
                    tool: tool.to_owned(),
                    detail: e.to_string(),
                });
            }
        }
    };
    let stdout = out_thread.join().unwrap_or_default();
    let stderr = err_thread.join().unwrap_or_default();
    Ok(std::process::Output {
        status,
        stdout,
        stderr,
    })
}

fn read_all<R: std::io::Read>(r: Option<R>) -> Vec<u8> {
    let mut buf = Vec::new();
    if let Some(mut r) = r {
        let _ = r.read_to_end(&mut buf);
    }
    buf
}

#[derive(Debug, Deserialize)]
struct ProbeOutput {
    #[serde(default)]
    streams: Vec<Stream>,
    format: Option<Format>,
}

#[derive(Debug, Deserialize)]
struct Format {
    format_name: Option<String>,
    duration: Option<String>,
    #[serde(default)]
    tags: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct Stream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    duration: Option<String>,
    r_frame_rate: Option<String>,
    avg_frame_rate: Option<String>,
    nb_frames: Option<String>,
    color_transfer: Option<String>,
    color_primaries: Option<String>,
    disposition: Option<Disposition>,
    #[serde(default)]
    tags: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    side_data_list: Vec<SideData>,
}

#[derive(Debug, Deserialize, Default)]
struct Disposition {
    #[serde(default)]
    attached_pic: u8,
}

#[derive(Debug, Deserialize)]
struct SideData {
    side_data_type: Option<String>,
    rotation: Option<f64>,
}

/// Pure translation of ffprobe output into [`MediaInfo`]; tested without
/// ffmpeg via canned JSON.
fn interpret(out: &ProbeOutput, path: &Path) -> Result<MediaInfo> {
    let video = out.streams.iter().find(|s| {
        s.codec_type.as_deref() == Some("video")
            && s.disposition.as_ref().is_none_or(|d| d.attached_pic == 0)
    });
    let audio = out
        .streams
        .iter()
        .find(|s| s.codec_type.as_deref() == Some("audio"));
    let format_name = out
        .format
        .as_ref()
        .and_then(|f| f.format_name.as_deref())
        .unwrap_or("");

    let duration_secs = out
        .format
        .as_ref()
        .and_then(|f| f.duration.as_deref())
        .or_else(|| video.and_then(|v| v.duration.as_deref()))
        .or_else(|| audio.and_then(|a| a.duration.as_deref()))
        .and_then(|d| d.parse::<f64>().ok());

    let creation = out
        .format
        .as_ref()
        .and_then(|f| {
            f.tags
                .get("creation_time")
                .or_else(|| f.tags.get("com.apple.quicktime.creationdate"))
        })
        .or_else(|| video.and_then(|v| v.tags.get("creation_time")))
        .and_then(|s| exif::parse_iso8601_ms(s));

    match (video, audio) {
        (Some(v), _) => {
            let is_still = is_still_image(format_name, v);
            let rotation = stream_rotation(v);
            let exif_summary = if is_still {
                exif::read(path)
            } else {
                exif::ExifSummary::default()
            };
            let frame_rate = if is_still { None } else { parse_frame_rate(v) };
            Ok(MediaInfo {
                kind: if is_still {
                    MediaKind::Photo
                } else {
                    MediaKind::Video
                },
                width: v.width,
                height: v.height,
                rotation: if rotation == Rotation::None {
                    exif_summary.rotation
                } else {
                    rotation
                },
                duration: if is_still {
                    None
                } else {
                    duration_secs.map(Ticks::from_seconds_f64)
                },
                frame_rate,
                has_audio: audio.is_some(),
                transfer: transfer_from(v.color_transfer.as_deref(), v.color_primaries.as_deref()),
                captured_at_ms: creation.or(exif_summary.captured_at_ms),
                codec: v
                    .codec_name
                    .clone()
                    .unwrap_or_else(|| format_name.to_owned()),
            })
        }
        (None, Some(a)) => Ok(MediaInfo {
            kind: MediaKind::Audio,
            width: None,
            height: None,
            rotation: Rotation::None,
            duration: duration_secs.map(Ticks::from_seconds_f64),
            frame_rate: None,
            has_audio: true,
            transfer: ColorTransfer::Sdr,
            captured_at_ms: creation,
            codec: a
                .codec_name
                .clone()
                .unwrap_or_else(|| format_name.to_owned()),
        }),
        (None, None) => Err(MediaError::corrupt(format_name, "no audio or video stream")),
    }
}

fn is_still_image(format_name: &str, v: &Stream) -> bool {
    let image_formats = [
        "image2",
        "png_pipe",
        "jpeg_pipe",
        "webp_pipe",
        "tiff_pipe",
        "gif",
        "heif",
        "avif",
    ];
    if image_formats
        .iter()
        .any(|f| format_name.split(',').any(|part| part == *f))
    {
        return true;
    }
    matches!(
        v.codec_name.as_deref(),
        Some("mjpeg" | "png" | "tiff" | "webp" | "bmp")
    ) && v.nb_frames.as_deref().is_none_or(|n| n == "1")
}

fn stream_rotation(v: &Stream) -> Rotation {
    if let Some(sd) = v
        .side_data_list
        .iter()
        .find(|s| s.side_data_type.as_deref() == Some("Display Matrix"))
        && let Some(r) = sd.rotation
    {
        // ffprobe reports counter-clockwise degrees in the display matrix.
        #[allow(clippy::cast_possible_truncation)]
        return Rotation::from_degrees(-(r.round() as i32));
    }
    v.tags
        .get("rotate")
        .and_then(|r| r.parse::<i32>().ok())
        .map_or(Rotation::None, Rotation::from_degrees)
}

fn parse_frame_rate(v: &Stream) -> Option<FrameRate> {
    let raw = v
        .avg_frame_rate
        .as_deref()
        .filter(|r| *r != "0/0")
        .or(v.r_frame_rate.as_deref())?;
    let (n, d) = raw.split_once('/').unwrap_or((raw, "1"));
    let n: u32 = n.parse().ok()?;
    let d: u32 = d.parse().ok()?;
    FrameRate::new(n, d).or_else(|| {
        // Not an exact flick divisor (e.g. variable frame rate averages):
        // snap to the nearest standard rate for display purposes.
        let fps = f64::from(n) / f64::from(d);
        [
            FrameRate::FPS_23_976,
            FrameRate::FPS_24,
            FrameRate::FPS_25,
            FrameRate::FPS_29_97,
            FrameRate::FPS_30,
            FrameRate::FPS_50,
            FrameRate::FPS_59_94,
            FrameRate::FPS_60,
        ]
        .into_iter()
        .min_by(|a, b| {
            (a.as_f64() - fps)
                .abs()
                .total_cmp(&(b.as_f64() - fps).abs())
        })
    })
}

fn transfer_from(transfer: Option<&str>, primaries: Option<&str>) -> ColorTransfer {
    match transfer {
        Some("arib-std-b67") => ColorTransfer::Hlg,
        Some("smpte2084") => ColorTransfer::Pq,
        _ => {
            let _ = primaries;
            ColorTransfer::Sdr
        }
    }
}

impl Prober for FfmpegCli {
    fn probe(&self, path: &Path) -> Result<MediaInfo> {
        if !path.exists() {
            return Err(MediaError::io(
                path,
                std::io::Error::from(std::io::ErrorKind::NotFound),
            ));
        }
        let out = self.ffprobe_json(path)?;
        interpret(&out, path)
    }
}

impl StillDecoder for FfmpegCli {
    /// Videos: a frame at 10 % of the duration (skips black lead-ins).
    /// Stills: the image itself. ffmpeg's autorotate is disabled so that
    /// rotation is applied uniformly by us from the probed metadata.
    fn decode_scaled(&self, path: &Path, max_edge: u32) -> Result<DecodedImage> {
        let info = self.probe(path)?;
        if info.kind == MediaKind::Audio {
            return Err(MediaError::Unsupported(format!(
                "{} has no picture",
                path.display()
            )));
        }
        let p = path.to_string_lossy();
        let scale = format!(
            "scale='min({max_edge},iw)':'min({max_edge},ih)':force_original_aspect_ratio=decrease"
        );
        let mut args: Vec<String> = vec![
            "-v".into(),
            "error".into(),
            "-nostdin".into(),
            "-noautorotate".into(),
        ];
        if info.kind == MediaKind::Video
            && let Some(d) = info.duration
        {
            let seek = (d.as_seconds_f64() * 0.1).min(10.0);
            args.extend(["-ss".into(), format!("{seek:.3}")]);
        }
        args.extend([
            "-i".into(),
            p.into_owned(),
            "-frames:v".into(),
            "1".into(),
            "-vf".into(),
            scale,
            "-f".into(),
            "image2pipe".into(),
            "-vcodec".into(),
            "png".into(),
            "-".into(),
        ]);
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let png = self.run(&self.location.ffmpeg, &arg_refs, path)?;
        let img = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
            .map_err(|e| MediaError::corrupt("ffmpeg frame", e))?;
        scale_and_rotate(img, max_edge, info.rotation)
    }
}

#[cfg(test)]
#[allow(clippy::print_stderr)]
mod tests {
    use super::*;
    use crate::test_support::fixture;

    fn parse(json: &str) -> ProbeOutput {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn interprets_hlg_video_json() {
        let out = parse(
            r#"{"streams":[{"codec_type":"video","codec_name":"hevc","width":3840,"height":2160,
                "r_frame_rate":"30000/1001","avg_frame_rate":"30000/1001","color_transfer":"arib-std-b67",
                "color_primaries":"bt2020","tags":{"creation_time":"2024-05-01T10:00:00.000000Z"}},
                {"codec_type":"audio","codec_name":"aac"}],
                "format":{"format_name":"mov,mp4,m4a,3gp,3g2,mj2","duration":"12.500000","tags":{}}}"#,
        );
        let info = interpret(&out, Path::new("x.mov")).unwrap();
        assert_eq!(info.kind, MediaKind::Video);
        assert_eq!((info.width, info.height), (Some(3840), Some(2160)));
        assert_eq!(info.frame_rate, Some(FrameRate::FPS_29_97));
        assert_eq!(info.transfer, ColorTransfer::Hlg);
        assert!(info.has_audio);
        assert_eq!(info.duration, Some(Ticks::from_millis(12_500)));
        assert_eq!(info.captured_at_ms, Some(1_714_557_600_000));
        assert_eq!(info.codec, "hevc");
    }

    #[test]
    fn display_matrix_rotation_is_inverted_and_tag_fallback_works() {
        let out = parse(
            r#"{"streams":[{"codec_type":"video","codec_name":"h264","width":1920,"height":1080,
                "side_data_list":[{"side_data_type":"Display Matrix","rotation":-90.0}]}],
                "format":{"format_name":"mov","duration":"1"}}"#,
        );
        assert_eq!(
            interpret(&out, Path::new("x")).unwrap().rotation,
            Rotation::Cw90
        );
        let out = parse(
            r#"{"streams":[{"codec_type":"video","codec_name":"h264","width":1920,"height":1080,"tags":{"rotate":"270"}}],
                "format":{"format_name":"mov","duration":"1"}}"#,
        );
        assert_eq!(
            interpret(&out, Path::new("x")).unwrap().rotation,
            Rotation::Cw270
        );
    }

    #[test]
    fn audio_only_and_empty_inputs() {
        let out = parse(
            r#"{"streams":[{"codec_type":"audio","codec_name":"aac","duration":"3.0"}],"format":{"format_name":"mov,mp4,m4a"}}"#,
        );
        let info = interpret(&out, Path::new("x.m4a")).unwrap();
        assert_eq!(info.kind, MediaKind::Audio);
        assert_eq!(info.duration, Some(Ticks::from_seconds(3)));
        assert!(info.has_audio);
        let out = parse(r#"{"streams":[],"format":{"format_name":"data"}}"#);
        assert!(matches!(
            interpret(&out, Path::new("x")),
            Err(MediaError::Corrupt { .. })
        ));
    }

    #[test]
    fn heif_and_image_pipes_are_photos() {
        let out = parse(
            r#"{"streams":[{"codec_type":"video","codec_name":"hevc","width":4032,"height":3024,"nb_frames":"1"}],
                "format":{"format_name":"heif","duration":"0.04"}}"#,
        );
        let info = interpret(&out, Path::new("x.heic")).unwrap();
        assert_eq!(info.kind, MediaKind::Photo);
        assert_eq!(info.duration, None);
        assert_eq!(info.frame_rate, None);
        let out = parse(
            r#"{"streams":[{"codec_type":"video","codec_name":"mjpeg","width":10,"height":10}],
                "format":{"format_name":"jpeg_pipe"}}"#,
        );
        assert_eq!(
            interpret(&out, Path::new("x.jpg")).unwrap().kind,
            MediaKind::Photo
        );
    }

    #[test]
    fn cover_art_is_not_the_video_stream() {
        let out = parse(
            r#"{"streams":[{"codec_type":"video","codec_name":"mjpeg","width":600,"height":600,"disposition":{"attached_pic":1}},
                {"codec_type":"audio","codec_name":"mp3","duration":"200"}],"format":{"format_name":"mp3","duration":"200"}}"#,
        );
        assert_eq!(
            interpret(&out, Path::new("x.mp3")).unwrap().kind,
            MediaKind::Audio
        );
    }

    #[test]
    fn odd_frame_rates_snap_to_standard() {
        let s = Stream {
            codec_type: None,
            codec_name: None,
            width: None,
            height: None,
            duration: None,
            r_frame_rate: Some("2997/100".into()),
            avg_frame_rate: Some("0/0".into()),
            nb_frames: None,
            color_transfer: None,
            color_primaries: None,
            disposition: None,
            tags: Default::default(),
            side_data_list: vec![],
        };
        assert_eq!(parse_frame_rate(&s), Some(FrameRate::FPS_29_97));
    }

    fn cli() -> Option<FfmpegCli> {
        let cli = FfmpegCli::discover();
        if cli.is_none() {
            eprintln!("ffmpeg not installed; skipping integration test");
        }
        cli
    }

    #[test]
    fn probes_real_fixtures() {
        let Some(cli) = cli() else { return };
        let sdr = cli.probe(&fixture("video_sdr_h264.mp4")).unwrap();
        assert_eq!(sdr.kind, MediaKind::Video);
        assert_eq!((sdr.width, sdr.height), (Some(320), Some(180)));
        assert_eq!(sdr.frame_rate, Some(FrameRate::FPS_25));
        assert!(sdr.has_audio);
        assert_eq!(sdr.transfer, ColorTransfer::Sdr);
        let d = sdr.duration.unwrap().as_seconds_f64();
        assert!((1.9..=2.1).contains(&d), "{d}");

        let hlg = cli.probe(&fixture("video_hlg_hevc.mp4")).unwrap();
        assert_eq!(hlg.transfer, ColorTransfer::Hlg);
        assert!(!hlg.has_audio);
        assert_eq!(hlg.codec, "hevc");

        let rot = cli.probe(&fixture("video_rotated_90.mov")).unwrap();
        assert_eq!(rot.rotation, Rotation::Cw90);
        assert_eq!(rot.display_size(), Some((180, 320)));

        let audio = cli.probe(&fixture("audio_stereo.m4a")).unwrap();
        assert_eq!(audio.kind, MediaKind::Audio);
        let wav = cli.probe(&fixture("audio_mono.wav")).unwrap();
        assert_eq!(wav.kind, MediaKind::Audio);
        assert!((0.45..=0.55).contains(&wav.duration.unwrap().as_seconds_f64()));

        let jpg = cli.probe(&fixture("photo_landscape.jpg")).unwrap();
        assert_eq!(jpg.kind, MediaKind::Photo);
        assert_eq!((jpg.width, jpg.height), (Some(320), Some(180)));
    }

    #[test]
    fn real_failures_are_errors_not_panics() {
        let Some(cli) = cli() else { return };
        assert!(matches!(
            cli.probe(&fixture("empty.mp4")),
            Err(MediaError::ToolFailed { .. })
        ));
        // ffprobe accepts a bare JPEG header without decoding; decoding must fail.
        assert!(matches!(
            cli.decode_scaled(&fixture("broken_truncated.jpg"), 64),
            Err(MediaError::ToolFailed { .. })
        ));
        assert!(matches!(
            cli.probe(Path::new("/nope/missing.mp4")),
            Err(MediaError::Io { .. })
        ));
    }

    #[test]
    fn audio_has_no_still() {
        let Some(cli) = cli() else { return };
        assert!(matches!(
            cli.decode_scaled(&fixture("audio_mono.wav"), 64),
            Err(MediaError::Unsupported(_))
        ));
    }

    #[test]
    fn extracts_scaled_video_frame() {
        let Some(cli) = cli() else { return };
        let frame = cli
            .decode_scaled(&fixture("video_sdr_h264.mp4"), 96)
            .unwrap();
        assert_eq!((frame.width, frame.height), (96, 54));
        let rotated = cli
            .decode_scaled(&fixture("video_rotated_90.mov"), 96)
            .unwrap();
        assert_eq!(
            (rotated.width, rotated.height),
            (54, 96),
            "rotation applied"
        );
        let avg = frame.average_rgb();
        assert!(
            avg.iter().any(|c| *c > 10),
            "test pattern is not black: {avg:?}"
        );
    }
}
