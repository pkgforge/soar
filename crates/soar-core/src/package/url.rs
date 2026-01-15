//! URL package parsing for installing packages from arbitrary URLs.

use std::sync::OnceLock;

use regex::Regex;

use crate::{database::models::Package, error::SoarError, SoarResult};

/// Represents a package parsed from a URL or GHCR reference.
#[derive(Debug, Clone)]
pub struct UrlPackage {
    /// The original URL or GHCR reference
    pub url: String,
    /// Extracted or overridden package name
    pub pkg_name: String,
    /// Generated package ID (lowercase, normalized)
    pub pkg_id: String,
    /// Extracted or overridden version
    pub version: String,
    /// Detected package type from extension (e.g., "appimage")
    pub pkg_type: Option<String>,
    /// Whether this is a GHCR package reference
    pub is_ghcr: bool,
}

impl UrlPackage {
    /// Check if a string is a valid HTTP(S) URL.
    pub fn is_url(input: &str) -> bool {
        let input = input.trim();
        let lower = input.to_lowercase();
        if !lower.starts_with("http://") && !lower.starts_with("https://") {
            return false;
        }
        url::Url::parse(input).is_ok()
    }

    /// Check if a string is a GHCR (GitHub Container Registry) package reference.
    ///
    /// Recognizes formats like:
    /// - `ghcr.io/org/repo:tag`
    /// - `ghcr.io/org/repo@sha256:digest`
    /// - `ghcr.io/org/repo` (implies :latest)
    pub fn is_ghcr(input: &str) -> bool {
        let input = input.trim().to_lowercase();
        input.starts_with("ghcr.io/")
    }

    /// Check if input is either a URL or GHCR reference.
    pub fn is_remote(input: &str) -> bool {
        Self::is_url(input) || Self::is_ghcr(input)
    }

    /// Parse a remote reference (URL or GHCR) and extract package metadata.
    pub fn from_remote(
        input: &str,
        name_override: Option<&str>,
        version_override: Option<&str>,
        pkg_type_override: Option<&str>,
        pkg_id_override: Option<&str>,
    ) -> SoarResult<Self> {
        if Self::is_ghcr(input) {
            Self::from_ghcr(
                input,
                name_override,
                version_override,
                pkg_type_override,
                pkg_id_override,
            )
        } else if Self::is_url(input) {
            Self::from_url(
                input,
                name_override,
                version_override,
                pkg_type_override,
                pkg_id_override,
            )
        } else {
            Err(SoarError::Custom(format!(
                "Invalid remote reference: {}. Expected HTTP(S) URL or ghcr.io/... reference",
                input
            )))
        }
    }

    /// Parse a GHCR reference and extract package metadata.
    pub fn from_ghcr(
        reference: &str,
        name_override: Option<&str>,
        version_override: Option<&str>,
        pkg_type_override: Option<&str>,
        pkg_id_override: Option<&str>,
    ) -> SoarResult<Self> {
        let reference = reference.trim();

        if !Self::is_ghcr(reference) {
            return Err(SoarError::Custom(format!(
                "Invalid GHCR reference: {}",
                reference
            )));
        }

        let path = reference
            .strip_prefix("ghcr.io/")
            .or_else(|| reference.strip_prefix("GHCR.IO/"))
            .unwrap_or(reference);

        let (package, tag) = if let Some((pkg, digest)) = path.split_once('@') {
            (pkg, digest.to_string())
        } else if let Some((pkg, tag)) = path.split_once(':') {
            (pkg, tag.to_string())
        } else {
            (path, "latest".to_string())
        };

        let pkg_name = name_override
            .map(|s| s.to_lowercase())
            .unwrap_or_else(|| package.rsplit('/').next().unwrap_or(package).to_lowercase());

        // Normalize version by stripping "v" prefix for consistency
        let version = version_override
            .map(|v| v.strip_prefix('v').unwrap_or(v).to_string())
            .unwrap_or_else(|| tag.strip_prefix('v').unwrap_or(&tag).to_string());

        let pkg_id = pkg_id_override
            .map(String::from)
            .unwrap_or_else(|| package.replace('/', "."));

        let pkg_type = pkg_type_override.map(|s| s.to_lowercase());

        Ok(Self {
            url: reference.to_string(),
            pkg_name,
            pkg_id,
            version,
            pkg_type,
            is_ghcr: true,
        })
    }

