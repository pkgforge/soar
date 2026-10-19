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

    #[error(transparent)]
    #[diagnostic(transparent)]
    Config(#[from] soar_config::error::ConfigError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    FileSystem(#[from] soar_utils::error::FileSystemError),

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

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::*;

    #[test]
    fn a_filesystem_error_keeps_its_cause_and_help() {
        let err = PackageError::from(soar_utils::error::FileSystemError::ReadFile {
            path: "/x/y".into(),
            source: std::io::Error::from(std::io::ErrorKind::PermissionDenied),
        });

        assert_eq!(err.to_string(), "Failed to read file '/x/y'");
        assert_eq!(
            err.source().map(ToString::to_string).as_deref(),
            Some("permission denied")
        );
        assert!(err.help().is_some());
        assert_eq!(
            err.code().map(|code| code.to_string()).as_deref(),
            Some("soar_utils::fs::read_file")
        );
    }

    #[test]
    fn a_config_error_keeps_its_own_diagnostic() {
        let err = PackageError::from(soar_config::error::ConfigError::Custom("bad".into()));

        assert_eq!(err.to_string(), "bad");
        assert_eq!(
            err.code().map(|code| code.to_string()).as_deref(),
            Some("soar_config::custom")
        );
    }
}
