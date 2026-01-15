//! Release source resolution for GitHub/GitLab packages.
//!
//! This module provides functionality to resolve package sources from
//! GitHub or GitLab releases, fetching version and download URL automatically.

use std::process::Command;

use soar_config::packages::ResolvedPackage;
use soar_dl::{
    github::{Github, GithubAsset, GithubRelease},
    gitlab::{GitLab, GitLabAsset, GitLabRelease},
    traits::{Asset, Platform, Release},
};

use crate::{error::SoarError, SoarResult};

/// Source for fetching package releases.
#[derive(Debug, Clone)]
pub enum ReleaseSource {
    /// GitHub releases source.
    GitHub {
        /// Repository in "owner/repo" format.
        repo: String,
        /// Glob pattern to match asset filename.
        asset_pattern: String,
        /// Whether to include pre-release versions.
        include_prerelease: bool,
        /// Optional glob pattern to match tag names.
        tag_pattern: Option<String>,
    },
    /// GitLab releases source.
    GitLab {
        /// Repository in "owner/repo" format.
        repo: String,
        /// Glob pattern to match asset filename.
        asset_pattern: String,
        /// Whether to include pre-release versions.
        include_prerelease: bool,
        /// Optional glob pattern to match tag names.
        tag_pattern: Option<String>,
    },
}

/// Result of resolving a release source.
#[derive(Debug, Clone)]
pub struct ResolvedRelease {
    /// The version tag from the release.
    pub version: String,
    /// Download URL for the matched asset.
    pub download_url: String,
    /// Optional size of the download in bytes.
    pub size: Option<u64>,
}

impl ReleaseSource {
    /// Create a ReleaseSource from a resolved package configuration.
    ///
    /// Returns `None` if the package doesn't have github/gitlab source configured.
    pub fn from_resolved(pkg: &ResolvedPackage) -> Option<Self> {
        if let Some(ref repo) = pkg.github {
            let asset_pattern = pkg.asset_pattern.clone()?;
            return Some(ReleaseSource::GitHub {
                repo: repo.clone(),
                asset_pattern,
                include_prerelease: pkg.include_prerelease.unwrap_or(false),
                tag_pattern: pkg.tag_pattern.clone(),
            });
        }

        if let Some(ref repo) = pkg.gitlab {
            let asset_pattern = pkg.asset_pattern.clone()?;
            return Some(ReleaseSource::GitLab {
                repo: repo.clone(),
                asset_pattern,
                include_prerelease: pkg.include_prerelease.unwrap_or(false),
                tag_pattern: pkg.tag_pattern.clone(),
            });
        }

        None
    }

    /// Resolve the release source to get version and download URL.
    ///
    /// Fetches releases from the configured source, finds the latest
    /// (non-prerelease unless configured), matches the asset pattern,
    /// and returns the resolved release info.
    pub fn resolve(&self) -> SoarResult<ResolvedRelease> {
        self.resolve_version(None)
    }

    /// Resolve the release source with a specific version/tag.
    ///
    /// If `version` is Some, fetches that specific tag instead of the latest.
    /// The version can be with or without 'v' prefix (both "1.0.0" and "v1.0.0" work).
    pub fn resolve_version(&self, version: Option<&str>) -> SoarResult<ResolvedRelease> {
        match self {
            ReleaseSource::GitHub {
                repo,
                asset_pattern,
                include_prerelease,
                tag_pattern,
            } => {
                resolve_github(
                    repo,
                    asset_pattern,
                    *include_prerelease,
                    tag_pattern.as_deref(),
                    version,
                )
            }
            ReleaseSource::GitLab {
                repo,
                asset_pattern,
                include_prerelease,
                tag_pattern,
            } => {
                resolve_gitlab(
                    repo,
                    asset_pattern,
                    *include_prerelease,
                    tag_pattern.as_deref(),
                    version,
                )
            }
        }
    }
}

/// Check if a release matches the tag pattern.
fn matches_tag_pattern(tag: &str, pattern: Option<&str>) -> bool {
    match pattern {
        Some(p) => fast_glob::glob_match(p, tag),
        None => true,
    }
}

/// Resolve a GitHub release source.
fn resolve_github(
    repo: &str,
    asset_pattern: &str,
    include_prerelease: bool,
    tag_pattern: Option<&str>,
    specific_version: Option<&str>,
) -> SoarResult<ResolvedRelease> {
    let releases: Vec<GithubRelease> = Github::fetch_releases(repo, None).map_err(|e| {
        SoarError::Custom(format!(
            "Failed to fetch GitHub releases for {}: {}",
            repo, e
        ))
    })?;

    let release = releases
        .iter()
        .find(|r| {
            // If a specific version is requested, match it exactly (with or without 'v' prefix)
            if let Some(ver) = specific_version {
                let tag = r.tag();
                let tag_normalized = tag.strip_prefix('v').unwrap_or(tag);
                let ver_normalized = ver.strip_prefix('v').unwrap_or(ver);
                return tag_normalized == ver_normalized || tag == ver;
            }

            let prerelease_ok = include_prerelease || !r.is_prerelease();
            let tag_ok = matches_tag_pattern(r.tag(), tag_pattern);
            prerelease_ok && tag_ok
        })
        .ok_or_else(|| {
            if let Some(ver) = specific_version {
                SoarError::Custom(format!(
                    "No release found for {} with version '{}'",
                    repo, ver
                ))
            } else if let Some(pattern) = tag_pattern {
                SoarError::Custom(format!(
                    "No releases found for {} matching tag pattern '{}'",
                    repo, pattern
                ))
            } else {
                SoarError::Custom(format!("No releases found for {}", repo))
            }
        })?;

    let assets: &[GithubAsset] = release.assets();
    let asset = find_matching_asset(assets, asset_pattern)?;

    Ok(ResolvedRelease {
        version: release.tag().to_string(),
        download_url: asset.url().to_string(),
        size: asset.size(),
    })
}

