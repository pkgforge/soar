//! Release source resolution for packages published on a git forge.
//!
//! This module resolves a package source to a concrete version and download
//! URL by asking the forge which releases a project has.

use std::{collections::HashMap, process::Command};

use soar_config::packages::ResolvedPackage;
use soar_dl::{
    error::DownloadError,
    forge::Forge,
    platform::parse_gitea_target,
    releasekit::{Asset, Release},
};

use crate::{
    error::SoarError, package::remote_update::is_valid_download_url,
    utils::substitute_placeholders, SoarResult,
};

/// Which releases a source will take.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Prerelease {
    /// The newest release that is not a prerelease.
    #[default]
    Exclude,
    /// The newest release, prerelease or not.
    Include,
    /// The newest prerelease, and nothing else.
    Only,
}

/// Where a package's releases come from, and which of its assets to take.
#[derive(Debug, Clone)]
pub struct ReleaseSource {
    /// The forge publishing the releases.
    pub forge: Forge,
    /// Repository in "owner/repo" format.
    pub repo: String,
    /// Glob pattern to match asset filename.
    pub asset_pattern: String,
    /// Which releases to consider.
    pub prerelease: Prerelease,
    /// Optional glob pattern to match tag names.
    pub tag_pattern: Option<String>,
    /// Custom architecture name mapping.
    pub arch_map: Option<HashMap<String, String>>,
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
    /// A source taking `asset_pattern` from the releases of `repo` on `forge`.
    pub fn new(forge: Forge, repo: impl Into<String>, asset_pattern: impl Into<String>) -> Self {
        Self {
            forge,
            repo: repo.into(),
            asset_pattern: asset_pattern.into(),
            prerelease: Prerelease::default(),
            tag_pattern: None,
            arch_map: None,
        }
    }

    /// The releases a download URL came out of, where its host publishes any.
    ///
    /// A forge download URL names the project, the release it belongs to and
    /// the asset taken from it, which is everything needed to ask for the
    /// current release later. A URL from anywhere else answers nothing, and
    /// is reported as such rather than guessed at.
    pub fn from_download_url(url: &str) -> Option<Self> {
        let ReleaseDownload {
            forge,
            repo,
            tag,
            asset,
        } = ReleaseDownload::parse(url)?;

        Some(Self::new(forge, repo, asset_glob(&tag, &asset)))
    }

    /// Create a ReleaseSource from a resolved package configuration.
    ///
    /// `Ok(None)` is a package that names no forge at all, while a package
    /// that names one soar cannot use is an error saying which part of the
    /// declaration is missing.
    pub fn from_resolved(pkg: &ResolvedPackage) -> SoarResult<Option<Self>> {
        let (forge, repo) = if let Some(ref repo) = pkg.github {
            (Forge::GitHub, repo.clone())
        } else if let Some(ref repo) = pkg.gitlab {
            (Forge::GitLab, repo.clone())
        } else if let Some(ref repo) = pkg.codeberg {
            (Forge::Codeberg, repo.clone())
        } else if let Some(ref target) = pkg.gitea {
            let (instance, repo, _) = parse_gitea_target(target, false).ok_or_else(|| {
                SoarError::Custom(format!(
                    "'{target}' names no Gitea or Forgejo repository; \
                     a full repository URL does, such as \
                     'https://git.example.com/owner/repo'"
                ))
            })?;
            (
                Forge::Gitea {
                    instance,
                },
                repo,
            )
        } else {
            return Ok(None);
        };

        let asset_pattern = pkg.asset_pattern.clone().ok_or_else(|| {
            SoarError::Custom(format!(
                "no asset_pattern to pick an asset out of {repo}'s releases"
            ))
        })?;

        Ok(Some(Self {
            forge,
            repo,
            asset_pattern,
            prerelease: if pkg.include_prerelease.unwrap_or(false) {
                Prerelease::Include
            } else {
                Prerelease::Exclude
            },
            tag_pattern: pkg.tag_pattern.clone(),
            arch_map: pkg.arch_map.clone(),
        }))
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
        let release = match version {
            Some(version) => self.tagged_release(version)?,
            None => self.newest_release()?,
        };

        let asset_pattern = substitute_placeholders(
            &self.asset_pattern,
            Some(release.tag()),
            self.arch_map.as_ref(),
        );
        let asset = find_matching_asset(release.assets(), &asset_pattern)?;

        Ok(ResolvedRelease {
            version: release.tag().to_string(),
            download_url: asset.url().to_string(),
            size: asset.size(),
        })
    }

    /// The release a version names.
    ///
    /// A forge answers for one tag directly, which a listing cannot: it stops
    /// at the newest hundred or so releases, and a pinned version is usually
    /// older than that. Publishers disagree on the leading `v`, so a version
    /// spelled without one is also asked for with it, and the other way round.
    fn tagged_release(&self, version: &str) -> SoarResult<Release> {
        let alternate = alternate_spelling(version);

        let mut failure = None;
        for tag in [version, alternate.as_str()] {
            match self.forge.fetch_releases(&self.repo, Some(tag)) {
                Ok(releases) => {
                    if let Some(release) = releases.into_iter().next() {
                        return Ok(release);
                    }
                }
                // A tag the project does not have, which is the other
                // spelling's turn rather than an error.
                Err(DownloadError::HttpError {
                    status: 404, ..
                }) => {}
                Err(err) => failure = Some(err),
            }
        }

        Err(match failure {
            Some(err) => self.fetch_error(err),
            None => self.no_release_error(Some(version)),
        })
    }

