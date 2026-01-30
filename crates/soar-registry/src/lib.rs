//! Registry management for the soar package manager.
//!
//! This crate provides functionality for fetching, processing, and managing
//! package metadata from remote repositories.
//!
//! # Overview
//!
//! The crate handles metadata sources:
//! - **Repositories**: Standard package repositories containing package metadata
//!
//! Metadata can be provided in two formats:
//! - SQLite databases (`.sdb` files, optionally zstd-compressed)
//! - JSON files containing package arrays
//!
//! # Example
//!
//! ```no_run
//! use soar_registry::{fetch_metadata, MetadataContent};
//! use soar_config::repository::Repository;
//!
//! async fn sync_repo(repo: &Repository, existing_etag: Option<String>) -> soar_registry::Result<()> {
//!     if let Some((etag, content)) = fetch_metadata(repo, false, existing_etag).await? {
//!         match content {
//!             MetadataContent::SqliteDb(bytes) => {
//!                 // Write SQLite database to disk
//!             }
//!             MetadataContent::Json(packages) => {
//!                 // Process JSON packages into a database
//!             }
//!         }
//!     }
//!     Ok(())
//! }
//! ```

pub mod error;
pub mod metadata;
pub mod package;

pub use error::{ErrorContext, RegistryError, Result};
pub use metadata::{
    fetch_metadata, fetch_public_key, process_metadata_content, write_metadata_db, MetadataContent,
    SQLITE_MAGIC_BYTES, ZST_MAGIC_BYTES,
};
pub use package::RemotePackage;
