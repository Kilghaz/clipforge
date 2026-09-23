use clipforge_core::MediaId;

#[derive(Debug, thiserror::Error)]
pub enum LibraryError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("catalogue schema version {found} is newer than this build supports ({supported})")]
    SchemaTooNew { found: u32, supported: u32 },
    #[error("media {0} not found")]
    NotFound(MediaId),
    #[error("invalid data in catalogue: {0}")]
    Corrupt(String),
}

pub type Result<T> = std::result::Result<T, LibraryError>;
