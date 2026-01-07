//! Error types for the package crate.

use miette::Diagnostic;
use thiserror::Error;

/// Errors that can occur during package operations.
#[derive(Error, Diagnostic, Debug)]
pub enum PackageError {
    #[error("Error while {action}: {source}")]
    #[diagnostic(code(soar_package::io))]
    IoError {
        action: String,
        source: std::io::Error,
    },

    #[error("Failed to read magic bytes")]
    #[diagnostic(code(soar_package::magic_bytes))]
    MagicBytesError,

    #[error("Failed to seek in file")]
    #[diagnostic(code(soar_package::seek))]
    SeekError,

    #[error(transparent)]
    #[diagnostic(code(soar_package::image))]
    ImageError(#[from] image::ImageError),

    #[error(transparent)]
    #[diagnostic(code(soar_package::appimage))]
    AppImageError(#[from] squishy::error::SquishyError),

    #[error("Configuration error: {0}")]
    #[diagnostic(code(soar_package::config))]
    ConfigError(String),

    #[error("{0}")]
    #[diagnostic(code(soar_package::custom))]
    Custom(String),
}

/// A specialized Result type for package operations.
pub type Result<T> = std::result::Result<T, PackageError>;

/// Extension trait for adding context to I/O errors.
pub trait ErrorContext<T> {
    /// Adds context to an error, describing what action was being performed.
    fn with_context<C>(self, context: C) -> Result<T>
    where
        C: FnOnce() -> String;
}

impl<T> ErrorContext<T> for std::io::Result<T> {
    fn with_context<C>(self, context: C) -> Result<T>
    where
        C: FnOnce() -> String,
    {
        self.map_err(|err| {
            PackageError::IoError {
                action: context(),
                source: err,
            }
        })
    }
}

impl From<soar_config::error::ConfigError> for PackageError {
    fn from(err: soar_config::error::ConfigError) -> Self {
        PackageError::ConfigError(err.to_string())
    }
}

impl From<soar_utils::error::FileSystemError> for PackageError {
    fn from(err: soar_utils::error::FileSystemError) -> Self {
        PackageError::Custom(err.to_string())
    }
}
