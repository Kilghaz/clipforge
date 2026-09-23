use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("io error on {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("unsupported format: {0}")]
    Unsupported(String),
    #[error("file is not valid {format}: {detail}")]
    Corrupt { format: String, detail: String },
    #[error("{tool} is not available: {detail}")]
    ToolMissing { tool: String, detail: String },
    #[error("{tool} failed: {detail}")]
    ToolFailed { tool: String, detail: String },
}

impl MediaError {
    pub(crate) fn io(path: &std::path::Path, source: std::io::Error) -> Self {
        MediaError::Io {
            path: path.to_path_buf(),
            source,
        }
    }

    pub(crate) fn corrupt(format: &str, detail: impl std::fmt::Display) -> Self {
        MediaError::Corrupt {
            format: format.to_owned(),
            detail: detail.to_string(),
        }
    }
}

pub type Result<T> = std::result::Result<T, MediaError>;
