//! Parsing ffmpeg's `-progress` output: blocks of `key=value` lines ending
//! with `progress=continue` or `progress=end`.

/// One complete progress block.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProgressLine {
    pub frame: Option<u64>,
    pub out_time_us: Option<i64>,
    pub total_size: Option<u64>,
    pub speed: Option<String>,
    pub ended: bool,
}

/// Accumulates lines and emits a [`ProgressLine`] per block.
#[derive(Debug, Default)]
pub struct ProgressParser {
    current: ProgressLine,
}

impl ProgressParser {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds one line. Returns the finished block when a `progress=` line
    /// closes it.
    pub fn feed(&mut self, line: &str) -> Option<ProgressLine> {
        let line = line.trim();
        let (key, value) = line.split_once('=')?;
        let value = value.trim();
        match key.trim() {
            "frame" => self.current.frame = value.parse().ok(),
            "out_time_us" | "out_time_ms" => {
                // ffmpeg < 7 wrote microseconds under out_time_ms.
                self.current.out_time_us = value.parse().ok();
            }
            "total_size" => self.current.total_size = value.parse().ok(),
            "speed" => self.current.speed = Some(value.to_owned()),
            "progress" => {
                let mut done = std::mem::take(&mut self.current);
                done.ended = value == "end";
                return Some(done);
            }
            _ => {}
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_blocks() {
        let mut p = ProgressParser::new();
        let text = "frame=10\nfps=0.0\nout_time_us=333333\nout_time_ms=333333\nout_time=00:00:00.333333\ntotal_size=1024\nspeed=2.5x\nprogress=continue\nframe=30\nout_time_us=1000000\nprogress=end\n";
        let blocks: Vec<_> = text.lines().filter_map(|l| p.feed(l)).collect();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].frame, Some(10));
        assert_eq!(blocks[0].out_time_us, Some(333_333));
        assert_eq!(blocks[0].total_size, Some(1024));
        assert_eq!(blocks[0].speed.as_deref(), Some("2.5x"));
        assert!(!blocks[0].ended);
        assert_eq!(blocks[1].frame, Some(30));
        assert!(blocks[1].ended);
        assert_eq!(
            blocks[1].total_size, None,
            "blocks do not leak into each other"
        );
    }

    #[test]
    fn ignores_garbage() {
        let mut p = ProgressParser::new();
        assert_eq!(p.feed("no equals sign"), None);
        assert_eq!(p.feed("frame=notanumber"), None);
        let b = p.feed("progress=continue").unwrap();
        assert_eq!(b.frame, None);
    }
}
