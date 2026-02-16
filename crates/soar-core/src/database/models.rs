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

/// Replace `{{version}}` placeholder in a string with the actual version.
fn resolve_version_placeholder(s: &str, version: &str) -> String {
    s.replace("{{version}}", version)
}

/// Replace `{{version}}` placeholder in an optional string.
fn resolve_version_placeholder_opt(s: Option<&str>, version: &str) -> Option<String> {
    s.map(|s| resolve_version_placeholder(s, version))
}

impl Package {
    /// Check if a version is available for this package.
    ///
    /// Returns true if the version matches the package's current version
    /// or is present in the snapshots array.
    pub fn has_version(&self, version: &str) -> bool {
        if self.version == version {
            return true;
        }
        self.snapshots
            .as_ref()
            .is_some_and(|s| s.iter().any(|v| v == version))
    }

    /// Create a copy of this package with all `{{version}}` placeholders resolved.
    ///
    /// If `version` is provided, uses that version; otherwise uses the package's version.
    /// This is useful when installing a specific snapshot version.
    pub fn resolve(&self, version: Option<&str>) -> Self {
        let ver = version.unwrap_or(&self.version);
        let mut pkg = self.clone();
        pkg.download_url = resolve_version_placeholder(&self.download_url, ver);
        pkg.ghcr_pkg = resolve_version_placeholder_opt(self.ghcr_pkg.as_deref(), ver);
        pkg.ghcr_blob = resolve_version_placeholder_opt(self.ghcr_blob.as_deref(), ver);
        pkg.ghcr_url = resolve_version_placeholder_opt(self.ghcr_url.as_deref(), ver);
        if version.is_some() {
            pkg.version = ver.to_string();
        }
        pkg
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
            pkg_family: pkg.pkg_family,
            pkg_type: pkg.pkg_type,
            pkg_webpage: pkg.pkg_webpage,
            app_id: pkg.app_id,
            description: pkg.description.unwrap_or_default(),
            version: pkg.version,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_version_placeholder() {
        assert_eq!(
            resolve_version_placeholder("https://example.com/pkg?tag={{version}}-x86_64", "v1.0.0"),
            "https://example.com/pkg?tag=v1.0.0-x86_64"
        );
    }

    #[test]
    fn test_resolve_version_placeholder_multiple() {
        assert_eq!(
            resolve_version_placeholder("ghcr.io/user/pkg:{{version}}-{{version}}", "v2.0.0"),
            "ghcr.io/user/pkg:v2.0.0-v2.0.0"
        );
    }

    #[test]
    fn test_resolve_version_placeholder_none() {
        assert_eq!(
            resolve_version_placeholder("https://example.com/static-url", "v1.0.0"),
            "https://example.com/static-url"
        );
    }

    #[test]
    fn test_resolve_version_placeholder_opt() {
        assert_eq!(
            resolve_version_placeholder_opt(Some("ghcr.io/pkg:{{version}}"), "v1.0.0"),
            Some("ghcr.io/pkg:v1.0.0".to_string())
        );
        assert_eq!(resolve_version_placeholder_opt(None, "v1.0.0"), None);
    }

    #[test]
    fn test_package_resolve() {
        let pkg = Package {
            version: "v1.0.0".to_string(),
            download_url: "https://example.com/pkg?tag={{version}}".to_string(),
            ghcr_pkg: Some("ghcr.io/pkg:{{version}}".to_string()),
            ..Default::default()
        };

        // Resolve with default version
        let resolved = pkg.resolve(None);
        assert_eq!(resolved.download_url, "https://example.com/pkg?tag=v1.0.0");
        assert_eq!(resolved.ghcr_pkg, Some("ghcr.io/pkg:v1.0.0".to_string()));
        assert_eq!(resolved.version, "v1.0.0");

        // Resolve with specific version (snapshot)
        let resolved = pkg.resolve(Some("v0.5.0"));
        assert_eq!(resolved.download_url, "https://example.com/pkg?tag=v0.5.0");
        assert_eq!(resolved.ghcr_pkg, Some("ghcr.io/pkg:v0.5.0".to_string()));
        assert_eq!(resolved.version, "v0.5.0");
    }

    #[test]
    fn test_package_resolve_no_placeholder() {
        let pkg = Package {
            version: "v2.0.0".to_string(),
            download_url: "https://api.example.com/pkg/static-url".to_string(),
            ..Default::default()
        };

        // No placeholder - URL unchanged
        let resolved = pkg.resolve(None);
        assert_eq!(
            resolved.download_url,
            "https://api.example.com/pkg/static-url"
        );
    }

    #[test]
    fn test_has_version_current() {
        let pkg = Package {
            version: "v1.0.0".to_string(),
            ..Default::default()
        };

        assert!(pkg.has_version("v1.0.0"));
        assert!(!pkg.has_version("v0.9.0"));
    }

    #[test]
    fn test_has_version_snapshot() {
        let pkg = Package {
            version: "v1.0.0".to_string(),
            snapshots: Some(vec![
                "v0.9.0".to_string(),
                "v0.8.0".to_string(),
                "v0.7.0".to_string(),
            ]),
            ..Default::default()
        };

        assert!(pkg.has_version("v1.0.0")); // current version
        assert!(pkg.has_version("v0.9.0")); // in snapshots
        assert!(pkg.has_version("v0.8.0")); // in snapshots
        assert!(!pkg.has_version("v0.6.0")); // not available
    }

    #[test]
    fn test_has_version_no_snapshots() {
        let pkg = Package {
            version: "v1.0.0".to_string(),
            snapshots: None,
            ..Default::default()
        };

        assert!(pkg.has_version("v1.0.0"));
        assert!(!pkg.has_version("v0.9.0"));
    }
}
