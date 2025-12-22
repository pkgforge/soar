//! Repository pattern implementations for database operations.
//!
//! This module provides type-safe database operations using the repository pattern.
//! Each repository handles CRUD operations for a specific domain:
//!
//! - [`CoreRepository`] - Installed package operations
//! - [`MetadataRepository`] - Package metadata queries
//! - [`NestRepository`] - Nest configuration management

pub mod core;
pub mod metadata;
pub mod nest;
