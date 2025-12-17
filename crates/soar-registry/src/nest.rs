//! Nest data structures.
//!
//! Nests are user-defined package collections that can be added to soar,
//! similar to PPAs in apt or custom repositories.

use serde::{Deserialize, Serialize};

/// Represents a user-defined package collection (nest).
///
/// Nests allow users to add custom package sources beyond the default
/// repositories. They can be hosted on GitHub or any HTTP server that
/// serves JSON metadata.
///
/// # URL Formats
///
/// Nests support two URL formats:
/// - `github:owner/repo` - Fetches from GitHub releases
/// - Direct HTTP/HTTPS URLs - Fetches from the specified URL
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Nest {
    /// Unique identifier for the nest in the local database.
    pub id: i64,
    /// Human-readable name for the nest.
    pub name: String,
    /// URL or GitHub shorthand for fetching nest metadata.
    pub url: String,
}

impl Nest {
    /// Creates a new nest with the specified parameters.
    ///
    /// # Arguments
    ///
    /// * `id` - Database identifier
    /// * `name` - Display name for the nest
    /// * `url` - Metadata URL (can be `github:owner/repo` or a direct URL)
    pub fn new(id: i64, name: String, url: String) -> Self {
        Self {
            id,
            name,
            url,
        }
    }
}