    /// Parse a URL and extract package metadata from filename.
    ///
    /// # Example
    /// ```
    /// use soar_core::package::url::UrlPackage;
    ///
    /// let url = "https://github.com/pkgforge/soar/releases/download/v0.8.1/soar-0.8.1-x86_64-linux";
    /// let pkg = UrlPackage::from_url(url, None, None, None, None).unwrap();
    /// assert_eq!(pkg.pkg_name, "soar");
    /// assert_eq!(pkg.version, "0.8.1");
    /// ```
    pub fn from_url(
        url: &str,
        name_override: Option<&str>,
        version_override: Option<&str>,
        pkg_type_override: Option<&str>,
        pkg_id_override: Option<&str>,
    ) -> SoarResult<Self> {
        let url = url.trim();

        if !Self::is_url(url) {
            return Err(SoarError::Custom(format!("Invalid URL: {}", url)));
        }

        // Extract filename from URL path
        let filename = url
            .rsplit('/')
            .next()
            .and_then(|s| s.split('?').next()) // Remove query params
            .ok_or_else(|| SoarError::Custom("Could not extract filename from URL".into()))?;

        if filename.is_empty() {
            return Err(SoarError::Custom(
                "Could not extract filename from URL".into(),
            ));
        }

        // Detect package type from extension or use override
        let pkg_type = pkg_type_override
            .map(|s| s.to_lowercase())
            .or_else(|| detect_pkg_type(filename));

        // Extract name and version from filename
        let (extracted_name, extracted_version) = parse_filename(filename);

        // Apply overrides or use extracted values
        let pkg_name = name_override
            .map(|s| s.to_lowercase())
            .unwrap_or(extracted_name);

        // Normalize version by stripping "v" prefix for consistency
        let version = version_override
            .map(|v| v.strip_prefix('v').unwrap_or(v).to_string())
            .unwrap_or(extracted_version);

        // Generate pkg_id: use override, or extract from URL, or generate from name and type
        let pkg_id = pkg_id_override
            .map(String::from)
            .or_else(|| extract_pkg_id_from_url(url))
            .unwrap_or_else(|| {
                if let Some(ref ptype) = pkg_type {
                    format!("{}-{}", pkg_name, ptype)
                } else {
                    pkg_name.clone()
                }
            });

        Ok(Self {
            url: url.to_string(),
            pkg_name,
            pkg_id,
            version,
            pkg_type,
            is_ghcr: false,
        })
    }

    /// Convert to a Package struct for installation.
    pub fn to_package(&self) -> Package {
        if self.is_ghcr {
            Package {
                id: 0,
                repo_name: "local".to_string(),
                pkg_id: self.pkg_id.clone(),
                pkg_name: self.pkg_name.clone(),
                pkg_type: self.pkg_type.clone(),
                version: self.version.clone(),
                download_url: String::new(),
                ghcr_pkg: Some(self.url.clone()),
                description: format!("Installed from {}", self.url),
                ..Default::default()
            }
        } else {
            Package {
                id: 0,
                repo_name: "local".to_string(),
                pkg_id: self.pkg_id.clone(),
                pkg_name: self.pkg_name.clone(),
                pkg_type: self.pkg_type.clone(),
                version: self.version.clone(),
                download_url: self.url.clone(),
                description: format!("Installed from {}", self.url),
                ..Default::default()
            }
        }
    }
}

