//! Which video encoders the bundled ffmpeg offers, and which one to use.

use std::path::Path;
use std::process::Command;

use crate::plan::Codec;

/// A concrete ffmpeg encoder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Encoder {
    pub name: &'static str,
    pub hardware: bool,
}

/// Preference order per codec; first available wins. Hardware first
/// (fast, licence handled by the OS vendor), software as fallback.
const H264_PREFERENCE: &[Encoder] = &[
    Encoder {
        name: "h264_videotoolbox",
        hardware: true,
    },
    Encoder {
        name: "h264_nvenc",
        hardware: true,
    },
    Encoder {
        name: "h264_qsv",
        hardware: true,
    },
    Encoder {
        name: "h264_amf",
        hardware: true,
    },
    Encoder {
        name: "h264_mf",
        hardware: true,
    },
    Encoder {
        name: "libx264",
        hardware: false,
    },
    Encoder {
        name: "libopenh264",
        hardware: false,
    },
];

const HEVC_PREFERENCE: &[Encoder] = &[
    Encoder {
        name: "hevc_videotoolbox",
        hardware: true,
    },
    Encoder {
        name: "hevc_nvenc",
        hardware: true,
    },
    Encoder {
        name: "hevc_qsv",
        hardware: true,
    },
    Encoder {
        name: "hevc_amf",
        hardware: true,
    },
    Encoder {
        name: "hevc_mf",
        hardware: true,
    },
    Encoder {
        name: "libx265",
        hardware: false,
    },
];

/// The set of encoder names an ffmpeg binary reports.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EncoderCatalog {
    names: Vec<String>,
}

impl EncoderCatalog {
    /// Runs `ffmpeg -encoders` and parses the list.
    pub fn probe(ffmpeg: &Path) -> std::io::Result<EncoderCatalog> {
        let out = Command::new(ffmpeg)
            .args(["-hide_banner", "-encoders"])
            .output()?;
        Ok(Self::parse(&String::from_utf8_lossy(&out.stdout)))
    }

    /// Parses `ffmpeg -encoders` output. Lines look like
    /// ` V....D libx264              libx264 H.264 / AVC ...`.
    #[must_use]
    pub fn parse(text: &str) -> EncoderCatalog {
        let mut names = Vec::new();
        let mut in_list = false;
        for line in text.lines() {
            if line.trim_start().starts_with("------") {
                in_list = true;
                continue;
            }
            if !in_list {
                continue;
            }
            let mut parts = line.split_whitespace();
            let Some(flags) = parts.next() else { continue };
            if !flags.starts_with('V') || flags.len() < 6 {
                continue;
            }
            if let Some(name) = parts.next() {
                names.push(name.to_owned());
            }
        }
        EncoderCatalog { names }
    }

    /// A catalog from explicit names (tests, overrides).
    #[must_use]
    pub fn from_names<I: IntoIterator<Item = S>, S: Into<String>>(names: I) -> EncoderCatalog {
        EncoderCatalog {
            names: names.into_iter().map(Into::into).collect(),
        }
    }

    #[must_use]
    pub fn has(&self, name: &str) -> bool {
        self.names.iter().any(|n| n == name)
    }

    /// Best available encoder for `codec`, honouring `prefer_hardware`.
    #[must_use]
    pub fn pick(&self, codec: Codec, prefer_hardware: bool) -> Option<Encoder> {
        let prefs = match codec {
            Codec::H264 => H264_PREFERENCE,
            Codec::Hevc => HEVC_PREFERENCE,
        };
        let pass = |hw: bool| {
            prefs
                .iter()
                .find(|e| e.hardware == hw && self.has(e.name))
                .cloned()
        };
        if prefer_hardware {
            pass(true).or_else(|| pass(false))
        } else {
            pass(false).or_else(|| pass(true))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "Encoders:\n V..... = Video\n A..... = Audio\n ------\n V....D libx264              libx264 H.264 / AVC\n V....D h264_videotoolbox    VideoToolbox H.264 Encoder\n V....D libx265              libx265 H.265 / HEVC\n A....D aac                  AAC\n S..... srt                  SubRip\n";

    #[test]
    fn parses_video_encoders_only() {
        let c = EncoderCatalog::parse(SAMPLE);
        assert!(c.has("libx264") && c.has("h264_videotoolbox") && c.has("libx265"));
        assert!(!c.has("aac") && !c.has("srt") && !c.has("Video"));
    }

    #[test]
    fn picks_hardware_first_then_software() {
        let c = EncoderCatalog::parse(SAMPLE);
        assert_eq!(c.pick(Codec::H264, true).unwrap().name, "h264_videotoolbox");
        assert_eq!(c.pick(Codec::H264, false).unwrap().name, "libx264");
        assert_eq!(
            c.pick(Codec::Hevc, true).unwrap().name,
            "libx265",
            "falls back to software"
        );
        assert_eq!(
            EncoderCatalog::from_names(["h264_nvenc"])
                .pick(Codec::H264, false)
                .unwrap()
                .name,
            "h264_nvenc"
        );
        assert_eq!(EncoderCatalog::default().pick(Codec::H264, true), None);
    }
}
