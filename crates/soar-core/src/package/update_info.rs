//! The update information an AppImage carries about itself.
//!
//! An AppImage may record where its updates come from in a `.upd_info` ELF
//! section. Every form names a zsync control file, either directly or as an
//! asset of a forge release, so resolving one always ends at a URL soar can
//! fetch.

use std::path::Path;

use crate::{error::SoarError, package::release_source::ReleaseSource, SoarResult};

/// The section an AppImage records its update information in.
const SECTION: &str = ".upd_info";

/// Where an AppImage says its updates come from.
#[derive(Debug, Clone, PartialEq)]
pub enum UpdateInfo {
    /// A zsync control file at a fixed URL.
    Direct { url: String },
    /// A zsync control file published as a release asset.
    Forge {
        forge: Forge,
        repo: String,
        /// Release to take the asset from. `latest` unless a tag is named.
        tag: String,
        /// Glob matching the asset filename.
        filename: String,
    },
}

/// A forge that publishes releases soar can resolve an asset from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Forge {
    GitHub,
    GitLab,
}

impl UpdateInfo {
    /// Read the update information out of an installed AppImage.
    ///
    /// `None` covers everything that is not an AppImage carrying a form soar
    /// understands, which is not an error: such a package simply has no feed.
    pub fn from_artifact(path: impl AsRef<Path>) -> Option<Self> {
        let raw = soar_utils::elf::section_data(path, SECTION)?;
        Self::parse(std::str::from_utf8(&raw).ok()?)
    }

    /// The raw update information string an artifact carries, kept only when
    /// soar can resolve it.
    pub fn raw_from_artifact(path: impl AsRef<Path>) -> Option<String> {
        let raw = soar_utils::elf::section_data(path, SECTION)?;
        let raw = std::str::from_utf8(&raw).ok()?.trim().to_string();
        Self::parse(&raw).map(|_| raw)
    }

    /// Parse an update information string.
    pub fn parse(raw: &str) -> Option<Self> {
        let raw = raw.trim();
        if raw.is_empty() {
            return None;
        }
        let mut parts = raw.split('|');
        let kind = parts.next()?;
        let rest: Vec<&str> = parts.collect();

        match kind {
            "zsync" => {
                let url = rest.first()?.trim();
                (!url.is_empty()).then(|| {
                    Self::Direct {
                        url: url.to_string(),
                    }
                })
            }
            // Codeberg and Gitea are GitLab-shaped in the string but not in
            // their API, so they are left unresolved rather than resolved
            // against the wrong host.
            "gh-releases-zsync" | "gl-releases-zsync" => {
                let [owner, repo, tag, filename] = rest[..].try_into().ok()?;
                let forge = if kind == "gh-releases-zsync" {
                    Forge::GitHub
                } else {
                    Forge::GitLab
                };
                (!owner.is_empty() && !repo.is_empty() && !filename.is_empty()).then(|| {
                    Self::Forge {
                        forge,
                        repo: format!("{owner}/{repo}"),
                        tag: tag.to_string(),
                        filename: filename.to_string(),
                    }
                })
            }
            _ => None,
        }
    }

    /// The URL of the zsync control file this points at.
    ///
    /// A forge form is resolved against the release API, so this reaches the
    /// network; a direct form does not.
    pub fn zsync_url(&self) -> SoarResult<String> {
        match self {
            Self::Direct {
                url,
            } => Ok(url.clone()),
            Self::Forge {
                forge,
                repo,
                tag,
                filename,
            } => {
                // `latest-pre` is the only form that asks for a prerelease;
                // any other tag is matched literally.
                let include_prerelease = tag == "latest-pre";
                let tag_pattern = match tag.as_str() {
                    "latest" | "latest-pre" | "" => None,
                    other => Some(other.to_string()),
                };
                let source = match forge {
                    Forge::GitHub => {
                        ReleaseSource::GitHub {
                            repo: repo.clone(),
                            asset_pattern: filename.clone(),
                            include_prerelease,
                            tag_pattern,
                            arch_map: None,
                        }
                    }
                    Forge::GitLab => {
                        ReleaseSource::GitLab {
                            repo: repo.clone(),
                            asset_pattern: filename.clone(),
                            include_prerelease,
                            tag_pattern,
                            arch_map: None,
                        }
                    }
                };
                let release = source.resolve()?;
                if release.download_url.is_empty() {
                    return Err(SoarError::Custom(format!(
                        "no zsync asset matching '{filename}' in {repo}"
                    )));
                }
                Ok(release.download_url)
            }
        }
    }
}

/// The version implied by what a feed says about the artifact.
///
/// A published filename carries the version far more often than the artifact
/// records one anywhere else, and it is what the install derived its own
/// version from, so the two agree. Failing that, the build date orders one
/// release against the next, which is all an update check needs.
pub fn version_from_feed(filename: Option<&str>, mtime: Option<&str>) -> Option<String> {
    if let Some(name) = filename {
        let (_, version) = crate::package::url::parse_filename(name);
        if version != "unknown" {
            return Some(version);
        }
    }
    mtime.and_then(http_date_to_version)
}

/// `Mon, 01 Aug 2026 12:00:00 GMT` as `20260801`, which compares like a date.
fn http_date_to_version(raw: &str) -> Option<String> {
    chrono::DateTime::parse_from_rfc2822(raw)
        .ok()
        .map(|d| d.format("%Y%m%d").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_direct_feed() {
        assert_eq!(
            UpdateInfo::parse("zsync|https://e.test/App-x86_64.AppImage.zsync"),
            Some(UpdateInfo::Direct {
                url: "https://e.test/App-x86_64.AppImage.zsync".into()
            })
        );
    }

    #[test]
    fn parses_a_forge_feed() {
        assert_eq!(
            UpdateInfo::parse("gh-releases-zsync|probono|AppImages|latest|App*.AppImage.zsync"),
            Some(UpdateInfo::Forge {
                forge: Forge::GitHub,
                repo: "probono/AppImages".into(),
                tag: "latest".into(),
                filename: "App*.AppImage.zsync".into(),
            })
        );
    }

    #[test]
    fn a_direct_feed_needs_no_network_to_resolve() {
        let info = UpdateInfo::parse("zsync|https://e.test/a.zsync").unwrap();
        assert_eq!(info.zsync_url().unwrap(), "https://e.test/a.zsync");
    }

    #[test]
    fn version_comes_from_the_filename_before_the_date() {
        assert_eq!(
            version_from_feed(
                Some("Ruffle-2026.8.1-1-anylinux-x86_64.AppImage"),
                Some("Sat, 01 Aug 2026 07:28:36 +0000")
            )
            .as_deref(),
            Some("2026.8.1")
        );
    }

    #[test]
    fn a_nameless_build_falls_back_to_its_date() {
        assert_eq!(
            version_from_feed(
                Some("app.AppImage"),
                Some("Sat, 01 Aug 2026 07:28:36 +0000")
            )
            .as_deref(),
            Some("20260801")
        );
        assert_eq!(version_from_feed(None, None), None);
    }

    #[test]
    fn unknown_and_malformed_forms_are_not_a_feed() {
        // A form soar does not resolve, rather than one it resolves wrongly.
        assert_eq!(UpdateInfo::parse("cb-releases-zsync|o|r|latest|f"), None);
        assert_eq!(UpdateInfo::parse("gh-releases-zsync|o|r|latest"), None);
        assert_eq!(UpdateInfo::parse("zsync|"), None);
        assert_eq!(UpdateInfo::parse(""), None);
        assert_eq!(UpdateInfo::parse("bittorrent|x"), None);
    }
}
