//! Remote package update checking.
//!
//! This module provides functionality to check for updates to remote packages
//! (those installed via URL) using various update sources like GitHub/GitLab
//! releases APIs.

use soar_config::packages::UpdateSource;
use soar_dl::{
    github::{Github, GithubAsset, GithubRelease},
    gitlab::{GitLab, GitLabAsset, GitLabRelease},
    traits::{Asset, Platform, Release},
};

use crate::{error::SoarError, SoarResult};

/// Result of checking for a remote package update.
#[derive(Debug, Clone)]
pub struct RemoteUpdate {
    /// The new version available.
    pub new_version: String,
    /// Download URL for the new version.
    pub download_url: String,
    /// Optional size of the download in bytes.
    pub size: Option<u64>,
}

/// Check for updates to a remote package.
///
/// # Arguments
/// * `update_source` - The update source configuration
/// * `current_version` - The currently installed version
///
/// # Returns
/// * `Ok(Some(RemoteUpdate))` if a newer version is available
/// * `Ok(None)` if already at the latest version
/// * `Err` if the check fails
pub fn check_for_update(
    update_source: &UpdateSource,
    current_version: &str,
) -> SoarResult<Option<RemoteUpdate>> {
    match update_source {
        UpdateSource::GitHub {
            repo,
            asset_pattern,
            include_prerelease,
            tag_pattern,
        } => check_github(
            repo,
            asset_pattern.as_deref(),
            *include_prerelease,
            tag_pattern.as_deref(),
            current_version,
        ),
        UpdateSource::GitLab {
            repo,
            asset_pattern,
            include_prerelease,
            tag_pattern,
        } => check_gitlab(
            repo,
            asset_pattern.as_deref(),
            *include_prerelease,
            tag_pattern.as_deref(),
            current_version,
        ),
        UpdateSource::Url {
            url,
            version_path,
            download_path,
        } => check_url(url, version_path, download_path, current_version),
        UpdateSource::Command { command } => check_command(command, current_version),
    }
}

/// Check for updates via GitHub releases API.
fn check_github(
    repo: &str,
    asset_pattern: Option<&str>,
    include_prerelease: Option<bool>,
    tag_pattern: Option<&str>,
    current_version: &str,
) -> SoarResult<Option<RemoteUpdate>> {
    let releases: Vec<GithubRelease> = Github::fetch_releases(repo, None).map_err(|e| {
        SoarError::Custom(format!(
            "Failed to fetch GitHub releases for {}: {}",
            repo, e
        ))
    })?;

    let include_prerelease = include_prerelease.unwrap_or(false);

    let release = releases.iter().find(|r: &&GithubRelease| {
        let prerelease_ok = include_prerelease || !r.is_prerelease();
        let tag_ok = tag_pattern.map_or(true, |p| fast_glob::glob_match(p, r.tag()));
        prerelease_ok && tag_ok
    });

    let Some(release) = release else {
        return Ok(None);
    };

    let new_version = release.tag();

    if !is_newer_version(current_version, new_version) {
        return Ok(None);
    }

    let assets: &[GithubAsset] = release.assets();
    let asset = find_matching_asset(assets, asset_pattern)?;

    Ok(Some(RemoteUpdate {
        new_version: new_version.to_string(),
        download_url: asset.url().to_string(),
        size: asset.size(),
    }))
}

