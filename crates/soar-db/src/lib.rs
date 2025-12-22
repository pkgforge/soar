//! Database layer for the soar package manager.
//!
//! This crate provides database management for soar, including:
//!
//! - **Connection management**: Separate connections for core, metadata, and nests databases
//! - **Models**: Diesel ORM models for all database tables
//! - **Repositories**: Type-safe CRUD operations using the repository pattern
//! - **Migrations**: Automatic schema migrations using diesel_migrations
//!
//! # Database Architecture
//!
//! Soar uses three types of SQLite databases:
//!
//! - **Core database** (`core.db`): Tracks installed packages
//! - **Metadata databases** (one per repository): Contains package metadata
//! - **Nests database** (`nests.db`): Stores nest configurations
//!
//! # Example
//!
//! ```ignore
//! use soar_db::connection::DatabaseManager;
//! use soar_db::repository::core::CoreRepository;
//! use soar_db::repository::metadata::MetadataRepository;
//!
//! // Create database manager
//! let mut manager = DatabaseManager::new("/path/to/db")?;
//!
//! // Add repository metadata
//! manager.add_metadata_db("pkgforge", "/path/to/pkgforge.db")?;
//!
//! // Query installed packages (DbConnection derefs to SqliteConnection)
//! let installed = CoreRepository::list_all(manager.core())?;
//!
//! // Search for packages
//! if let Some(metadata) = manager.metadata("pkgforge") {
//!     let packages = MetadataRepository::search(metadata, "firefox")?;
//! }
//! ```

pub mod connection;
pub mod error;
pub mod migration;
pub mod models;
pub mod repository;
pub mod schema;

/// Helper macro to convert `Option<serde_json::Value>` to `Option<Vec<T>>`.
#[macro_export]
macro_rules! json_vec {
    ($val:expr) => {
        $val.map(|v| serde_json::from_value(v).unwrap_or_default())
    };
}
