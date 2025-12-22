//! Error types for soar-db.

use miette::Diagnostic;
use thiserror::Error;

/// Database error type for soar-db operations.
#[derive(Error, Diagnostic, Debug)]
pub enum DbError {
    #[error("Database connection failed: {0}")]
    #[diagnostic(
        code(soar_db::connection),
        help("Check if the database file exists and is accessible")
    )]
    ConnectionError(String),

    #[error("Database query failed: {0}")]
    #[diagnostic(
        code(soar_db::query),
        help("Try running 'soar sync' to refresh the database")
    )]
    QueryError(String),

    #[error("Database migration failed: {0}")]
    #[diagnostic(
        code(soar_db::migration),
        help("The database schema may be corrupted. Try removing and re-syncing.")
    )]
    MigrationError(String),

    #[error("Package not found: {0}")]
    #[diagnostic(
        code(soar_db::not_found),
        help("Run 'soar sync' to update package list, or check the package name")
    )]
    NotFound(String),

    #[error("Package already installed: {0}")]
    #[diagnostic(
        code(soar_db::already_installed),
        help("Use 'soar update' to update the package, or 'soar remove' first")
    )]
    AlreadyInstalled(String),

    #[error("Database integrity error: {0}")]
    #[diagnostic(
        code(soar_db::integrity),
        help("The database may be corrupted. Try removing and re-syncing.")
    )]
    IntegrityError(String),

    #[error("IO error: {0}")]
    #[diagnostic(
        code(soar_db::io),
        help("Check file permissions and disk space")
    )]
    IoError(#[from] std::io::Error),
}

impl From<diesel::result::Error> for DbError {
    fn from(err: diesel::result::Error) -> Self {
        match err {
            diesel::result::Error::NotFound => DbError::NotFound("Record not found".to_string()),
            diesel::result::Error::DatabaseError(_, info) => {
                DbError::QueryError(info.message().to_string())
            }
            other => DbError::QueryError(other.to_string()),
        }
    }
}

impl From<diesel::result::ConnectionError> for DbError {
    fn from(err: diesel::result::ConnectionError) -> Self {
        DbError::ConnectionError(err.to_string())
    }
}

/// Result type alias for soar-db operations.
pub type Result<T> = std::result::Result<T, DbError>;