/// Extract pkg_id from URL based on host and first two path segments.
///
/// Examples:
/// - `https://github.com/user/repo/...` → `github.com.user.repo`
/// - `https://example.com/foo/bar/...` → `example.com.foo.bar`
fn extract_pkg_id_from_url(url: &str) -> Option<String> {
    let url = url.trim().to_lowercase();

    // Remove protocol
    let without_protocol = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;

    // Split into host and path
    let (host, path) = without_protocol.split_once('/')?;

    // Take first two path segments
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).take(2).collect();

    if parts.len() >= 2 {
        Some(format!("{}.{}.{}", host, parts[0], parts[1]))
    } else if parts.len() == 1 {
        Some(format!("{}.{}", host, parts[0]))
    } else {
        None
    }
}

/// Detect package type from filename extension.
fn detect_pkg_type(filename: &str) -> Option<String> {
    let lower = filename.to_lowercase();

    if lower.ends_with(".appimage") {
        Some("appimage".to_string())
    } else if lower.ends_with(".flatimage") {
        Some("flatimage".to_string())
    } else if lower.ends_with(".runimage") {
        Some("runimage".to_string())
    } else if lower.ends_with(".nixappimage") {
        Some("nixappimage".to_string())
    } else if lower.ends_with(".tar.gz")
        || lower.ends_with(".tgz")
        || lower.ends_with(".tar.xz")
        || lower.ends_with(".tar.bz2")
        || lower.ends_with(".zip")
    {
        Some("archive".to_string())
    } else {
        // Assume binary if no recognized extension
        None
    }
}

/// Parse filename to extract name and version.
///
/// Handles common patterns like:
/// - `Name-Version-platform.ext`
/// - `name_version.ext`
/// - `name-version.ext`
fn parse_filename(filename: &str) -> (String, String) {
    static VERSION_RE: OnceLock<Regex> = OnceLock::new();
    let re = VERSION_RE.get_or_init(|| {
        // Match: Name[-_.]v?Version (where version is purely numeric like 1.2.3)
        // The name can contain letters, digits, underscores but must not end with underscore before version
        Regex::new(
            r"(?ix)
            ^
            (?P<name>[a-zA-Z][a-zA-Z0-9]*(?:_[a-zA-Z][a-zA-Z0-9]*)*)  # Name (letters/digits, underscores between words)
            [-_.]                              # Separator before version
            (?:v)?                             # Optional 'v' prefix
            (?P<version>\d+(?:\.\d+)*)         # Version: only digits and dots (1.2.3)
            (?:[-_.].*)?                       # Rest of filename (platform, arch, etc)
            $
            ",
        )
        .unwrap()
    });

    // Remove extension(s) for parsing
    let base = remove_extensions(filename);

    if let Some(caps) = re.captures(&base) {
        let name = caps
            .name("name")
            .map(|m| m.as_str().to_lowercase())
            .unwrap_or_else(|| base.to_lowercase());

        let version = caps
            .name("version")
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        (name, version)
    } else {
        // Fallback: use entire base as name, unknown version
        (base.to_lowercase(), "unknown".to_string())
    }
}

