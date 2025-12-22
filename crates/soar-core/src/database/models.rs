//! Database models for soar-core.

use std::fmt::Display;

use serde::{Deserialize, Serialize};
use soar_db::{models::types::PackageProvide, repository::core::InstalledPackageWithPortable};
use soar_package::PackageExt;

/// Package maintainer information.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Maintainer {
    pub name: String,
    pub contact: String,
}

impl Display for Maintainer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.name, self.contact)
    }
}

/// Remote package metadata from repository.
#[derive(Debug, Clone, Default)]
pub struct Package {
    pub id: u64,
    pub repo_name: String,
    pub disabled: Option<bool>,
    pub disabled_reason: Option<String>,
    pub pkg_id: String,
    pub pkg_name: String,
    pub pkg_family: Option<String>,
    pub pkg_type: Option<String>,
    pub pkg_webpage: Option<String>,
    pub app_id: Option<String>,
    pub description: String,
    pub version: String,
    pub version_upstream: Option<String>,
    pub licenses: Option<Vec<String>>,
    pub download_url: String,
    pub size: Option<u64>,
    pub ghcr_pkg: Option<String>,
    pub ghcr_size: Option<u64>,
    pub ghcr_files: Option<Vec<String>>,
    pub ghcr_blob: Option<String>,
    pub ghcr_url: Option<String>,
    pub bsum: Option<String>,
    pub homepages: Option<Vec<String>>,
    pub notes: Option<Vec<String>>,
    pub source_urls: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub categories: Option<Vec<String>>,
    pub icon: Option<String>,
    pub desktop: Option<String>,
    pub appstream: Option<String>,
    pub build_id: Option<String>,
    pub build_date: Option<String>,
    pub build_action: Option<String>,
    pub build_script: Option<String>,
    pub build_log: Option<String>,
    pub provides: Option<Vec<PackageProvide>>,
    pub snapshots: Option<Vec<String>>,
    pub repology: Option<Vec<String>>,
    pub maintainers: Option<Vec<Maintainer>>,
    pub replaces: Option<Vec<String>>,
    pub soar_syms: bool,
    pub deprecated: bool,
    pub desktop_integration: Option<bool>,
    pub portable: Option<bool>,
    pub recurse_provides: Option<bool>,
}

impl PackageExt for Package {
    fn pkg_name(&self) -> &str {
        &self.pkg_name
    }

    fn pkg_id(&self) -> &str {
        &self.pkg_id
    }

    fn version(&self) -> &str {
        &self.version
    }

    fn repo_name(&self) -> &str {
        &self.repo_name
    }
}

/// Installed package record.
#[derive(Debug, Clone)]
pub struct InstalledPackage {
    pub id: u64,
    pub repo_name: String,
    pub pkg_id: String,
    pub pkg_name: String,
    pub pkg_type: Option<String>,
    pub version: String,
    pub size: u64,
    pub checksum: Option<String>,
    pub installed_path: String,
    pub installed_date: String,
    pub profile: String,
    pub pinned: bool,
    pub is_installed: bool,
    pub with_pkg_id: bool,
    pub detached: bool,
    pub unlinked: bool,
    pub provides: Option<Vec<PackageProvide>>,
    pub portable_path: Option<String>,
    pub portable_home: Option<String>,
    pub portable_config: Option<String>,
    pub portable_share: Option<String>,
    pub portable_cache: Option<String>,
    pub install_patterns: Option<Vec<String>>,
}

impl PackageExt for InstalledPackage {
    fn pkg_name(&self) -> &str {
        &self.pkg_name
    }

    fn pkg_id(&self) -> &str {
        &self.pkg_id
    }

    fn version(&self) -> &str {
        &self.version
    }

    fn repo_name(&self) -> &str {
        &self.repo_name
    }
}

