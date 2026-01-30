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

    #[error(transparent)]
    #[diagnostic(code(soar::system), help("Check system permissions and resources"))]
    Errno(#[from] nix::errno::Errno),

    #[error(transparent)]
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

    #[error("Error while {action}")]
    #[diagnostic(code(soar::io), help("Check file permissions and disk space"))]
    IoError {
        action: String,
        #[source]
        source: std::io::Error,
    },

    #[error(transparent)]
    #[diagnostic(code(soar::time))]
    SystemTimeError(#[from] std::time::SystemTimeError),

    #[error(transparent)]
    #[diagnostic(code(soar::toml), help("Check your configuration syntax"))]
    TomlError(#[from] toml::ser::Error),

    #[error("Database operation failed: {0}")]
    #[diagnostic(
        code(soar::database),
        help("Try running 'soar sync' to refresh the database")
    )]
    DatabaseError(String),

    #[error(transparent)]
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

    #[error(transparent)]
    #[diagnostic(code(soar::regex), help("Check your regex pattern syntax"))]
    RegexError(#[from] regex::Error),

    #[error("Landlock is not supported on this system")]
    #[diagnostic(
        code(soar::sandbox::not_supported),
        help("Landlock requires Linux kernel 5.13+. Hooks will run without sandboxing.")
    )]
    SandboxNotSupported,

    #[error("Failed to create Landlock ruleset: {0}")]
    #[diagnostic(
        code(soar::sandbox::ruleset),
        help("This may indicate a kernel or permission issue")
    )]
    SandboxRulesetCreation(String),

    #[error("Failed to add sandbox rule for path '{path}': {reason}")]
    #[diagnostic(
        code(soar::sandbox::path_rule),
        help("Check if the path exists and is accessible")
    )]
    SandboxPathRule { path: String, reason: String },

    #[error("Failed to add sandbox network rule for port {port}: {reason}")]
    #[diagnostic(
        code(soar::sandbox::network_rule),
        help("Network restrictions require Landlock V4+ (kernel 6.7+)")
    )]
    SandboxNetworkRule { port: u16, reason: String },

    #[error("Failed to enforce Landlock sandbox: {0}")]
    #[diagnostic(
        code(soar::sandbox::enforcement),
        help("This may indicate a kernel or permission issue")
    )]
    SandboxEnforcement(String),

    #[error("Sandboxed command execution failed: {0}")]
    #[diagnostic(
        code(soar::sandbox::execution),
        help("Check the command and sandbox configuration")
    )]
    SandboxExecution(String),
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
        self.map_err(|err| {
            SoarError::IoError {
                action: context(),
                source: err,
            }
        })
    }
}
