//! Error types for the registry crate.
//!
//! This module defines [`RegistryError`], the error type used throughout
//! the crate, along with helper traits for error context.

use miette::Diagnostic;
use thiserror::Error;

/// Errors that can occur during registry operations.
///
/// This enum covers all error conditions that can arise when fetching,
/// processing, or storing package metadata.
#[derive(Error, Diagnostic, Debug)]
pub enum RegistryError {
    #[error("Error while {action}: {source}")]
    #[diagnostic(code(soar_registry::io))]
    IoError {
        action: String,
        source: std::io::Error,
    },

    #[error(transparent)]
    #[diagnostic(code(soar_registry::system_time))]
    SystemTimeError(#[from] std::time::SystemTimeError),

    #[error(transparent)]
    #[diagnostic(
        code(soar_registry::http),
        help("Check your network connection and the repository URL")
    )]
    UreqError(#[from] ureq::Error),

    #[error(transparent)]
    #[diagnostic(code(soar_registry::download))]
    DownloadError(#[from] soar_dl::error::DownloadError),

    #[error("Failed to fetch from remote source: {0}")]
    #[diagnostic(
        code(soar_registry::fetch_remote),
        help("Verify the repository URL is correct and accessible")
    )]
    FailedToFetchRemote(String),

    #[error(transparent)]
    #[diagnostic(
        code(soar_registry::json),
        help("The metadata file may be corrupted or in an invalid format")
    )]
    JsonError(#[from] serde_json::Error),

    #[error("Invalid URL: {0}")]
    #[diagnostic(
        code(soar_registry::invalid_url),
        help("Ensure the URL is valid and properly formatted")
    )]
    InvalidUrl(String),

    #[error("Metadata content is too short")]
    #[diagnostic(
        code(soar_registry::metadata_too_short),
        help("The metadata file appears to be corrupted or incomplete")
    )]
    MetadataTooShort,

    #[error("ETag not found in metadata response")]
    #[diagnostic(
        code(soar_registry::missing_etag),
        help("The server did not return an ETag header")
    )]
    MissingEtag,

    #[error("{0}")]
    #[diagnostic(code(soar_registry::custom))]
    Custom(String),
}

/// A specialized Result type for registry operations.
pub type Result<T> = std::result::Result<T, RegistryError>;

/// Extension trait for adding context to I/O errors.
///
/// This trait provides a convenient way to convert `std::io::Result` into
/// [`Result`] with descriptive context about what operation failed.
pub trait ErrorContext<T> {
    /// Adds context to an error, describing what action was being performed.
    ///
    /// # Arguments
    ///
    /// * `context` - A closure that returns a description of the failed action
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
            RegistryError::IoError {
                action: context(),
                source: err,
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = RegistryError::MetadataTooShort;
        assert_eq!(err.to_string(), "Metadata content is too short");

        let err = RegistryError::MissingEtag;
        assert_eq!(err.to_string(), "ETag not found in metadata response");

        let err = RegistryError::InvalidUrl("bad-url".to_string());
        assert_eq!(err.to_string(), "Invalid URL: bad-url");
    }
}