    /// The newest release this source accepts.
    fn newest_release(&self) -> SoarResult<Release> {
        let releases = self
            .forge
            .fetch_releases(&self.repo, None)
            .map_err(|err| self.fetch_error(err))?;

        releases
            .into_iter()
            .find(|r| {
                let prerelease_ok = match self.prerelease {
                    Prerelease::Exclude => !r.is_prerelease(),
                    Prerelease::Include => true,
                    Prerelease::Only => r.is_prerelease(),
                };
                prerelease_ok && matches_tag_pattern(r.tag(), self.tag_pattern.as_deref())
            })
            .ok_or_else(|| self.no_release_error(None))
    }

    fn fetch_error(&self, err: DownloadError) -> SoarError {
        SoarError::Custom(format!(
            "Failed to fetch {} releases for {}: {}",
            self.forge, self.repo, err
        ))
    }

    /// Why nothing the project published fits what was asked for.
    fn no_release_error(&self, version: Option<&str>) -> SoarError {
        if let Some(ver) = version {
            SoarError::Custom(format!(
                "No release found for {} with version '{}'",
                self.repo, ver
            ))
        } else if self.prerelease == Prerelease::Only {
            SoarError::Custom(format!("No prerelease found for {}", self.repo))
        } else if let Some(ref pattern) = self.tag_pattern {
            SoarError::Custom(format!(
                "No releases found for {} matching tag pattern '{}'",
                self.repo, pattern
            ))
        } else {
            SoarError::Custom(format!("No releases found for {}", self.repo))
        }
    }
}

/// A version as the other convention spells it, with the leading `v` added
/// where it is missing and dropped where it is not.
fn alternate_spelling(version: &str) -> String {
    match version.strip_prefix('v') {
        Some(bare) => bare.to_string(),
        None => format!("v{version}"),
    }
}

/// Check if a release matches the tag pattern.
fn matches_tag_pattern(tag: &str, pattern: Option<&str>) -> bool {
    match pattern {
        Some(p) => fast_glob::glob_match(p, tag),
        None => true,
    }
}

/// A download URL taken apart into the release it came from.
struct ReleaseDownload {
    forge: Forge,
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

        // gitlab.com/{owner}/{repo}/-/releases/{tag}/downloads/{asset}
        if let [owner, repo, "-", "releases", tag, "downloads", asset] = segments.as_slice() {
            if host == "gitlab.com" {
                return Some(Self {
                    forge: Forge::GitLab,
                    repo: format!("{owner}/{repo}"),
                    tag: tag.to_string(),
                    asset: asset.to_string(),
                });
            }
        }

        // {prefix}/{owner}/{repo}/releases/download/{tag}/{asset}, which is
        // GitHub's shape and the one Gitea and Forgejo publish. Anything left
        // of the project is the path an instance is served under.
        let marker = (2..segments.len().saturating_sub(3))
            .find(|&i| segments[i] == "releases" && segments[i + 1] == "download")
            .filter(|&i| i + 4 == segments.len())?;
        let (prefix, owner, repo) = (
            &segments[..marker - 2],
            segments[marker - 2],
            segments[marker - 1],
        );

        let forge = match host {
            "github.com" | "codeberg.org" if !prefix.is_empty() => return None,
            "github.com" => Forge::GitHub,
            "codeberg.org" => Forge::Codeberg,
            // The port is part of the host, and the prefix part of the path,
            // so an instance is named by everything ahead of the project.
            _ => {
                let mut instance = parsed.origin().ascii_serialization();
                for segment in prefix {
                    instance.push('/');
                    instance.push_str(segment);
                }
                Forge::Gitea {
                    instance,
                }
            }
        };

        Some(Self {
            forge,
            repo: format!("{owner}/{repo}"),
            tag: segments[marker + 2].to_string(),
            asset: segments[marker + 3].to_string(),
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

/// Find an asset matching the given glob pattern.
fn find_matching_asset<'a>(assets: &'a [Asset], pattern: &str) -> SoarResult<&'a Asset> {
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

        let source = ReleaseSource::from_resolved(&pkg).unwrap().unwrap();
        assert_eq!(source.forge, Forge::GitHub);
        assert_eq!(source.repo, "user/repo");
        assert_eq!(source.asset_pattern, "*.AppImage");
        assert_eq!(source.prerelease, Prerelease::Include);
        assert!(source.tag_pattern.is_none());
    }

