//! The shapes `--json` reports for the query commands.
//!
//! Written out rather than derived from the internal models: what a caller can
//! read is a contract, and the models carry fields meant for the installer.
//! Adding a field here is safe; a model gaining one should not change output.

use serde::Serialize;
use soar_config::repository::Repository;
use soar_core::{
    database::models::{InstalledPackage, Package},
    package::install::InstallTarget,
};
use soar_operations::{ApplyDiff, InstalledEntry, PackageListEntry, SearchEntry, UpdateInfo};

/// A package as published by a repository.
#[derive(Serialize)]
pub struct PackageJson {
    pub name: String,
    pub family: Option<String>,
    pub repo: String,
    pub version: String,
    pub description: String,
    pub pkg_type: Option<String>,
    pub size: Option<u64>,
    pub installed: bool,
    /// Other versions the repository publishes, newest first.
    pub other_versions: Vec<String>,
}

impl PackageJson {
    fn new(package: &Package, installed: bool, other_versions: Vec<String>) -> Self {
        Self {
            name: package.pkg_name.clone(),
            family: package.pkg_family.clone(),
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
    pub repo: String,
    pub version: String,
    pub pkg_type: Option<String>,
    pub installed_path: String,
    pub installed_date: String,
    /// Size on disk, which is not the download size.
    pub disk_size: u64,
    pub pinned: bool,
    /// False when the install did not finish.
    pub healthy: bool,
}

impl From<&InstalledEntry> for InstalledJson {
    fn from(entry: &InstalledEntry) -> Self {
        let package: &InstalledPackage = &entry.package;
        Self {
            name: package.pkg_name.clone(),
            family: package.pkg_family.clone(),
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
    pub repo: String,
    pub version: String,
    pub description: String,
    pub pkg_type: Option<String>,
    pub size: Option<u64>,
    /// blake3, as a download is verified against.
    pub checksum: Option<String>,
    pub homepages: Vec<String>,
    pub source_urls: Vec<String>,
    pub licenses: Vec<String>,
    pub categories: Vec<String>,
    pub notes: Vec<String>,
    pub download_url: String,
    pub build_date: Option<String>,
    /// Formatted for display.
    pub maintainers: Vec<String>,
}

impl From<&Package> for PackageDetailJson {
    fn from(package: &Package) -> Self {
        Self {
            name: package.pkg_name.clone(),
            family: package.pkg_family.clone(),
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
            build_date: package.build_date.clone(),
            maintainers: package
                .maintainers
                .as_ref()
                .map(|all| all.iter().map(ToString::to_string).collect())
                .unwrap_or_default(),
        }
    }
}

/// A package with a newer version waiting for it.
#[derive(Serialize)]
pub struct UpdateJson {
    pub name: String,
    pub family: Option<String>,
    pub repo: String,
    pub current_version: String,
    pub new_version: String,
    pub size: Option<u64>,
}

impl From<&UpdateInfo> for UpdateJson {
    fn from(update: &UpdateInfo) -> Self {
        let package = &update.target.package;
        Self {
            name: update.pkg_name.clone(),
            family: package.pkg_family.clone(),
            repo: update.repo_name.clone(),
            current_version: update.current_version.clone(),
            new_version: update.new_version.clone(),
            size: package.ghcr_size.or(package.size),
        }
    }
}

/// A repository soar is configured to read.
#[derive(Serialize)]
pub struct RepositoryJson {
    pub name: String,
    pub url: String,
    pub enabled: bool,
    pub signature_verification: bool,
    pub desktop_integration: bool,
}

impl From<&Repository> for RepositoryJson {
    fn from(repo: &Repository) -> Self {
        Self {
            name: repo.name.clone(),
            url: repo.url.clone(),
            enabled: repo.is_enabled(),
            signature_verification: repo.signature_verification.unwrap_or(false),
            desktop_integration: repo.desktop_integration.unwrap_or(false),
        }
    }
}

/// Where soar keeps its files, so a frontend can read and write the same ones.
#[derive(Serialize)]
pub struct EnvJson {
    pub config: String,
    pub packages_config: String,
    pub bin: String,
    pub db: String,
    pub cache: String,
    pub packages: String,
    pub repositories: String,
}

/// One package the declarative configuration would change.
#[derive(Serialize)]
pub struct ApplyChangeJson {
    pub name: String,
    pub family: Option<String>,
    pub repo: String,
    pub version: String,
    /// The version on disk now, for a package being replaced.
    pub current_version: Option<String>,
}

/// What applying the declarative configuration would do.
#[derive(Serialize)]
pub struct ApplyDiffJson {
    pub to_install: Vec<ApplyChangeJson>,
    pub to_update: Vec<ApplyChangeJson>,
    pub to_remove: Vec<ApplyChangeJson>,
    pub in_sync: Vec<String>,
    pub not_found: Vec<String>,
}

impl ApplyDiffJson {
    pub fn new(diff: &ApplyDiff) -> Self {
        let change = |target: &InstallTarget| {
            let package = &target.package;
            ApplyChangeJson {
                name: package.pkg_name.clone(),
                family: package.pkg_family.clone(),
                repo: package.repo_name.clone(),
                version: package.version.clone(),
                current_version: target.existing_install.as_ref().map(|e| e.version.clone()),
            }
        };

        Self {
            to_install: diff.to_install.iter().map(|(_, t)| change(t)).collect(),
            to_update: diff.to_update.iter().map(|(_, t)| change(t)).collect(),
            to_remove: diff
                .to_remove
                .iter()
                .map(|package| {
                    ApplyChangeJson {
                        name: package.pkg_name.clone(),
                        family: package.pkg_family.clone(),
                        repo: package.repo_name.clone(),
                        version: package.version.clone(),
                        current_version: Some(package.version.clone()),
                    }
                })
                .collect(),
            in_sync: diff.in_sync.clone(),
            not_found: diff.not_found.clone(),
        }
    }
}

/// Wraps a listing so fields can be added without changing the shape.
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
pub fn emit<T: Serialize>(value: &T) {
    if let Ok(json) = serde_json::to_string(value) {
        println!("{json}");
    }
}
