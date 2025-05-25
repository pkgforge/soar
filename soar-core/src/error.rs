use std::error::Error;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Invalid configuration")]
    InvalidConfig,

    #[error("Configuration file already exists")]
    ConfigAlreadyExists,

    #[error("Invalid profile: {0}")]
    InvalidProfile(String),

    #[error("TOML deserialization error: {0}")]
    TomlDeError(#[from] toml::de::Error),

    #[error("TOML serialization error: {0}")]
    TomlSerError(#[from] toml::ser::Error),

    #[error("IO error reading config: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Default profile '{0}' does not exist")]
    MissingDefaultProfile(String),

    #[error("Reserved repository name 'local' is not allowed")]
    ReservedRepositoryName,

    #[error("Duplicate repository name '{0}'")]
    DuplicateRepositoryName(String),

    #[error("Profile '{0}' does not exist")]
    MissingProfile(String),

    #[error("{0}")]
    Custom(String),
}

#[derive(Error, Debug)]
pub enum SoarError {
    #[error(transparent)]
    Config(#[from] ConfigError),

    #[error("System error: {0}")]
    Errno(#[from] nix::errno::Errno),

    #[error("Environment variable error: {0}")]
    VarError(#[from] std::env::VarError),

    #[error("IO error while {action}: {source}")]
    IoError {
        action: String,
        source: std::io::Error,
    },

    #[error("System time error: {0}")]
    SystemTimeError(#[from] std::time::SystemTimeError),

    #[error("TOML serialization error: {0}")]
    TomlError(#[from] toml::ser::Error),

    #[error("SQLite database error: {0}")]
    RusqliteError(#[from] rusqlite::Error),

    #[error("Database operation failed: {0}")]
    DatabaseError(String),

    #[error("HTTP request error: {0:?}")]
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

    #[error("Failed to fetch from remote source: {0}")]
    FailedToFetchRemote(String),

    #[error("Invalid path specified")]
    InvalidPath,

    #[error("Thread lock poison error")]
    PoisonError,

    #[error("Invalid checksum detected")]
    InvalidChecksum,

    #[error("Configuration file already exists")]
    ConfigAlreadyExists,

    #[error("Invalid package query: {0}")]
    InvalidPackageQuery(String),

    #[error("{0}")]
    Custom(String),

    #[error("Invalid profile: {0}")]
    InvalidProfile(String),

    #[error("{0}")]
    Warning(String),
}

impl SoarError {
    pub fn message(&self) -> String {
        self.to_string()
    }

    pub fn root_cause(&self) -> String {
        match self {
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
            Self::Config(err) => err.to_string(),
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

pub trait ErrorContext<T> {
    fn with_context<C>(self, context: C) -> Result<T, SoarError>
    where
        C: FnOnce() -> String;
}

impl<T> ErrorContext<T> for std::io::Result<T> {
    fn with_context<C>(self, context: C) -> Result<T, SoarError>
    where
        C: FnOnce() -> String,
    {
        self.map_err(|err| SoarError::IoError {
            action: context(),
            source: err,
        })
    }
}
