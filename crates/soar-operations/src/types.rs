use std::path::PathBuf;

use soar_core::{
    database::models::{InstalledPackage, Package},
    package::install::InstallTarget,
};

// ---- Install ----

/// Options for an install operation.
#[derive(Debug, Default)]
pub struct InstallOptions {
    pub force: bool,
    pub portable: Option<String>,
    pub portable_home: Option<String>,
    pub portable_config: Option<String>,
    pub portable_share: Option<String>,
    pub portable_cache: Option<String>,
    pub binary_only: bool,
    pub no_verify: bool,
    pub name_override: Option<String>,
    pub version_override: Option<String>,
    pub pkg_type_override: Option<String>,
    pub pkg_id_override: Option<String>,
}

/// Result of resolving a single package query.
pub enum ResolveResult {
    /// Resolved to install targets.
    Resolved(Vec<InstallTarget>),
    /// Multiple candidates found; caller must pick one.
    Ambiguous(AmbiguousPackage),
    /// Package not found.
    NotFound(String),
    /// Already installed (and not --force).
    AlreadyInstalled {
        pkg_name: String,
        pkg_id: String,
        repo_name: String,
        version: String,
    },
}

/// Multiple matching packages for a query.
pub struct AmbiguousPackage {
    pub query: String,
    pub candidates: Vec<Package>,
}

/// Report returned after installation completes.
pub struct InstallReport {
    pub installed: Vec<InstalledInfo>,
    pub failed: Vec<FailedInfo>,
    pub warnings: Vec<String>,
}

/// Info about a successfully installed package.
#[derive(Debug)]
pub struct InstalledInfo {
    pub pkg_name: String,
    pub pkg_id: String,
    pub repo_name: String,
    pub version: String,
    pub install_dir: PathBuf,
    pub symlinks: Vec<(PathBuf, PathBuf)>,
    pub notes: Option<Vec<String>>,
}

/// Info about a failed operation.
#[derive(Debug)]
pub struct FailedInfo {
    pub pkg_name: String,
    pub pkg_id: String,
    pub error: String,
}

// ---- Remove ----

/// Result of resolving packages for removal.
pub enum RemoveResolveResult {
    Resolved(Vec<InstalledPackage>),
    Ambiguous {
        query: String,
        candidates: Vec<InstalledPackage>,
    },
    NotInstalled(String),
}

pub struct RemoveReport {
    pub removed: Vec<RemovedInfo>,
    pub failed: Vec<FailedInfo>,
}

pub struct RemovedInfo {
    pub pkg_name: String,
    pub pkg_id: String,
    pub repo_name: String,
    pub version: String,
}

// ---- Update ----

pub struct UpdateInfo {
    pub pkg_name: String,
    pub pkg_id: String,
    pub repo_name: String,
    pub current_version: String,
    pub new_version: String,
    pub target: InstallTarget,
    pub update_toml_url: Option<String>,
}

pub struct UpdateReport {
    pub updated: Vec<InstalledInfo>,
    pub failed: Vec<FailedInfo>,
    pub url_updates: Vec<UrlUpdateInfo>,
}

/// Tracks URL packages that need their packages.toml updated after successful update.
pub struct UrlUpdateInfo {
    pub pkg_name: String,
    pub new_version: String,
    pub new_url: Option<String>,
}

// ---- Search / List ----

pub struct SearchResult {
    pub packages: Vec<SearchEntry>,
    pub total_count: usize,
}

pub struct SearchEntry {
    pub package: Package,
    pub installed: bool,
}

pub struct PackageListResult {
    pub packages: Vec<PackageListEntry>,
    pub total: usize,
}

pub struct PackageListEntry {
    pub package: Package,
    pub installed: bool,
}

pub struct InstalledListResult {
    pub packages: Vec<InstalledEntry>,
    pub total_count: usize,
    pub total_size: u64,
}

pub struct InstalledEntry {
    pub package: InstalledPackage,
    pub disk_size: u64,
    pub is_healthy: bool,
}

// ---- Health ----

pub struct HealthReport {
    pub path_configured: bool,
    pub bin_path: PathBuf,
    pub broken_packages: Vec<BrokenPackage>,
    pub broken_symlinks: Vec<PathBuf>,
}

pub struct BrokenPackage {
    pub pkg_name: String,
    pub pkg_id: String,
    pub installed_path: String,
}

// ---- Apply ----

/// Result of comparing declared packages vs installed packages.
#[derive(Default)]
pub struct ApplyDiff {
    /// Packages to install (declared but not installed).
    pub to_install: Vec<(soar_config::packages::ResolvedPackage, InstallTarget)>,
    /// Packages to update (version mismatch).
    pub to_update: Vec<(soar_config::packages::ResolvedPackage, InstallTarget)>,
    /// Packages to remove (installed but not declared, only with --prune).
    pub to_remove: Vec<InstalledPackage>,
    /// Packages already in sync.
    pub in_sync: Vec<String>,
    /// Packages not found in metadata.
    pub not_found: Vec<String>,
    /// Pending version updates for packages.toml (package_name, version).
    pub pending_version_updates: Vec<(String, String)>,
}

impl ApplyDiff {
    pub fn has_changes(&self) -> bool {
        !self.to_install.is_empty() || !self.to_update.is_empty() || !self.to_remove.is_empty()
    }

    pub fn has_toml_updates(&self) -> bool {
        !self.pending_version_updates.is_empty()
    }
}

pub struct ApplyReport {
    pub installed_count: usize,
    pub updated_count: usize,
    pub removed_count: usize,
    pub failed_count: usize,
}

// ---- Run ----

pub enum PrepareRunResult {
    Ready(PathBuf),
    Ambiguous(AmbiguousPackage),
}

pub struct RunResult {
    pub exit_code: i32,
}

// ---- Switch (use) ----

pub struct VariantInfo {
    pub package: InstalledPackage,
    pub is_active: bool,
}
