//! Local package parsing for installing packages from a file on disk.
//!
//! Mirrors [`crate::package::url::UrlPackage`] but for a path that already
//! exists locally (e.g. `soar add ~/Downloads/foo.AppImage`). The file is
//! copied into the install directory instead of being downloaded.

use std::path::{Path, PathBuf};

use soar_utils::path::resolve_path;

use crate::{
    database::models::Package,
    error::SoarError,
    package::url::{detect_pkg_type, parse_filename},
    SoarResult,
};

/// URL scheme used to carry a local source path through the installer's
/// `download_url` field, so the download stage can branch to a local copy.
pub const LOCAL_SCHEME: &str = "file://";

/// Represents a package parsed from a local file path.
#[derive(Debug, Clone)]
pub struct LocalPackage {
    /// Absolute, expanded path to the source file
    pub path: PathBuf,
    /// Extracted or overridden package name
    pub pkg_name: String,
    /// Generated package ID (lowercase, normalized)
    pub pkg_id: String,
    /// Extracted or overridden version
    pub version: String,
    /// Detected package type from extension (e.g., "appimage")
    pub pkg_type: Option<String>,
    /// Size of the source file in bytes
    pub size: Option<u64>,
}

impl LocalPackage {
    /// Check if an input refers to an existing local path.
    ///
    /// Only path-like inputs are considered so that bare registry queries
    /// (e.g. `jq`, `repo/pkg`, `pkg#id`) are never accidentally treated as
    /// local files. To install a file in the current directory, reference it
    /// explicitly (e.g. `./foo.AppImage`).
    ///
    /// Existing directories match too (so [`Self::from_path`] can report a
    /// clear "not a file" error) rather than falling through to a confusing
    /// "package not found".
    pub fn is_local(input: &str) -> bool {
        let trimmed = input.trim();
        let candidate = trimmed.strip_prefix(LOCAL_SCHEME).unwrap_or(trimmed);

        let looks_path_like =
            candidate.starts_with('~') || candidate.starts_with('.') || candidate.contains('/');
        if !looks_path_like {
            return false;
        }

        resolve_path(candidate)
            .map(|p| p.exists())
            .unwrap_or(false)
    }

    /// Parse a local file path and extract package metadata.
    pub fn from_path(
        input: &str,
        name_override: Option<&str>,
        version_override: Option<&str>,
        pkg_type_override: Option<&str>,
        pkg_id_override: Option<&str>,
    ) -> SoarResult<Self> {
        let trimmed = input.trim();
        let candidate = trimmed.strip_prefix(LOCAL_SCHEME).unwrap_or(trimmed);

        let path = resolve_path(candidate)
            .map_err(|err| SoarError::Custom(format!("Invalid local path '{input}': {err}")))?;

        if !path.is_file() {
            return Err(SoarError::Custom(format!(
                "Local source is not a file: {}",
                path.display()
            )));
        }

        let filename = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .ok_or_else(|| {
                SoarError::Custom(format!("Could not extract filename from {}", path.display()))
            })?;

        let size = path.metadata().ok().map(|m| m.len());

        let mut pkg_type = pkg_type_override
            .map(|s| s.to_lowercase())
            .or_else(|| detect_pkg_type(&filename));

        // The filename extension is the primary signal, but local files may be
        // renamed or extensionless. When the extension tells us nothing, sniff
        // the contents so archives still get extracted. compak only matches
        // genuine archive signatures, so ELF binaries/AppImages stay as-is.
        if pkg_type.is_none() && compak::detect_from_file(&path).is_ok() {
            pkg_type = Some("archive".to_string());
        }

        let (extracted_name, extracted_version) = parse_filename(&filename);

        let pkg_name = name_override
            .map(|s| s.to_lowercase())
            .unwrap_or(extracted_name);

        // Normalize version by stripping a leading "v" for consistency.
        let version = version_override
            .map(|v| v.strip_prefix('v').unwrap_or(v).to_string())
            .unwrap_or(extracted_version);

        let pkg_id = pkg_id_override.map(String::from).unwrap_or_else(|| {
            if let Some(ref ptype) = pkg_type {
                format!("local.{pkg_name}-{ptype}")
            } else {
                format!("local.{pkg_name}")
            }
        });

        Ok(Self {
            path,
            pkg_name,
            pkg_id,
            version,
            pkg_type,
            size,
        })
    }

