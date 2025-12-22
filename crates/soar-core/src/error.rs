//! Error types for soar-core.

use std::error::Error;

use miette::Diagnostic;
use soar_config::error::ConfigError;
use soar_utils::error::{FileSystemError, HashError, PathError};
use thiserror::Error;

/// Core error type for soar package manager operations.
#[derive(Error, Diagnostic, Debug)]
pub enum SoarError {
    #[error(transparent)]
    #[diagnostic(transparent)]
    Config(#[from] ConfigError),

    #[error("System error: {0}")]
    #[diagnostic(
        code(soar::system),
        help("Check system permissions and resources")
    )]
    Errno(#[from] nix::errno::Errno),

    #[error("Environment variable '{0}' not set")]
    #[diagnostic(
        code(soar::env_var),
        help("Set the required environment variable before running")
    )]
    VarError(#[from] std::env::VarError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    FileSystemError(#[from] FileSystemError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    HashError(#[from] HashError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    PathError(#[from] PathError),

    #[error("IO error while {action}")]
    #[diagnostic(
        code(soar::io),
        help("Check file permissions and disk space")
    )]
    IoError {
        action: String,
        #[source]
        source: std::io::Error,
    },

    #[error("System time error: {0}")]
    #[diagnostic(code(soar::time))]
    SystemTimeError(#[from] std::time::SystemTimeError),

    #[error("TOML serialization error: {0}")]
    #[diagnostic(
        code(soar::toml),
        help("Check your configuration syntax")
    )]
    TomlError(#[from] toml::ser::Error),

    #[error("Database operation failed: {0}")]
    #[diagnostic(
        code(soar::database),
        help("Try running 'soar sync' to refresh the database")
    )]
    DatabaseError(String),

    #[error("HTTP request failed")]
    #[diagnostic(
        code(soar::network),
        help("Check your internet connection and try again")
    )]
    UreqError(#[from] ureq::Error),

    #[error(transparent)]
    #[diagnostic(transparent)]
    DownloadError(#[from] soar_dl::error::DownloadError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    PackageError(#[from] soar_package::PackageError),

    #[error("Package integration failed: {0}")]
    #[diagnostic(
        code(soar::integration),
        help("Check if the package format is supported")
    )]
    PackageIntegrationFailed(String),

    #[error("Package '{0}' not found")]
    #[diagnostic(
        code(soar::package_not_found),
        help("Run 'soar sync' to update package list, or check the package name")
    )]
    PackageNotFound(String),

    #[error("Failed to fetch from remote source: {0}")]
    #[diagnostic(
        code(soar::fetch),
        help("Check your internet connection and repository URL")
    )]
    FailedToFetchRemote(String),

    #[error("Invalid path specified")]
    #[diagnostic(
        code(soar::invalid_path),
        help("Provide a valid file or directory path")
    )]
    InvalidPath,

    #[error("Thread lock poison error")]
    #[diagnostic(
        code(soar::poison),
        help("This is an internal error, please report it")
    )]
    PoisonError,

    #[error("Invalid checksum detected")]
    #[diagnostic(
        code(soar::checksum),
        help("The downloaded file may be corrupted. Try downloading again.")
    )]
    InvalidChecksum,

    #[error("Invalid package query: {0}")]
    #[diagnostic(
        code(soar::invalid_query),
        help("Use format: name#pkg_id@version:repo (e.g., 'curl', 'curl#bin', 'curl@8.0.0')")
    )]
    InvalidPackageQuery(String),

    #[error("{0}")]
    #[diagnostic(code(soar::error))]
    Custom(String),

    #[error("{0}")]
    #[diagnostic(code(soar::warning), severity(warning))]
    Warning(String),

    #[error("Regex compilation error: {0}")]
    #[diagnostic(
        code(soar::regex),
        help("Check your regex pattern syntax")
    )]
    RegexError(#[from] regex::Error),
}

impl SoarError {
    pub fn message(&self) -> String {
        self.to_string()
    }

    pub fn root_cause(&self) -> String {
        match self {
            Self::UreqError(e) => {
                format!(
                    "Root cause: {}",
                    e.source()
                        .map_or_else(|| e.to_string(), |source| source.to_string())
                )
            }
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

/// Trait for adding context to IO errors.
pub trait ErrorContext<T> {
    fn with_context<C>(self, context: C) -> std::result::Result<T, SoarError>
    where
        C: FnOnce() -> String;
}

impl<T> ErrorContext<T> for std::io::Result<T> {
    fn with_context<C>(self, context: C) -> std::result::Result<T, SoarError>
    where
        C: FnOnce() -> String,
    {
        self.map_err(|err| SoarError::IoError {
            action: context(),
            source: err,
        })
    }
}
