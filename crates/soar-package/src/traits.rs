//! Traits for package operations.

/// Trait for types that represent package metadata.
///
/// This trait provides access to basic package information needed for
/// integration operations like desktop file creation and symlink management.
pub trait PackageExt {
    /// Returns the package name (human-readable name).
    fn pkg_name(&self) -> &str;

    /// Returns the package identifier, when the repository publishes one.
    fn pkg_id(&self) -> Option<&str>;

    /// Returns the package version string.
    fn version(&self) -> &str;

    /// Returns the repository name this package belongs to.
    fn repo_name(&self) -> &str;
}