    /// Convert to a [`Package`] for installation.
    ///
    /// The source path is carried in `download_url` using the `file://`
    /// scheme so the installer can recognize and copy it locally.
    pub fn to_package(&self) -> Package {
        Package {
            id: 0,
            repo_name: "local".to_string(),
            pkg_id: self.pkg_id.clone(),
            pkg_name: self.pkg_name.clone(),
            pkg_type: self.pkg_type.clone(),
            version: self.version.clone(),
            download_url: format!("{LOCAL_SCHEME}{}", self.path.display()),
            size: self.size,
            description: format!("Installed from {}", self.path.display()),
            ..Default::default()
        }
    }
}

/// Strip the `file://` scheme from a download URL, returning the local path
/// if the URL refers to a local source.
pub fn local_path_from_url(download_url: &str) -> Option<&Path> {
    download_url
        .strip_prefix(LOCAL_SCHEME)
        .map(Path::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_local_rejects_bare_names() {
        assert!(!LocalPackage::is_local("jq"));
        assert!(!LocalPackage::is_local("jq#all"));
        assert!(!LocalPackage::is_local("repo/pkg")); // path-like but doesn't exist
        assert!(!LocalPackage::is_local("https://example.com/file.AppImage"));
    }

    #[test]
    fn test_is_local_detects_existing_file() {
        let tmp = std::env::temp_dir().join("soar_local_test_existing.AppImage");
        std::fs::write(&tmp, b"\x7fELF").unwrap();
        let path = tmp.to_string_lossy().into_owned();

        assert!(LocalPackage::is_local(&path));
        assert!(LocalPackage::is_local(&format!("file://{path}")));

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn test_from_path_parses_metadata() {
        let tmp = std::env::temp_dir().join("MyApp-2.0.1-x86_64.AppImage");
        std::fs::write(&tmp, b"\x7fELF").unwrap();

        let pkg =
            LocalPackage::from_path(&tmp.to_string_lossy(), None, None, None, None).unwrap();

        assert_eq!(pkg.pkg_name, "myapp");
        assert_eq!(pkg.version, "2.0.1");
        assert_eq!(pkg.pkg_type, Some("appimage".to_string()));
        assert_eq!(pkg.pkg_id, "local.myapp-appimage");

        let package = pkg.to_package();
        assert_eq!(package.repo_name, "local");
        assert!(package.download_url.starts_with("file://"));

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn test_from_path_sniffs_archive_without_extension() {
        // gzip magic bytes, but an extension the parser doesn't recognize.
        let tmp = std::env::temp_dir().join("soar_local_test_blob.bin");
        std::fs::write(&tmp, [0x1f, 0x8b, 0x08, 0x00]).unwrap();

        let pkg =
            LocalPackage::from_path(&tmp.to_string_lossy(), None, None, None, None).unwrap();
        assert_eq!(pkg.pkg_type, Some("archive".to_string()));

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn test_from_path_elf_is_not_archive() {
        let tmp = std::env::temp_dir().join("soar_local_test_binary");
        std::fs::write(&tmp, b"\x7fELF\x02\x01\x01\x00").unwrap();

        let pkg =
            LocalPackage::from_path(&tmp.to_string_lossy(), None, None, None, None).unwrap();
        assert_eq!(pkg.pkg_type, None);

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn test_local_path_from_url() {
        assert_eq!(
            local_path_from_url("file:///home/user/foo.AppImage"),
            Some(Path::new("/home/user/foo.AppImage"))
        );
        assert_eq!(local_path_from_url("https://example.com/foo"), None);
    }
}
