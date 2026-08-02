//! URL package parsing for installing packages from arbitrary URLs.

use std::sync::OnceLock;

use regex::Regex;
use soar_utils::path::is_safe_component;

use crate::{database::models::Package, error::SoarError, SoarResult};

/// Represents a package parsed from a URL or GHCR reference.
#[derive(Debug, Clone)]
pub struct UrlPackage {
    /// The original URL or GHCR reference
    pub url: String,
    /// Extracted or overridden package name
    pub pkg_name: String,
    /// Package id, only when the caller supplied one. Nothing derives an id:
    /// repositories on the declarative format publish none, so inventing one
    /// here would be the only place they came from.
    pub pkg_id: Option<String>,
    /// Where the package came from, used to tell apart two sources that
    /// produce the same name. This is what an id used to stand in for.
    pub pkg_family: Option<String>,
    /// Extracted or overridden version
    pub version: String,
    /// Detected package type from extension (e.g., "appimage")
    pub pkg_type: Option<String>,
    /// Whether this is a GHCR package reference
    pub is_ghcr: bool,
    /// Optional size in bytes
    pub size: Option<u64>,
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

    /// Rejects a name or id that is not usable as a single path component.
    ///
    /// Both are joined into the install dir and interpolated into resource
    /// paths, so a caller-supplied override containing `/` or `..` would escape
    /// it. The derived defaults are always dot-separated and unaffected.
    fn validate_names(
        pkg_name: &str,
        pkg_id: Option<&str>,
        pkg_family: Option<&str>,
    ) -> SoarResult<()> {
        if !is_safe_component(pkg_name) {
            return Err(SoarError::Custom(format!(
                "Invalid package name '{}': must be a single path component",
                pkg_name
            )));
        }
        for (label, value) in [("id", pkg_id), ("family", pkg_family)] {
            if let Some(value) = value {
                if !is_safe_component(value) {
                    return Err(SoarError::Custom(format!(
                        "Invalid package {label} '{value}': must be a single path component"
                    )));
                }
            }
        }
        Ok(())
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

        let pkg_id = pkg_id_override.map(String::from);
        let pkg_family = Some(package.replace('/', "."));

        let pkg_type = pkg_type_override.map(|s| s.to_lowercase());

        Self::validate_names(&pkg_name, pkg_id.as_deref(), pkg_family.as_deref())?;

        Ok(Self {
            url: reference.to_string(),
            pkg_id,
            pkg_family,
            pkg_name,
            version,
            pkg_type,
            is_ghcr: true,
            size: None,
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

        // A release is tagged with its version, while the asset in it is named
        // however the project chose, so the tag is preferred over the filename
        // where the URL has one. An explicit override still wins over both.
        let version = version_override
            .map(|v| v.strip_prefix('v').unwrap_or(v).to_string())
            .or_else(|| crate::package::release_source::version_from_release_url(url))
            .unwrap_or(extracted_version);

        let pkg_id = pkg_id_override.map(String::from);
        // The host and project the URL points at, falling back to the name so
        // two unrelated downloads sharing a filename stay distinct.
        let pkg_family = extract_family_from_url(url).or_else(|| {
            if let Some(ref ptype) = pkg_type {
                Some(format!("{}-{}", pkg_name, ptype))
            } else {
                Some(pkg_name.clone())
            }
        });

        Self::validate_names(&pkg_name, pkg_id.as_deref(), pkg_family.as_deref())?;

        Ok(Self {
            url: url.to_string(),
            pkg_id,
            pkg_family,
            pkg_name,
            version,
            pkg_type,
            is_ghcr: false,
            size: None,
        })
    }

    /// Convert to a Package struct for installation.
    pub fn to_package(&self) -> Package {
        if self.is_ghcr {
            Package {
                id: 0,
                repo_name: "local".to_string(),
                pkg_id: self.pkg_id.clone(),
                pkg_family: self.pkg_family.clone(),
                pkg_name: self.pkg_name.clone(),
                pkg_type: self.pkg_type.clone(),
                version: self.version.clone(),
                download_url: String::new(),
                ghcr_pkg: Some(self.url.clone()),
                ghcr_size: self.size,
                description: format!("Installed from {}", self.url),
                ..Default::default()
            }
        } else {
            Package {
                id: 0,
                repo_name: "local".to_string(),
                pkg_id: self.pkg_id.clone(),
                pkg_family: self.pkg_family.clone(),
                pkg_name: self.pkg_name.clone(),
                pkg_type: self.pkg_type.clone(),
                version: self.version.clone(),
                download_url: self.url.clone(),
                size: self.size,
                description: format!("Installed from {}", self.url),
                ..Default::default()
            }
        }
    }
}

/// Extract the family from a URL: host plus the first two path segments.
///
/// Examples:
/// - `https://github.com/user/repo/...` → `github.com.user.repo`
/// - `https://example.com/foo/bar/...` → `example.com.foo.bar`
fn extract_family_from_url(url: &str) -> Option<String> {
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
pub(crate) fn detect_pkg_type(filename: &str) -> Option<String> {
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

/// Architecture words, which are never a package name.
///
/// `x86_64` is why this exists: its trailing `64` reads as a version and its
/// leading `x86` as part of the name, so `tool_x86_64-unknown-linux-musl`
/// parses as `tool_x86` at version `64`.
const ARCH_WORDS: &[&str] = &[
    "x86_64", "ppc64le", "riscv64", "loong64", "aarch64", "armv7l", "armhf", "arm64", "amd64",
    "s390x", "i686", "i386",
];

/// Platform words, which a package may legitimately be called.
///
/// These are only dropped once the name is over, which the version or the
/// architecture marks the start of. A package called `windows` keeps its name;
/// the `windows` in `tool-1.0-windows-x86_64` does not survive.
const PLATFORM_WORDS: &[&str] = &[
    "unknown", "anylinux", "linux", "darwin", "apple", "windows", "musl", "gnu", "static",
];

/// Drop the platform decoration from a filename, leaving what names the
/// package and, where the name carries one, its version.
fn strip_platform_words(base: &str) -> String {
    // Separators are kept as they were: the name pattern joins words with `_`
    // but not with `-`, so rejoining with either would rename the package.
    let mut words: Vec<(String, char)> = Vec::new();
    let mut current = String::new();
    for ch in base.chars() {
        if matches!(ch, '-' | '_') {
            words.push((std::mem::take(&mut current), ch));
        } else {
            current.push(ch);
        }
    }
    words.push((current, '\0'));

    // Rejoin the architecture words a separator split apart, so `x86_64` is one
    // word rather than `x86` followed by something that looks like a version.
    let mut i = 0;
    while i + 1 < words.len() {
        let joined = format!("{}{}{}", words[i].0, words[i].1, words[i + 1].0);
        if ARCH_WORDS.contains(&joined.to_lowercase().as_str()) {
            let sep = words[i + 1].1;
            words[i] = (joined, sep);
            words.remove(i + 1);
            continue;
        }
        i += 1;
    }

    let is_arch = |w: &str| ARCH_WORDS.contains(&w.to_lowercase().as_str());
    let is_platform = |w: &str| PLATFORM_WORDS.contains(&w.to_lowercase().as_str());
    // Everything up to the version or the architecture is the name, however
    // much of it reads like a platform.
    let name_ends = words
        .iter()
        .position(|(w, _)| is_arch(w) || w.starts_with(|c: char| c.is_ascii_digit()))
        .unwrap_or(words.len());

    let mut out = String::new();
    for (index, (word, sep)) in words.iter().enumerate() {
        if word.is_empty() || (index >= name_ends && (is_arch(word) || is_platform(word))) {
            continue;
        }
        if !out.is_empty() {
            out.push(if index == 0 { '-' } else { words[index - 1].1 });
        }
        out.push_str(word);
        let _ = sep;
    }
    out
}

/// Parse filename to extract name and version.
///
/// Handles common patterns like:
///
/// - `Name-Version-platform.ext`
/// - `name_version.ext`
/// - `name-version.ext`
pub fn parse_filename(filename: &str) -> (String, String) {
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
    let base = strip_platform_words(&remove_extensions(filename));

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
        assert_eq!(pkg.pkg_family.as_deref(), Some("github.com.user.repo"));
    }

    #[test]
    fn test_parse_with_pkg_type_override() {
        let url = "https://example.com/downloads/app";
        let pkg = UrlPackage::from_url(url, Some("myapp"), Some("1.0.0"), Some("appimage"), None)
            .unwrap();

        assert_eq!(pkg.pkg_name, "myapp");
        assert_eq!(pkg.version, "1.0.0");
        assert_eq!(pkg.pkg_type, Some("appimage".to_string()));
        assert_eq!(pkg.pkg_family.as_deref(), Some("example.com.downloads.app"));
    }

    #[test]
    fn test_extract_family_from_url() {
        assert_eq!(
            extract_family_from_url("https://github.com/pkgforge/soar/releases/file"),
            Some("github.com.pkgforge.soar".to_string())
        );
        assert_eq!(
            extract_family_from_url("https://gitlab.com/user/project/-/releases"),
            Some("gitlab.com.user.project".to_string())
        );
        assert_eq!(
            extract_family_from_url("https://example.com/foo/bar/baz"),
            Some("example.com.foo.bar".to_string())
        );
        assert_eq!(
            extract_family_from_url("https://example.com/app"),
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
        assert_eq!(pkg.pkg_family.as_deref(), Some("github.com.user.testrepo"));
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
    fn a_package_may_be_named_after_a_platform() {
        // The name runs until the version or the architecture, so a platform
        // word inside it is part of the name.
        let (name, ver) = parse_filename("windows-1.0-x86_64.AppImage");
        assert_eq!(name, "windows");
        assert_eq!(ver, "1.0");

        let (name, ver) = parse_filename("something_linux-1.0-x86_64.tar.gz");
        assert_eq!(name, "something_linux");
        assert_eq!(ver, "1.0");

        let (name, _) = parse_filename("linuxdeploy-x86_64.AppImage");
        assert_eq!(name, "linuxdeploy");

        // Past the version, the same words are decoration.
        let (name, ver) = parse_filename("tool-1.0-windows-x86_64.zip");
        assert_eq!(name, "tool");
        assert_eq!(ver, "1.0");
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
        // The platform words in an asset name are not a version, however much
        // the trailing digits of one look like it.
        let (name, ver) = parse_filename("eza_x86_64-unknown-linux-musl.tar.gz");
        assert_eq!(name, "eza");
        assert_eq!(ver, "unknown");

        let (name, ver) = parse_filename("ripgrep-15.2.0-x86_64-unknown-linux-musl.tar.gz");
        assert_eq!(name, "ripgrep");
        assert_eq!(ver, "15.2.0");

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
        assert_eq!(pkg.pkg_family.as_deref(), Some("pkgforge.soar"));
        assert!(pkg.is_ghcr);
    }

    #[test]
    fn test_ghcr_with_digest() {
        let ghcr = "ghcr.io/org/repo@sha256:deadbeef1234567890";
        let pkg = UrlPackage::from_ghcr(ghcr, None, None, None, None).unwrap();

        assert_eq!(pkg.pkg_name, "repo");
        assert_eq!(pkg.version, "sha256:deadbeef1234567890");
        assert_eq!(pkg.pkg_family.as_deref(), Some("org.repo"));
        assert!(pkg.is_ghcr);
    }

    #[test]
    fn test_ghcr_without_tag() {
        let ghcr = "ghcr.io/org/package";
        let pkg = UrlPackage::from_ghcr(ghcr, None, None, None, None).unwrap();

        assert_eq!(pkg.pkg_name, "package");
        assert_eq!(pkg.version, "latest");
        assert_eq!(pkg.pkg_family.as_deref(), Some("org.package"));
        assert!(pkg.is_ghcr);
    }

    #[test]
    fn test_ghcr_nested_package() {
        let ghcr = "ghcr.io/org/team/repo:1.0";
        let pkg = UrlPackage::from_ghcr(ghcr, None, None, None, None).unwrap();

        assert_eq!(pkg.pkg_name, "repo");
        assert_eq!(pkg.version, "1.0");
        assert_eq!(pkg.pkg_family.as_deref(), Some("org.team.repo"));
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
        assert_eq!(pkg.pkg_id.as_deref(), Some("custom-id"));
        assert!(pkg.is_ghcr);
    }

    #[test]
    fn test_ghcr_rejects_traversal_overrides() {
        let ghcr = "ghcr.io/org/repo:v1.0";

        assert!(UrlPackage::from_ghcr(ghcr, Some("../../evil"), None, None, None).is_err());
        assert!(UrlPackage::from_ghcr(ghcr, None, None, None, Some("../../evil")).is_err());
        assert!(UrlPackage::from_ghcr(ghcr, None, None, None, Some("/abs/evil")).is_err());
        assert!(UrlPackage::from_ghcr(ghcr, Some(".."), None, None, None).is_err());
    }

    #[test]
    fn test_url_rejects_traversal_overrides() {
        let url = "https://github.com/user/repo/releases/download/v1.0/app";

        assert!(UrlPackage::from_url(url, Some("../../evil"), None, None, None).is_err());
        assert!(UrlPackage::from_url(url, None, None, None, Some("../../evil")).is_err());
        assert!(UrlPackage::from_url(url, None, None, None, Some("a/b")).is_err());
    }

    #[test]
    fn test_ghcr_to_package() {
        let ghcr = "ghcr.io/pkgforge/soar:v0.8.1";
        let url_pkg = UrlPackage::from_ghcr(ghcr, None, None, None, None).unwrap();
        let pkg = url_pkg.to_package();

        assert_eq!(pkg.repo_name, "local");
        assert_eq!(pkg.pkg_name, "soar");
        assert_eq!(pkg.version, "0.8.1"); // 'v' prefix stripped
        assert_eq!(pkg.pkg_family.as_deref(), Some("pkgforge.soar"));
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