/// Conversion from soar-db InstalledPackageWithPortable to soar-core InstalledPackage.
impl From<InstalledPackageWithPortable> for InstalledPackage {
    fn from(pkg: InstalledPackageWithPortable) -> Self {
        Self {
            id: pkg.id as u64,
            repo_name: pkg.repo_name,
            pkg_id: pkg.pkg_id,
            pkg_name: pkg.pkg_name,
            pkg_type: pkg.pkg_type,
            version: pkg.version,
            size: pkg.size as u64,
            checksum: pkg.checksum,
            installed_path: pkg.installed_path,
            installed_date: pkg.installed_date,
            profile: pkg.profile,
            pinned: pkg.pinned,
            is_installed: pkg.is_installed,
            with_pkg_id: pkg.with_pkg_id,
            detached: pkg.detached,
            unlinked: pkg.unlinked,
            provides: pkg.provides,
            portable_path: pkg.portable_path,
            portable_home: pkg.portable_home,
            portable_config: pkg.portable_config,
            portable_share: pkg.portable_share,
            portable_cache: pkg.portable_cache,
            install_patterns: pkg.install_patterns,
        }
    }
}

/// Conversion from soar-db core Package to soar-core InstalledPackage.
impl From<soar_db::repository::core::InstalledPackage> for InstalledPackage {
    fn from(pkg: soar_db::repository::core::InstalledPackage) -> Self {
        Self {
            id: pkg.id as u64,
            repo_name: pkg.repo_name,
            pkg_id: pkg.pkg_id,
            pkg_name: pkg.pkg_name,
            pkg_type: pkg.pkg_type,
            version: pkg.version,
            size: pkg.size as u64,
            checksum: pkg.checksum,
            installed_path: pkg.installed_path,
            installed_date: pkg.installed_date,
            profile: pkg.profile,
            pinned: pkg.pinned,
            is_installed: pkg.is_installed,
            with_pkg_id: pkg.with_pkg_id,
            detached: pkg.detached,
            unlinked: pkg.unlinked,
            provides: pkg.provides,
            portable_path: None,
            portable_home: None,
            portable_config: None,
            portable_share: None,
            portable_cache: None,
            install_patterns: pkg.install_patterns,
        }
    }
}

/// Conversion from soar-db metadata Package to soar-core Package.
impl From<soar_db::models::metadata::Package> for Package {
    fn from(pkg: soar_db::models::metadata::Package) -> Self {
        Self {
            id: pkg.id as u64,
            repo_name: String::new(), // Set by caller
            disabled: None,
            disabled_reason: None,
            pkg_id: pkg.pkg_id,
            pkg_name: pkg.pkg_name,
            pkg_family: None,
            pkg_type: pkg.pkg_type,
            pkg_webpage: pkg.pkg_webpage,
            app_id: pkg.app_id,
            description: pkg.description.unwrap_or_default(),
            version: pkg.version,
            version_upstream: pkg.version_upstream,
            licenses: pkg.licenses,
            download_url: pkg.download_url,
            size: pkg.size.map(|s| s as u64),
            ghcr_pkg: pkg.ghcr_pkg,
            ghcr_size: pkg.ghcr_size.map(|s| s as u64),
            ghcr_files: None,
            ghcr_blob: pkg.ghcr_blob,
            ghcr_url: pkg.ghcr_url,
            bsum: pkg.bsum,
            homepages: pkg.homepages,
            notes: pkg.notes,
            source_urls: pkg.source_urls,
            tags: pkg.tags,
            categories: pkg.categories,
            icon: pkg.icon,
            desktop: pkg.desktop,
            appstream: pkg.appstream,
            build_id: pkg.build_id,
            build_date: pkg.build_date,
            build_action: pkg.build_action,
            build_script: pkg.build_script,
            build_log: pkg.build_log,
            provides: pkg.provides,
            snapshots: pkg.snapshots,
            repology: None,
            maintainers: None,
            replaces: pkg.replaces,
            soar_syms: pkg.soar_syms,
            deprecated: false,
            desktop_integration: pkg.desktop_integration,
            portable: pkg.portable,
            recurse_provides: pkg.recurse_provides,
        }
    }
}

/// Conversion from soar-db PackageWithRepo to soar-core Package.
impl From<soar_db::models::metadata::PackageWithRepo> for Package {
    fn from(pkg_with_repo: soar_db::models::metadata::PackageWithRepo) -> Self {
        let mut pkg: Package = pkg_with_repo.package.into();
        pkg.repo_name = pkg_with_repo.repo_name;
        pkg
    }
}
