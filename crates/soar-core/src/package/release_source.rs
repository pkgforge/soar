//! Release source resolution for GitHub/GitLab packages.
//!
//! This module provides functionality to resolve package sources from
//! GitHub or GitLab releases, fetching version and download URL automatically.

use std::{collections::HashMap, process::Command};

use soar_config::packages::ResolvedPackage;
use soar_dl::{
    github::{Github, GithubAsset, GithubRelease},
    gitlab::{GitLab, GitLabAsset, GitLabRelease},
    traits::{Asset, Platform, Release},
};

use crate::{
    error::SoarError, package::remote_update::is_valid_download_url,
    utils::substitute_placeholders, SoarResult,
};

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
        /// Custom architecture name mapping.
        arch_map: Option<HashMap<String, String>>,
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
        /// Custom architecture name mapping.
        arch_map: Option<HashMap<String, String>>,
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
    /// The releases a download URL came out of, where its host publishes any.
    ///
    /// A forge download URL names the project, the release it belongs to and
    /// the asset taken from it, which is everything needed to ask for the
    /// current release later. A URL from anywhere else answers nothing, and
    /// is reported as such rather than guessed at.
    pub fn from_download_url(url: &str) -> Option<Self> {
        let ReleaseDownload {
            is_github,
            owner,
            repo,
            tag,
            asset,
        } = ReleaseDownload::parse(url)?;
        let (owner, repo, tag, asset) = (&owner, &repo, &tag, &asset);

        let source = if is_github {
            Self::GitHub {
                repo: format!("{owner}/{repo}"),
                asset_pattern: asset_glob(tag, asset),
                include_prerelease: false,
                tag_pattern: None,
                arch_map: None,
            }
        } else {
            Self::GitLab {
                repo: format!("{owner}/{repo}"),
                asset_pattern: asset_glob(tag, asset),
                include_prerelease: false,
                tag_pattern: None,
                arch_map: None,
            }
        };
        Some(source)
    }

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
                arch_map: pkg.arch_map.clone(),
            });
        }

        if let Some(ref repo) = pkg.gitlab {
            let asset_pattern = pkg.asset_pattern.clone()?;
            return Some(ReleaseSource::GitLab {
                repo: repo.clone(),
                asset_pattern,
                include_prerelease: pkg.include_prerelease.unwrap_or(false),
                tag_pattern: pkg.tag_pattern.clone(),
                arch_map: pkg.arch_map.clone(),
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
                arch_map,
            } => {
                resolve_github(
                    repo,
                    asset_pattern,
                    *include_prerelease,
                    tag_pattern.as_deref(),
                    version,
                    arch_map.as_ref(),
                )
            }
            ReleaseSource::GitLab {
                repo,
                asset_pattern,
                include_prerelease,
                tag_pattern,
                arch_map,
            } => {
                resolve_gitlab(
                    repo,
                    asset_pattern,
                    *include_prerelease,
                    tag_pattern.as_deref(),
                    version,
                    arch_map.as_ref(),
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
/// A download URL taken apart into the release it came from.
struct ReleaseDownload {
    is_github: bool,
    owner: String,
    repo: String,
    tag: String,
    asset: String,
}

impl ReleaseDownload {
    fn parse(url: &str) -> Option<Self> {
        let parsed = url::Url::parse(url).ok()?;
        let host = parsed.host_str()?;
        let decoded: Vec<String> = parsed
            .path_segments()?
            .map(|s| {
                percent_encoding::percent_decode_str(s)
                    .decode_utf8_lossy()
                    .to_string()
            })
            .collect();
        let segments: Vec<&str> = decoded.iter().map(String::as_str).collect();

        // github.com/{owner}/{repo}/releases/download/{tag}/{asset}
        // gitlab.com/{owner}/{repo}/-/releases/{tag}/downloads/{asset}
        let (is_github, owner, repo, tag, asset) = match segments.as_slice() {
            [owner, repo, "releases", "download", tag, asset] if host == "github.com" => {
                (true, owner, repo, tag, asset)
            }
            [owner, repo, "-", "releases", tag, "downloads", asset] if host == "gitlab.com" => {
                (false, owner, repo, tag, asset)
            }
            _ => return None,
        };
        Some(Self {
            is_github,
            owner: owner.to_string(),
            repo: repo.to_string(),
            tag: tag.to_string(),
            asset: asset.to_string(),
        })
    }
}

/// The version a forge release URL states outright.
///
/// A release is tagged with its version, while the asset in it is named
/// however the project chose: plenty carry no version at all, and one named
/// for a platform can be mistaken for carrying one.
pub fn version_from_release_url(url: &str) -> Option<String> {
    let tag = ReleaseDownload::parse(url)?.tag;
    // Tags carry build metadata after an `@` that no version should show.
    let version = tag.split('@').next().unwrap_or(&tag);
    let version = version.strip_prefix('v').unwrap_or(version);
    (!version.is_empty() && version.starts_with(|c: char| c.is_ascii_digit()))
        .then(|| version.to_string())
}

/// A glob matching this asset across releases.
///
/// An asset is named after the release it belongs to, so the name as-is only
/// ever matches the release it came from. Taking the version out of it leaves
/// what stays the same from one release to the next, which is the platform and
/// the extension: `tool-1.2.3-linux-x86_64.tar.gz` from tag `v1.2.3` becomes
/// `tool-*-linux-x86_64.tar.gz`.
fn asset_glob(tag: &str, asset: &str) -> String {
    // Publishers append build metadata to the tag that the asset does not
    // carry, and a leading `v` that it usually does not either.
    let version = tag.split(['@', '+']).next().unwrap_or(tag);
    for candidate in [version, version.strip_prefix('v').unwrap_or(version)] {
        if !candidate.is_empty() && asset.contains(candidate) {
            return asset.replace(candidate, "*");
        }
    }
    asset.to_string()
}

fn resolve_github(
    repo: &str,
    asset_pattern: &str,
    include_prerelease: bool,
    tag_pattern: Option<&str>,
    specific_version: Option<&str>,
    arch_map: Option<&HashMap<String, String>>,
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
    let asset_pattern = substitute_placeholders(asset_pattern, Some(release.tag()), arch_map);
    let asset = find_matching_asset(assets, &asset_pattern)?;

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
    arch_map: Option<&HashMap<String, String>>,
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
    let asset_pattern = substitute_placeholders(asset_pattern, Some(release.tag()), arch_map);
    let asset = find_matching_asset(assets, &asset_pattern)?;

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

/// Result of running a version command.
#[derive(Debug, Clone)]
pub struct VersionCommandResult {
    /// The version string (line 1).
    pub version: String,
    /// The download URL (line 2, optional).
    /// If not provided, the `url` field from config should be used with {version} substituted.
    pub download_url: Option<String>,
    /// Optional size in bytes (line 3).
    pub size: Option<u64>,
}

/// Execute a version command and return version, optional URL, and optional size.
///
/// The command is executed via `sh -c` and should output:
/// - Line 1: version string (required)
/// - Line 2: download URL (optional - if omitted, use `url` field with {version} placeholder)
/// - Line 3: size in bytes (optional)
///
/// Leading/trailing whitespace is trimmed from each line.
pub fn run_version_command(command: &str) -> SoarResult<VersionCommandResult> {
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

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines = stdout.lines();

    let version = lines
        .next()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| SoarError::Custom("Version command returned empty output".into()))?;

    let download_url = lines
        .next()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && is_valid_download_url(s));

    let size = lines.next().and_then(|s| s.trim().parse::<u64>().ok());

    Ok(VersionCommandResult {
        version,
        download_url,
        size,
    })
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
                ..
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
                ..
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

    #[test]
    fn a_github_download_names_the_releases_it_came_from() {
        // The tag is percent-encoded in the path, as one carrying build
        // metadata has to be.
        let source = ReleaseSource::from_download_url(
            "https://github.com/owner/repo/releases/download/\
             1.2.3-abcdef%402026-04-01_1775061744/\
             tool-1.2.3-abcdef-linux-x86_64.AppImage",
        )
        .unwrap();
        match source {
            ReleaseSource::GitHub {
                repo,
                asset_pattern,
                ..
            } => {
                assert_eq!(repo, "owner/repo");
                assert_eq!(asset_pattern, "tool-*-linux-x86_64.AppImage");
            }
            other => panic!("expected GitHub, got {other:?}"),
        }
    }

    #[test]
    fn the_version_comes_out_of_the_asset_name() {
        // The tag carries build metadata the asset does not.
        assert_eq!(
            asset_glob(
                "1.2.3-1@2026-08-01_1785586116",
                "tool-1.2.3-1-linux-x86_64.AppImage"
            ),
            "tool-*-linux-x86_64.AppImage"
        );
        // A tag the asset spells without its `v`.
        assert_eq!(
            asset_glob("v1.2.3", "tool-1.2.3-x86_64-unknown-linux-musl.tar.gz"),
            "tool-*-x86_64-unknown-linux-musl.tar.gz"
        );
        // Nothing of the tag in the name leaves the name alone.
        assert_eq!(
            asset_glob("nightly", "tool-x86_64.AppImage"),
            "tool-x86_64.AppImage"
        );
    }

    #[test]
    fn a_url_no_forge_publishes_releases_for_answers_nothing() {
        assert!(ReleaseSource::from_download_url("https://example.com/app.AppImage").is_none());
        assert!(ReleaseSource::from_download_url(
            "https://github.com/owner/repo/archive/refs/tags/v1.0.tar.gz"
        )
        .is_none());
    }
}