    #[test]
    fn test_release_source_from_resolved_gitlab() {
        let pkg = ResolvedPackage {
            name: "test".to_string(),
            gitlab: Some("group/project".to_string()),
            asset_pattern: Some("*.tar.gz".to_string()),
            ..Default::default()
        };

        let source = ReleaseSource::from_resolved(&pkg).unwrap().unwrap();
        assert_eq!(source.forge, Forge::GitLab);
        assert_eq!(source.repo, "group/project");
        assert_eq!(source.asset_pattern, "*.tar.gz");
        assert_eq!(source.prerelease, Prerelease::Exclude);
    }

    #[test]
    fn a_codeberg_package_resolves_against_codeberg() {
        let pkg = ResolvedPackage {
            name: "test".to_string(),
            codeberg: Some("user/repo".to_string()),
            asset_pattern: Some("*.AppImage".to_string()),
            ..Default::default()
        };

        let source = ReleaseSource::from_resolved(&pkg).unwrap().unwrap();
        assert_eq!(source.forge, Forge::Codeberg);
        assert_eq!(source.repo, "user/repo");
    }

    #[test]
    fn a_gitea_package_names_its_own_instance() {
        let pkg = ResolvedPackage {
            name: "test".to_string(),
            gitea: Some("https://git.example.com/user/repo".to_string()),
            asset_pattern: Some("*.AppImage".to_string()),
            ..Default::default()
        };

        let source = ReleaseSource::from_resolved(&pkg).unwrap().unwrap();
        assert_eq!(
            source.forge,
            Forge::Gitea {
                instance: "https://git.example.com".to_string()
            }
        );
        assert_eq!(source.repo, "user/repo");
    }

    #[test]
    fn test_release_source_from_resolved_none() {
        let pkg = ResolvedPackage {
            name: "test".to_string(),
            url: Some("https://example.com/file".to_string()),
            ..Default::default()
        };

        assert!(ReleaseSource::from_resolved(&pkg).unwrap().is_none());
    }

    #[test]
    fn test_release_source_requires_asset_pattern() {
        let pkg = ResolvedPackage {
            name: "test".to_string(),
            github: Some("user/repo".to_string()),
            asset_pattern: None, // Missing!
            ..Default::default()
        };

        let err = ReleaseSource::from_resolved(&pkg).unwrap_err().to_string();
        assert!(err.contains("asset_pattern"), "{err}");
    }

    #[test]
    fn a_gitea_package_without_an_instance_says_so() {
        let pkg = ResolvedPackage {
            name: "test".to_string(),
            gitea: Some("owner/repo".to_string()),
            asset_pattern: Some("*.AppImage".to_string()),
            ..Default::default()
        };

        let err = ReleaseSource::from_resolved(&pkg).unwrap_err().to_string();
        assert!(err.contains("repository URL"), "{err}");
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
        assert_eq!(source.forge, Forge::GitHub);
        assert_eq!(source.repo, "owner/repo");
        assert_eq!(source.asset_pattern, "tool-*-linux-x86_64.AppImage");
    }

    #[test]
    fn a_gitea_download_names_the_instance_it_came_from() {
        let source = ReleaseSource::from_download_url(
            "https://git.example.com/owner/repo/releases/download/v1.2.3/tool-1.2.3-x86_64.AppImage",
        )
        .unwrap();
        assert_eq!(
            source.forge,
            Forge::Gitea {
                instance: "https://git.example.com".to_string()
            }
        );
        assert_eq!(source.repo, "owner/repo");
        assert_eq!(source.asset_pattern, "tool-*-x86_64.AppImage");

        let codeberg = ReleaseSource::from_download_url(
            "https://codeberg.org/owner/repo/releases/download/v1.0/tool-1.0-x86_64.AppImage",
        )
        .unwrap();
        assert_eq!(codeberg.forge, Forge::Codeberg);
    }

    #[test]
    fn a_version_is_asked_for_both_ways_publishers_spell_it() {
        assert_eq!(alternate_spelling("1.2.3"), "v1.2.3");
        assert_eq!(alternate_spelling("v1.2.3"), "1.2.3");
        assert_eq!(alternate_spelling("nightly"), "vnightly");
    }

    #[test]
    fn a_gitea_instance_keeps_its_port_and_path() {
        let source = ReleaseSource::from_download_url(
            "https://git.example.com:3000/o/r/releases/download/v1.2.3/tool-1.2.3-x86_64.AppImage",
        )
        .unwrap();
        assert_eq!(
            source.forge,
            Forge::Gitea {
                instance: "https://git.example.com:3000".to_string()
            }
        );
        assert_eq!(source.repo, "o/r");

        let prefixed = ReleaseSource::from_download_url(
            "https://example.com/git/o/r/releases/download/v1.2.3/tool-1.2.3-x86_64.AppImage",
        )
        .unwrap();
        assert_eq!(
            prefixed.forge,
            Forge::Gitea {
                instance: "https://example.com/git".to_string()
            }
        );
        assert_eq!(prefixed.repo, "o/r");

        // A host soar knows serves its projects at the root, so a prefix
        // means the URL is something else.
        assert!(ReleaseSource::from_download_url(
            "https://github.com/x/o/r/releases/download/v1/tool.AppImage"
        )
        .is_none());
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