/// Resolve a GitLab release source.
fn resolve_gitlab(
    repo: &str,
    asset_pattern: &str,
    include_prerelease: bool,
    tag_pattern: Option<&str>,
    specific_version: Option<&str>,
) -> SoarResult<ResolvedRelease> {
    let releases: Vec<GitLabRelease> = GitLab::fetch_releases(repo, None).map_err(|e| {
        SoarError::Custom(format!(
            "Failed to fetch GitLab releases for {}: {}",
            repo, e
        ))
    })?;

    let release = releases
        .iter()
        .find(|r| {
            // If a specific version is requested, match it exactly (with or without 'v' prefix)
            if let Some(ver) = specific_version {
                let tag = r.tag();
                let tag_normalized = tag.strip_prefix('v').unwrap_or(tag);
                let ver_normalized = ver.strip_prefix('v').unwrap_or(ver);
                return tag_normalized == ver_normalized || tag == ver;
            }

            let prerelease_ok = include_prerelease || !r.is_prerelease();
            let tag_ok = matches_tag_pattern(r.tag(), tag_pattern);
            prerelease_ok && tag_ok
        })
        .ok_or_else(|| {
            if let Some(ver) = specific_version {
                SoarError::Custom(format!(
                    "No release found for {} with version '{}'",
                    repo, ver
                ))
            } else if let Some(pattern) = tag_pattern {
                SoarError::Custom(format!(
                    "No releases found for {} matching tag pattern '{}'",
                    repo, pattern
                ))
            } else {
                SoarError::Custom(format!("No releases found for {}", repo))
            }
        })?;

    let assets: &[GitLabAsset] = release.assets();
    let asset = find_matching_asset(assets, asset_pattern)?;

    Ok(ResolvedRelease {
        version: release.tag().to_string(),
        download_url: asset.url().to_string(),
        size: asset.size(),
    })
}

/// Find an asset matching the given glob pattern.
fn find_matching_asset<'a, A: Asset>(assets: &'a [A], pattern: &str) -> SoarResult<&'a A> {
    if assets.is_empty() {
        return Err(SoarError::Custom("No assets found in release".into()));
    }

    assets
        .iter()
        .find(|a| fast_glob::glob_match(pattern, a.name()))
        .ok_or_else(|| {
            let available = assets
                .iter()
                .map(|a| a.name())
                .collect::<Vec<_>>()
                .join(", ");
            SoarError::Custom(format!(
                "No asset matching pattern '{}' found. Available: {}",
                pattern, available
            ))
        })
}

/// Execute a version command and return the version string.
///
/// The command is executed via `sh -c` and should output a version
/// string on stdout. Leading/trailing whitespace is trimmed.
pub fn run_version_command(command: &str) -> SoarResult<String> {
    let output = Command::new("sh")
        .arg("-c")
        .arg(command)
        .output()
        .map_err(|e| SoarError::Custom(format!("Failed to execute version command: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SoarError::Custom(format!(
            "Version command failed: {}",
            stderr
        )));
    }

    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if version.is_empty() {
        return Err(SoarError::Custom(
            "Version command returned empty output".into(),
        ));
    }

    Ok(version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_release_source_from_resolved_github() {
        let pkg = ResolvedPackage {
            name: "test".to_string(),
            github: Some("user/repo".to_string()),
            asset_pattern: Some("*.AppImage".to_string()),
            include_prerelease: Some(true),
            ..Default::default()
        };

        let source = ReleaseSource::from_resolved(&pkg).unwrap();
        match source {
            ReleaseSource::GitHub {
                repo,
                asset_pattern,
                include_prerelease,
                tag_pattern,
            } => {
                assert_eq!(repo, "user/repo");
                assert_eq!(asset_pattern, "*.AppImage");
                assert!(include_prerelease);
                assert!(tag_pattern.is_none());
            }
            _ => panic!("Expected GitHub source"),
        }
    }

    #[test]
    fn test_release_source_from_resolved_gitlab() {
        let pkg = ResolvedPackage {
            name: "test".to_string(),
            gitlab: Some("group/project".to_string()),
            asset_pattern: Some("*.tar.gz".to_string()),
            ..Default::default()
        };

        let source = ReleaseSource::from_resolved(&pkg).unwrap();
        match source {
            ReleaseSource::GitLab {
                repo,
                asset_pattern,
                include_prerelease,
                tag_pattern,
            } => {
                assert_eq!(repo, "group/project");
                assert_eq!(asset_pattern, "*.tar.gz");
                assert!(!include_prerelease);
                assert!(tag_pattern.is_none());
            }
            _ => panic!("Expected GitLab source"),
        }
    }

    #[test]
    fn test_release_source_from_resolved_none() {
        let pkg = ResolvedPackage {
            name: "test".to_string(),
            url: Some("https://example.com/file".to_string()),
            ..Default::default()
        };

        assert!(ReleaseSource::from_resolved(&pkg).is_none());
    }

    #[test]
    fn test_release_source_requires_asset_pattern() {
        let pkg = ResolvedPackage {
            name: "test".to_string(),
            github: Some("user/repo".to_string()),
            asset_pattern: None, // Missing!
            ..Default::default()
        };

        assert!(ReleaseSource::from_resolved(&pkg).is_none());
    }
}