/// Check for updates via GitLab releases API.
fn check_gitlab(
    repo: &str,
    asset_pattern: Option<&str>,
    include_prerelease: Option<bool>,
    tag_pattern: Option<&str>,
    current_version: &str,
) -> SoarResult<Option<RemoteUpdate>> {
    let releases: Vec<GitLabRelease> = GitLab::fetch_releases(repo, None).map_err(|e| {
        SoarError::Custom(format!(
            "Failed to fetch GitLab releases for {}: {}",
            repo, e
        ))
    })?;

    let include_prerelease = include_prerelease.unwrap_or(false);

    let release = releases.iter().find(|r: &&GitLabRelease| {
        let prerelease_ok = include_prerelease || !r.is_prerelease();
        let tag_ok = tag_pattern.map_or(true, |p| fast_glob::glob_match(p, r.tag()));
        prerelease_ok && tag_ok
    });

    let Some(release) = release else {
        return Ok(None);
    };

    let new_version = release.tag();

    if !is_newer_version(current_version, new_version) {
        return Ok(None);
    }

    let assets: &[GitLabAsset] = release.assets();
    let asset = find_matching_asset(assets, asset_pattern)?;

    Ok(Some(RemoteUpdate {
        new_version: new_version.to_string(),
        download_url: asset.url().to_string(),
        size: asset.size(),
    }))
}

/// Check for updates via custom URL endpoint.
fn check_url(
    url: &str,
    version_path: &str,
    download_path: &str,
    current_version: &str,
) -> SoarResult<Option<RemoteUpdate>> {
    use soar_dl::http::Http;

    let json: serde_json::Value = Http::json(url).map_err(|e| {
        SoarError::Custom(format!("Failed to fetch update info from {}: {}", url, e))
    })?;

    let new_version = extract_json_value(&json, version_path).ok_or_else(|| {
        SoarError::Custom(format!(
            "Could not find version at path '{}' in response",
            version_path
        ))
    })?;

    if !is_newer_version(current_version, &new_version) {
        return Ok(None);
    }

    let download_url = extract_json_value(&json, download_path).ok_or_else(|| {
        SoarError::Custom(format!(
            "Could not find download URL at path '{}' in response",
            download_path
        ))
    })?;

    if !is_valid_download_url(&download_url) {
        return Err(SoarError::Custom(format!(
            "Invalid download URL returned: {}",
            download_url
        )));
    }

    Ok(Some(RemoteUpdate {
        new_version,
        download_url,
        size: None,
    }))
}

