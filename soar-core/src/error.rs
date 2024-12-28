use std::error::Error;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SoarError {
    #[error("System error: {0}")]
    Errno(#[from] nix::errno::Errno),

    #[error("Environment variable error: {0}")]
    VarError(#[from] std::env::VarError),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("System time error: {0}")]
    SystemTimeError(#[from] std::time::SystemTimeError),

    #[error("TOML serialization error: {0}")]
    TomlError(#[from] toml::ser::Error),

    #[error("SQLite database error: {0}")]
    RusqliteError(#[from] rusqlite::Error),

    #[error("Database operation failed: {0}")]
    DatabaseError(String),

    #[error("HTTP request error: {0}")]
    ReqwestError(#[from] reqwest::Error),

    #[error("Download failed: {0}")]
    DownloadError(#[from] soar_dl::error::DownloadError),

    #[error("{0}")]
    PlatformError(soar_dl::error::PlatformError),

    #[error("Squashy Error: {0}")]
    SquishyError(#[from] squishy::error::SquishyError),

    #[error("Image Error: {0}")]
    ImageError(#[from] image::error::ImageError),

    #[error("Package integration failed: {0}")]
    PackageIntegrationFailed(String),

    #[error("Package {0} not found")]
    PackageNotFound(String),

    #[error("Failed to fetch from remote source")]
    FailedToFetchRemote,

    #[error("Invalid path specified")]
    InvalidPath,

    #[error("Thread lock poison error")]
    PoisonError,

    #[error("Invalid checksum detected")]
    InvalidChecksum,

    #[error("Invalid configuration")]
    InvalidConfig,

    #[error("Configuration file already exists")]
    ConfigAlreadyExists,

    #[error("Invalid package query: {0}")]
    InvalidPackageQuery(String),

    #[error("{0}")]
    Custom(String),
}

impl SoarError {
    pub fn message(&self) -> String {
        self.to_string()
    }

    pub fn root_cause(&self) -> String {
        match self {
            Self::IoError(e) => format!(
                "Root cause: {}",
                e.source()
                    .map_or_else(|| e.to_string(), |source| source.to_string())
            ),
            Self::ReqwestError(e) => format!(
                "Root cause: {}",
                e.source()
                    .map_or_else(|| e.to_string(), |source| source.to_string())
            ),
            Self::RusqliteError(e) => format!(
                "Root cause: {}",
                e.source()
                    .map_or_else(|| e.to_string(), |source| source.to_string())
            ),
            _ => self.to_string(),
        }
    }
}

impl<T> From<std::sync::PoisonError<T>> for SoarError {
    fn from(_: std::sync::PoisonError<T>) -> Self {
        Self::PoisonError
    }
}

impl From<soar_dl::error::PlatformError> for SoarError {
    fn from(value: soar_dl::error::PlatformError) -> Self {
        Self::PlatformError(value)
    }
}
