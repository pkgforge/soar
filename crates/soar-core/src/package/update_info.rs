//! The update information an AppImage carries about itself.
//!
//! An AppImage may record where its updates come from in a `.upd_info` ELF
//! section. Every form names a zsync control file, either directly or as an
//! asset of a forge release, so resolving one always ends at a URL soar can
//! fetch.
//!
//! The forms are the ones [appimageupdate] publishes:
//!
//! - `zsync|<url>`
//! - `gh-releases-zsync|<owner>|<repo>|<tag>|<filename>`
//! - `gl-releases-zsync|<owner>|<repo>|<tag>|<filename>`
//! - `cb-releases-zsync|<owner>|<repo>|<tag>|<filename>`
//! - `gitea-releases-zsync|<instance>|<owner>|<repo>|<tag>|<filename>`
//! - `forgejo-releases-zsync|<instance>|<owner>|<repo>|<tag>|<filename>`
//!
//! [appimageupdate]: https://github.com/pkgforge-dev/appimageupdate

use std::path::Path;

use soar_dl::forge::Forge;

use crate::{
    error::SoarError,
    package::release_source::{Prerelease, ReleaseSource},
    SoarResult,
};

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
            "gh-releases-zsync"
            | "gl-releases-zsync"
            | "cb-releases-zsync"
            | "gitea-releases-zsync"
            | "forgejo-releases-zsync" => {
                let (forge, fields): (Forge, &[&str]) = match kind {
                    "gh-releases-zsync" => (Forge::GitHub, &rest),
                    "gl-releases-zsync" => (Forge::GitLab, &rest),
                    "cb-releases-zsync" => (Forge::Codeberg, &rest),
                    _ => {
                        let (instance, fields) = rest.split_first()?;
                        let forge = Forge::Gitea {
                            instance: gitea_instance(instance)?,
                        };
                        (forge, fields)
                    }
                };
                let [owner, repo, tag, filename] = fields.try_into().ok()?;
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
                let (prerelease, exact_tag) = release_selection(tag);
                let source = ReleaseSource {
                    prerelease,
                    ..ReleaseSource::new(forge.clone(), repo, filename)
                };
                let release = source.resolve_version(exact_tag)?;
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

/// Which release a feed's tag field asks for.
///
/// `latest` takes the newest stable release, `latest-pre` the newest
/// prerelease and `latest-all` whichever of the two is newest. Any other tag
/// names one release, and names it exactly: a tag is not a pattern, and one
/// carrying `*` or `[` would otherwise select a different release.
fn release_selection(tag: &str) -> (Prerelease, Option<&str>) {
    match tag {
        "latest" | "" => (Prerelease::Exclude, None),
        "latest-pre" => (Prerelease::Only, None),
        "latest-all" => (Prerelease::Include, None),
        other => (Prerelease::Include, Some(other)),
    }
}

/// The base URL of a Gitea or Forgejo instance, as a feed spells it.
///
/// The scheme is optional there, and https is the only one worth assuming for
/// a host publishing releases.
fn gitea_instance(raw: &str) -> Option<String> {
    let instance = raw.trim().trim_end_matches('/');
    if instance.is_empty() {
        return None;
    }
    let lowered = instance.to_ascii_lowercase();
    if lowered.starts_with("https://") || lowered.starts_with("http://") {
        Some(instance.to_string())
    } else {
        Some(format!("https://{instance}"))
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
    fn the_tag_keywords_pick_which_releases_count() {
        assert_eq!(release_selection("latest"), (Prerelease::Exclude, None));
        assert_eq!(release_selection(""), (Prerelease::Exclude, None));
        assert_eq!(release_selection("latest-pre"), (Prerelease::Only, None));
        assert_eq!(release_selection("latest-all"), (Prerelease::Include, None));
        // A named tag is taken as it is, prerelease or not, and is matched
        // rather than globbed.
        assert_eq!(
            release_selection("v1.2.3"),
            (Prerelease::Include, Some("v1.2.3"))
        );
        assert_eq!(
            release_selection("v1.0[beta]"),
            (Prerelease::Include, Some("v1.0[beta]"))
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
    fn parses_a_gitea_feed_naming_its_instance() {
        assert_eq!(
            UpdateInfo::parse(
                "gitea-releases-zsync|git.example.com|owner|repo|latest|App*.AppImage.zsync"
            ),
            Some(UpdateInfo::Forge {
                forge: Forge::Gitea {
                    instance: "https://git.example.com".into()
                },
                repo: "owner/repo".into(),
                tag: "latest".into(),
                filename: "App*.AppImage.zsync".into(),
            })
        );

        // Forgejo is the same API under another name, and an instance may
        // spell out its scheme.
        assert_eq!(
            UpdateInfo::parse(
                "forgejo-releases-zsync|http://git.example.com/|owner|repo|latest|App*.zsync"
            ),
            Some(UpdateInfo::Forge {
                forge: Forge::Gitea {
                    instance: "http://git.example.com".into()
                },
                repo: "owner/repo".into(),
                tag: "latest".into(),
                filename: "App*.zsync".into(),
            })
        );

        // Without an instance there is nothing to ask.
        assert_eq!(
            UpdateInfo::parse("gitea-releases-zsync||owner|repo|latest|App*.zsync"),
            None
        );
        // The project alone, in the shape the other forges use, is one field short.
        assert_eq!(
            UpdateInfo::parse("gitea-releases-zsync|owner|repo|latest|App*.zsync"),
            None
        );
    }

    #[test]
    fn parses_a_codeberg_feed() {
        assert_eq!(
            UpdateInfo::parse("cb-releases-zsync|owner|repo|latest|App*.AppImage.zsync"),
            Some(UpdateInfo::Forge {
                forge: Forge::Codeberg,
                repo: "owner/repo".into(),
                tag: "latest".into(),
                filename: "App*.AppImage.zsync".into(),
            })
        );
    }

    #[test]
    fn unknown_and_malformed_forms_are_not_a_feed() {
        // A form soar does not resolve, rather than one it resolves wrongly.
        assert_eq!(UpdateInfo::parse("gt-releases-zsync|o|r|latest|f"), None);
        assert_eq!(UpdateInfo::parse("gh-releases-zsync|o|r|latest"), None);
        assert_eq!(UpdateInfo::parse("zsync|"), None);
        assert_eq!(UpdateInfo::parse(""), None);
        assert_eq!(UpdateInfo::parse("bittorrent|x"), None);
    }
}