/// Check for updates via shell command.
fn check_command(command: &str, current_version: &str) -> SoarResult<Option<RemoteUpdate>> {
    use std::process::Command;

    let output = Command::new("sh")
        .arg("-c")
        .arg(command)
        .output()
        .map_err(|e| SoarError::Custom(format!("Failed to execute update command: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SoarError::Custom(format!(
            "Update command failed: {}",
            stderr
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines = stdout.lines();

    let new_version = lines
        .next()
        .ok_or_else(|| SoarError::Custom("Update command returned no output".into()))?
        .trim()
        .to_string();

    if !is_newer_version(current_version, &new_version) {
        return Ok(None);
    }

    let download_url = lines
        .next()
        .ok_or_else(|| SoarError::Custom("Update command did not return download URL".into()))?
        .trim()
        .to_string();

    if !is_valid_download_url(&download_url) {
        return Err(SoarError::Custom(format!(
            "Invalid download URL returned by command: {}",
            download_url
        )));
    }

    Ok(Some(RemoteUpdate {
        new_version,
        download_url,
        size: None,
    }))
}

/// Validate that a download URL is properly formed.
///
/// Checks that the URL:
/// - Starts with http:// or https://
/// - Is a valid URL structure (has host, etc.)
fn is_valid_download_url(url: &str) -> bool {
    let url = url.trim();
    if url.is_empty() {
        return false;
    }

    let lower = url.to_lowercase();
    if !lower.starts_with("http://") && !lower.starts_with("https://") {
        return false;
    }

    match url::Url::parse(url) {
        Ok(parsed) => parsed.host().is_some(),
        Err(_) => false,
    }
}

/// Compare versions to determine if candidate is newer than current.
///
/// Uses semver comparison if both versions are valid semver, otherwise
/// treats any difference as potentially newer.
fn is_newer_version(current: &str, candidate: &str) -> bool {
    let current = current.strip_prefix('v').unwrap_or(current);
    let candidate = candidate.strip_prefix('v').unwrap_or(candidate);

    if current == candidate {
        return false;
    }

    match (
        semver::Version::parse(current),
        semver::Version::parse(candidate),
    ) {
        (Ok(cur), Ok(cand)) => cand > cur,
        // If semver parsing fails, treat different versions as newer
        _ => true,
    }
}

/// Find an asset matching the given pattern from a list of assets.
fn find_matching_asset<'a, A: Asset>(assets: &'a [A], pattern: Option<&str>) -> SoarResult<&'a A> {
    if assets.is_empty() {
        return Err(SoarError::Custom("No assets found in release".into()));
    }

    match pattern {
        Some(pattern) => {
            assets
                .iter()
                .find(|a| fast_glob::glob_match(pattern, a.name()))
                .ok_or_else(|| {
                    SoarError::Custom(format!(
                        "No asset matching pattern '{}' found. Available: {}",
                        pattern,
                        assets
                            .iter()
                            .map(|a| a.name())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))
                })
        }
        None => {
            // No pattern specified, return first asset
            Ok(&assets[0])
        }
    }
}

/// Extract a value from JSON using a simple dot-separated path.
fn extract_json_value(json: &serde_json::Value, path: &str) -> Option<String> {
    let mut current = json;

    for key in path.split('.') {
        // Handle array indexing like "assets[0]"
        if let Some((array_key, index_str)) = key.split_once('[') {
            let index_str = index_str.trim_end_matches(']');
            let index: usize = index_str.parse().ok()?;

            current = current.get(array_key)?;
            current = current.get(index)?;
        } else {
            current = current.get(key)?;
        }
    }

    match current {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_newer_version_semver() {
        assert!(is_newer_version("1.0.0", "1.0.1"));
        assert!(is_newer_version("1.0.0", "1.1.0"));
        assert!(is_newer_version("1.0.0", "2.0.0"));
        assert!(!is_newer_version("1.0.0", "1.0.0"));
        assert!(!is_newer_version("2.0.0", "1.0.0"));
    }

    #[test]
    fn test_is_newer_version_with_v_prefix() {
        assert!(is_newer_version("v1.0.0", "v1.0.1"));
        assert!(is_newer_version("1.0.0", "v1.0.1"));
        assert!(is_newer_version("v1.0.0", "1.0.1"));
    }

    #[test]
    fn test_is_newer_version_non_semver() {
        // Non-semver versions: treat any difference as potentially newer
        assert!(is_newer_version("abc", "def"));
        assert!(is_newer_version("HEAD-123", "HEAD-456"));
    }

    #[test]
    fn test_extract_json_value() {
        let json: serde_json::Value = serde_json::json!({
            "tag_name": "v1.0.0",
            "assets": [
                {"name": "app.zip", "browser_download_url": "https://example.com/app.zip"}
            ]
        });

        assert_eq!(
            extract_json_value(&json, "tag_name"),
            Some("v1.0.0".to_string())
        );
        assert_eq!(
            extract_json_value(&json, "assets[0].name"),
            Some("app.zip".to_string())
        );
        assert_eq!(
            extract_json_value(&json, "assets[0].browser_download_url"),
            Some("https://example.com/app.zip".to_string())
        );
        assert_eq!(extract_json_value(&json, "nonexistent"), None);
    }

    #[test]
    fn test_is_valid_download_url() {
        // Valid URLs
        assert!(is_valid_download_url("https://example.com/file.AppImage"));
        assert!(is_valid_download_url("http://example.com/file"));
        assert!(is_valid_download_url(
            "https://github.com/user/repo/releases/download/v1.0/app.zip"
        ));

        // Invalid URLs
        assert!(!is_valid_download_url("")); // empty
        assert!(!is_valid_download_url("   ")); // whitespace only
        assert!(!is_valid_download_url("https://")); // no host
        assert!(!is_valid_download_url("https://?query=1")); // no host
        assert!(!is_valid_download_url("not-a-url")); // no protocol
        assert!(!is_valid_download_url("ftp://example.com/file")); // wrong protocol
        assert!(!is_valid_download_url("file:///path/to/file")); // file protocol
    }
}