/// Remove known extensions from filename.
fn remove_extensions(filename: &str) -> String {
    let lower = filename.to_lowercase();

    let extensions = [
        ".tar.gz",
        ".tar.xz",
        ".tar.bz2",
        ".appimage",
        ".flatimage",
        ".runimage",
        ".nixappimage",
        ".tgz",
        ".zip",
        ".exe",
        ".bin",
    ];

    for ext in extensions {
        if lower.ends_with(ext) {
            return filename[..filename.len() - ext.len()].to_string();
        }
    }

    filename.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_url() {
        assert!(UrlPackage::is_url("https://example.com/file.AppImage"));
        assert!(UrlPackage::is_url("http://example.com/file.AppImage"));
        assert!(UrlPackage::is_url("  HTTPS://example.com/file  "));
        assert!(!UrlPackage::is_url("example.com/file.AppImage"));
        assert!(!UrlPackage::is_url("curl"));
        assert!(!UrlPackage::is_url("jq#all"));
    }

    #[test]
    fn test_parse_with_overrides() {
        let url = "https://github.com/user/repo/releases/app.AppImage";
        let pkg = UrlPackage::from_url(url, Some("myapp"), Some("2.0.0"), None, None).unwrap();

        assert_eq!(pkg.pkg_name, "myapp");
        assert_eq!(pkg.version, "2.0.0");
        assert_eq!(pkg.pkg_id, "github.com.user.repo");
    }

    #[test]
    fn test_parse_with_pkg_type_override() {
        let url = "https://example.com/downloads/app";
        let pkg = UrlPackage::from_url(url, Some("myapp"), Some("1.0.0"), Some("appimage"), None)
            .unwrap();

        assert_eq!(pkg.pkg_name, "myapp");
        assert_eq!(pkg.version, "1.0.0");
        assert_eq!(pkg.pkg_type, Some("appimage".to_string()));
        assert_eq!(pkg.pkg_id, "example.com.downloads.app");
    }

    #[test]
    fn test_extract_pkg_id_from_url() {
        assert_eq!(
            extract_pkg_id_from_url("https://github.com/pkgforge/soar/releases/file"),
            Some("github.com.pkgforge.soar".to_string())
        );
        assert_eq!(
            extract_pkg_id_from_url("https://gitlab.com/user/project/-/releases"),
            Some("gitlab.com.user.project".to_string())
        );
        assert_eq!(
            extract_pkg_id_from_url("https://example.com/foo/bar/baz"),
            Some("example.com.foo.bar".to_string())
        );
        assert_eq!(
            extract_pkg_id_from_url("https://example.com/app"),
            Some("example.com.app".to_string())
        );
    }

    #[test]
    fn test_to_package() {
        let url = "https://github.com/user/testrepo/releases/Test-1.0.AppImage";
        let url_pkg = UrlPackage::from_url(url, None, None, None, None).unwrap();
        let pkg = url_pkg.to_package();

        assert_eq!(pkg.repo_name, "local");
        assert_eq!(pkg.pkg_name, "test");
        assert_eq!(pkg.version, "1.0");
        assert_eq!(pkg.pkg_id, "github.com.user.testrepo");
        assert_eq!(pkg.download_url, url);
    }

    #[test]
    fn test_detect_pkg_type() {
        assert_eq!(
            detect_pkg_type("app.AppImage"),
            Some("appimage".to_string())
        );
        assert_eq!(
            detect_pkg_type("app.FlatImage"),
            Some("flatimage".to_string())
        );
        assert_eq!(detect_pkg_type("app.tar.gz"), Some("archive".to_string()));
        assert_eq!(detect_pkg_type("app"), None);
    }

    #[test]
    fn test_parse_various_filenames() {
        // Standard pattern
        let (name, ver) = parse_filename("myapp-2.0.1-linux-arm64.AppImage");
        assert_eq!(name, "myapp");
        assert_eq!(ver, "2.0.1");

        // With v prefix
        let (name, ver) = parse_filename("app-v2.0.0.AppImage");
        assert_eq!(name, "app");
        assert_eq!(ver, "2.0.0");

        // Underscore separator
        let (name, ver) = parse_filename("myapp_1.2.3.AppImage");
        assert_eq!(name, "myapp");
        assert_eq!(ver, "1.2.3");

        // No version
        let (name, ver) = parse_filename("simple.AppImage");
        assert_eq!(name, "simple");
        assert_eq!(ver, "unknown");
    }

    #[test]
    fn test_is_ghcr() {
        assert!(UrlPackage::is_ghcr("ghcr.io/org/repo:tag"));
        assert!(UrlPackage::is_ghcr("ghcr.io/org/repo@sha256:abc123"));
        assert!(UrlPackage::is_ghcr("ghcr.io/org/repo"));
        assert!(UrlPackage::is_ghcr("  GHCR.IO/org/repo:tag  "));
        assert!(!UrlPackage::is_ghcr("docker.io/org/repo:tag"));
        assert!(!UrlPackage::is_ghcr("https://ghcr.io/org/repo"));
        assert!(!UrlPackage::is_ghcr("org/repo:tag"));
    }

    #[test]
    fn test_is_remote() {
        // HTTP URLs
        assert!(UrlPackage::is_remote("https://example.com/file.AppImage"));
        assert!(UrlPackage::is_remote("http://example.com/file.AppImage"));
        // GHCR references
        assert!(UrlPackage::is_remote("ghcr.io/org/repo:tag"));
        assert!(UrlPackage::is_remote("ghcr.io/org/repo"));
        // Not remote
        assert!(!UrlPackage::is_remote("org/repo:tag"));
        assert!(!UrlPackage::is_remote("curl"));
    }

    #[test]
    fn test_ghcr_with_tag() {
        let ghcr = "ghcr.io/pkgforge/soar:v0.8.1";
        let pkg = UrlPackage::from_ghcr(ghcr, None, None, None, None).unwrap();

        assert_eq!(pkg.pkg_name, "soar");
        assert_eq!(pkg.version, "0.8.1"); // 'v' prefix stripped
        assert_eq!(pkg.pkg_id, "pkgforge.soar");
        assert!(pkg.is_ghcr);
    }

    #[test]
    fn test_ghcr_with_digest() {
        let ghcr = "ghcr.io/org/repo@sha256:deadbeef1234567890";
        let pkg = UrlPackage::from_ghcr(ghcr, None, None, None, None).unwrap();

        assert_eq!(pkg.pkg_name, "repo");
        assert_eq!(pkg.version, "sha256:deadbeef1234567890");
        assert_eq!(pkg.pkg_id, "org.repo");
        assert!(pkg.is_ghcr);
    }

    #[test]
    fn test_ghcr_without_tag() {
        let ghcr = "ghcr.io/org/package";
        let pkg = UrlPackage::from_ghcr(ghcr, None, None, None, None).unwrap();

        assert_eq!(pkg.pkg_name, "package");
        assert_eq!(pkg.version, "latest");
        assert_eq!(pkg.pkg_id, "org.package");
        assert!(pkg.is_ghcr);
    }

    #[test]
    fn test_ghcr_nested_package() {
        let ghcr = "ghcr.io/org/team/repo:1.0";
        let pkg = UrlPackage::from_ghcr(ghcr, None, None, None, None).unwrap();

        assert_eq!(pkg.pkg_name, "repo");
        assert_eq!(pkg.version, "1.0");
        assert_eq!(pkg.pkg_id, "org.team.repo");
        assert!(pkg.is_ghcr);
    }

    #[test]
    fn test_ghcr_with_overrides() {
        let ghcr = "ghcr.io/org/repo:v1.0";
        let pkg =
            UrlPackage::from_ghcr(ghcr, Some("myapp"), Some("2.0.0"), None, Some("custom-id"))
                .unwrap();

        assert_eq!(pkg.pkg_name, "myapp");
        assert_eq!(pkg.version, "2.0.0");
        assert_eq!(pkg.pkg_id, "custom-id");
        assert!(pkg.is_ghcr);
    }

    #[test]
    fn test_ghcr_to_package() {
        let ghcr = "ghcr.io/pkgforge/soar:v0.8.1";
        let url_pkg = UrlPackage::from_ghcr(ghcr, None, None, None, None).unwrap();
        let pkg = url_pkg.to_package();

        assert_eq!(pkg.repo_name, "local");
        assert_eq!(pkg.pkg_name, "soar");
        assert_eq!(pkg.version, "0.8.1"); // 'v' prefix stripped
        assert_eq!(pkg.pkg_id, "pkgforge.soar");
        assert_eq!(pkg.download_url, "");
        assert_eq!(
            pkg.ghcr_pkg,
            Some("ghcr.io/pkgforge/soar:v0.8.1".to_string())
        );
    }

    #[test]
    fn test_url_to_package_not_ghcr() {
        let url = "https://github.com/user/repo/releases/app-1.0.AppImage";
        let url_pkg = UrlPackage::from_url(url, None, None, None, None).unwrap();
        let pkg = url_pkg.to_package();

        assert!(!url_pkg.is_ghcr);
        assert_eq!(pkg.download_url, url);
        assert_eq!(pkg.ghcr_pkg, None);
    }
}
