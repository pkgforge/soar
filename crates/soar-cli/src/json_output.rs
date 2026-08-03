//! The shapes `--json` reports for the query commands.
//!
//! These are written out rather than derived from the internal models on
//! purpose. Anything a caller can read is a contract soar has to keep, and the
//! models carry fields that exist for the installer's benefit and would be
//! awkward to promise. Adding a field here is safe; a model gaining one is not
//! meant to change the output.

use serde::Serialize;
use soar_core::database::models::{InstalledPackage, Package};
use soar_operations::{InstalledEntry, PackageListEntry, SearchEntry};

/// A package as published by a repository.
#[derive(Serialize)]
pub struct PackageJson {
    pub name: String,
    pub family: Option<String>,
    pub pkg_id: Option<String>,
    pub repo: String,
    pub version: String,
    pub description: String,
    pub pkg_type: Option<String>,
    pub size: Option<u64>,
    pub installed: bool,
    /// Other versions the repository publishes, newest first. Only the newest
    /// is reported above, so these say what is not being shown.
    pub other_versions: Vec<String>,
}

impl PackageJson {
    fn new(package: &Package, installed: bool, other_versions: Vec<String>) -> Self {
        Self {
            name: package.pkg_name.clone(),
            family: package.pkg_family.clone(),
            pkg_id: package.pkg_id.clone(),
            repo: package.repo_name.clone(),
            version: package.version.clone(),
            description: package.description.clone(),
            pkg_type: package.pkg_type.clone(),
            size: package.ghcr_size.or(package.size),
            installed,
            other_versions,
        }
    }
}

impl From<&PackageListEntry> for PackageJson {
    fn from(entry: &PackageListEntry) -> Self {
        Self::new(
            &entry.package,
            entry.installed,
            entry.other_versions.clone(),
        )
    }
}

impl From<&SearchEntry> for PackageJson {
    fn from(entry: &SearchEntry) -> Self {
        Self::new(
            &entry.package,
            entry.installed,
            entry.other_versions.clone(),
        )
    }
}

/// A package installed on this system.
#[derive(Serialize)]
pub struct InstalledJson {
    pub name: String,
    pub family: Option<String>,
    pub pkg_id: Option<String>,
    pub repo: String,
    pub version: String,
    pub pkg_type: Option<String>,
    pub installed_path: String,
    pub installed_date: String,
    /// What the package occupies on disk, which is not the download size.
    pub disk_size: u64,
    pub pinned: bool,
    /// False when the install did not finish, so the package is on disk but
    /// not usable.
    pub healthy: bool,
}

impl From<&InstalledEntry> for InstalledJson {
    fn from(entry: &InstalledEntry) -> Self {
        let package: &InstalledPackage = &entry.package;
        Self {
            name: package.pkg_name.clone(),
            family: package.pkg_family.clone(),
            pkg_id: package.pkg_id.clone(),
            repo: package.repo_name.clone(),
            version: package.version.clone(),
            pkg_type: package.pkg_type.clone(),
            installed_path: package.installed_path.clone(),
            installed_date: package.installed_date.clone(),
            disk_size: entry.disk_size,
            pinned: package.pinned,
            healthy: entry.is_healthy,
        }
    }
}

/// Everything known about one package, as `query` reports it.
#[derive(Serialize)]
pub struct PackageDetailJson {
    pub name: String,
    pub family: Option<String>,
    pub pkg_id: Option<String>,
    pub repo: String,
    pub version: String,
    pub description: String,
    pub pkg_type: Option<String>,
    pub size: Option<u64>,
    /// blake3, which is what a download is verified against.
    pub checksum: Option<String>,
    pub homepages: Vec<String>,
    pub source_urls: Vec<String>,
    pub licenses: Vec<String>,
    pub categories: Vec<String>,
    pub notes: Vec<String>,
    pub download_url: String,
}

impl From<&Package> for PackageDetailJson {
    fn from(package: &Package) -> Self {
        Self {
            name: package.pkg_name.clone(),
            family: package.pkg_family.clone(),
            pkg_id: package.pkg_id.clone(),
            repo: package.repo_name.clone(),
            version: package.version.clone(),
            description: package.description.clone(),
            pkg_type: package.pkg_type.clone(),
            size: package.ghcr_size.or(package.size),
            checksum: package.bsum.clone(),
            homepages: package.homepages.clone().unwrap_or_default(),
            source_urls: package.source_urls.clone().unwrap_or_default(),
            licenses: package.licenses.clone().unwrap_or_default(),
            categories: package.categories.clone().unwrap_or_default(),
            notes: package.notes.clone().unwrap_or_default(),
            download_url: package.download_url.clone(),
        }
    }
}

/// What a command returns, wrapped so fields can be added later without
/// changing the shape a caller already reads.
#[derive(Serialize)]
pub struct Listing<T: Serialize> {
    pub items: Vec<T>,
    pub total: usize,
}

impl<T: Serialize> Listing<T> {
    pub fn new(items: Vec<T>, total: usize) -> Self {
        Self {
            items,
            total,
        }
    }
}

/// Write a result to stdout as a single JSON document.
///
/// Query commands answer once, so unlike the event stream this is one object
/// rather than a line per record.
pub fn emit<T: Serialize>(value: &T) {
    if let Ok(json) = serde_json::to_string(value) {
        println!("{json}");
    }
}
