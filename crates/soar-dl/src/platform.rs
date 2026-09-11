use std::sync::LazyLock;

use percent_encoding::percent_decode_str;
use regex::Regex;
use url::Url;

#[derive(Debug)]
pub enum PlatformUrl {
    Github {
        project: String,
        tag: Option<String>,
    },
    Gitlab {
        project: String,
        tag: Option<String>,
    },
    Codeberg {
        project: String,
        tag: Option<String>,
    },
    Gitea {
        instance: String,
        project: String,
        tag: Option<String>,
    },
    Oci {
        reference: String,
    },
    Direct {
        url: String,
    },
}

static GITHUB_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?i)(?:https?://)?(?:github(?:\.com)?[:/])([^/@]+/[^/@]+)(?:@([^\r\n]+))?$")
        .expect("unable to compile github release regex")
});

static GITLAB_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(?i)(?:https?://)?(?:gitlab(?:\.com)?[:/])((?:\d+)|(?:[^/@]+(?:/[^/@]+)*))(?:@([^\r\n]+))?$",
    )
    .expect("unable to compile gitlab release regex")
});

static CODEBERG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?i)(?:https?://)?(?:codeberg(?:\.org)?[:/])([^/@]+/[^/@]+)(?:@([^\r\n]+))?$")
        .expect("unable to compile codeberg release regex")
});

impl PlatformUrl {
    /// Classifies an input string as a platform URL and returns the corresponding `PlatformUrl` variant.
    ///
    /// This inspects the input URL (or reference) and returns:
    /// - `Oci` when the normalized string starts with `ghcr.io/` (treated as an OCI reference).
    /// - `Github` when it matches the GitHub repository pattern, extracting project and optional tag.
    /// - `Codeberg` when it matches the Codeberg repository pattern.
    /// - `Gitea` when it is prefixed `gitea:` or `forgejo:`, which is what names an
    ///   instance soar cannot recognize from its host alone.
    /// - `Gitlab` when it matches the GitLab repository pattern, extracting project and optional tag
    ///   (except when the project looks like an API path or contains `/-/`, which is treated as `Direct`).
    /// - `Direct` when the input parses as a valid URL with a scheme and host.
    ///
    /// Returns `None` if the input cannot be classified or parsed as a valid URL.
    ///
    /// # Examples
    ///
    /// ```
    /// use soar_dl::platform::PlatformUrl;
    ///
    /// // OCI reference
    /// let _ = PlatformUrl::parse("ghcr.io/myorg/myimage:latest").unwrap();
    ///
    /// // GitHub repo
    /// let _ = PlatformUrl::parse("https://github.com/owner/repo/releases/tag/v1.0").unwrap();
    ///
    /// // Direct URL
    /// let _ = PlatformUrl::parse("https://example.com/resource").unwrap();
    /// ```
    pub fn parse(url: impl AsRef<str>) -> Option<Self> {
        let url = url.as_ref();

        let normalized = url
            .trim_start_matches("https://")
            .trim_start_matches("http://");
        if normalized.starts_with("ghcr.io/") {
            return Some(Self::Oci {
                reference: normalized.to_string(),
            });
        }

        if let Some((instance, project, tag)) = parse_gitea_target(url, true) {
            return Some(Self::Gitea {
                instance,
                project,
                tag,
            });
        }

        if let Some((project, tag)) = Self::parse_repo(&GITHUB_RE, url) {
            return Some(Self::Github {
                project,
                tag,
            });
        }

        if let Some((project, tag)) = Self::parse_repo(&CODEBERG_RE, url) {
            return Some(Self::Codeberg {
                project,
                tag,
            });
        }

        if let Some((project, tag)) = Self::parse_repo(&GITLAB_RE, url) {
            if project.starts_with("api/") || project.contains("/-/") {
                return Url::parse(url).ok().map(|_| {
                    Self::Direct {
                        url: url.to_string(),
                    }
                });
            }
            return Some(Self::Gitlab {
                project,
                tag,
            });
        }

        Url::parse(url)
            .ok()
            .filter(|u| !u.scheme().is_empty() && u.host().is_some())
            .map(|_| {
                Self::Direct {
                    url: url.to_string(),
                }
            })
    }

    /// Extracts a repository project path and an optional tag from `url` using `re`.
    ///
    /// The returned `project` is the first capture group as a `String`. The optional `tag`
    /// is taken from the second capture group (if present), with surrounding quotes and
    /// spaces removed and URI-decoded. Returns `None` if the regex does not match.
    fn parse_repo(re: &Regex, url: &str) -> Option<(String, Option<String>)> {
        let caps = re.captures(url)?;
        let project = caps.get(1)?.as_str().to_string();
        let tag = caps.get(2).and_then(|m| clean_tag(m.as_str()));

        Some((project, tag))
    }
}

/// A tag as the publisher spells it, out of how a URL had to carry it.
fn clean_tag(raw: &str) -> Option<String> {
    let trimmed = raw.trim_matches(&['\'', '"', ' '][..]);
    if trimmed.is_empty() {
        return None;
    }
    percent_decode_str(trimmed)
        .decode_utf8()
        .ok()
        .map(|cow| cow.into_owned())
}

/// A Gitea or Forgejo target as its instance, the project on it and an
/// optional tag.
///
/// A bare instance URL looks exactly like a direct download link, so
/// `require_prefix` asks for the explicit `gitea:` or `forgejo:` marker. A
/// value passed to `--gitea` already says which forge it is and does not need
/// one.
///
/// The last two path segments name the project, so an instance served under a
/// path prefix parses the same as one served at the root.
///
/// # Examples
///
/// ```
/// use soar_dl::platform::parse_gitea_target;
///
/// let (instance, project, tag) =
///     parse_gitea_target("gitea:git.example.com/owner/repo@v1.0", true).unwrap();
/// assert_eq!(instance, "https://git.example.com");
/// assert_eq!(project, "owner/repo");
/// assert_eq!(tag.as_deref(), Some("v1.0"));
/// ```
pub fn parse_gitea_target(
    input: &str,
    require_prefix: bool,
) -> Option<(String, String, Option<String>)> {
    let target = input.trim();
    let stripped =
        strip_prefix_ci(target, "gitea:").or_else(|| strip_prefix_ci(target, "forgejo:"));
    if require_prefix && stripped.is_none() {
        return None;
    }
    let target = stripped.unwrap_or(target);

    let (scheme, rest) = match strip_prefix_ci(target, "https://") {
        Some(rest) => ("https://", rest),
        None => {
            match strip_prefix_ci(target, "http://") {
                Some(rest) => ("http://", rest),
                None => ("https://", target),
            }
        }
    };

    let last_segment = rest.rfind('/').map(|idx| idx + 1).unwrap_or(0);
    let (rest, tag) = match rest[last_segment..].split_once('@') {
        Some((repo, tag)) => (&rest[..last_segment + repo.len()], clean_tag(tag)),
        None => (rest, None),
    };

    let (head, repo) = rest.rsplit_once('/')?;
    let (base, owner) = head.rsplit_once('/')?;
    if base.is_empty() || owner.is_empty() || repo.is_empty() || base.contains('@') {
        return None;
    }

    Some((format!("{scheme}{base}"), format!("{owner}/{repo}"), tag))
}

fn strip_prefix_ci<'a>(input: &'a str, prefix: &str) -> Option<&'a str> {
    input
        .get(..prefix.len())
        .filter(|head| head.eq_ignore_ascii_case(prefix))
        .map(|head| &input[head.len()..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_url_parse_oci() {
        let result = PlatformUrl::parse("ghcr.io/owner/repo:latest");
        match result {
            Some(PlatformUrl::Oci {
                reference,
            }) => {
                assert_eq!(reference, "ghcr.io/owner/repo:latest");
            }
            _ => panic!("Expected OCI variant"),
        }
    }

    #[test]
    fn test_platform_url_parse_oci_with_prefix() {
        let result = PlatformUrl::parse("https://ghcr.io/owner/repo:v1.0");
        match result {
            Some(PlatformUrl::Oci {
                reference,
            }) => {
                assert_eq!(reference, "ghcr.io/owner/repo:v1.0");
            }
            _ => panic!("Expected OCI variant"),
        }
    }

    #[test]
    fn test_platform_url_parse_github_https() {
        let result = PlatformUrl::parse("https://github.com/owner/repo");
        match result {
            Some(PlatformUrl::Github {
                project,
                tag,
            }) => {
                assert_eq!(project, "owner/repo");
                assert_eq!(tag, None);
            }
            _ => panic!("Expected Github variant"),
        }
    }

    #[test]
    fn test_platform_url_parse_github_with_tag() {
        let result = PlatformUrl::parse("https://github.com/owner/repo@v1.0.0");
        match result {
            Some(PlatformUrl::Github {
                project,
                tag,
            }) => {
                assert_eq!(project, "owner/repo");
                assert_eq!(tag, Some("v1.0.0".to_string()));
            }
            _ => panic!("Expected Github variant with tag"),
        }
    }

    #[test]
    fn test_platform_url_parse_github_shorthand() {
        let result = PlatformUrl::parse("github:owner/repo");
        match result {
            Some(PlatformUrl::Github {
                project,
                tag,
            }) => {
                assert_eq!(project, "owner/repo");
                assert_eq!(tag, None);
            }
            _ => panic!("Expected Github variant"),
        }
    }

    #[test]
    fn test_platform_url_parse_github_case_insensitive() {
        let result = PlatformUrl::parse("GITHUB.COM/owner/repo");
        match result {
            Some(PlatformUrl::Github {
                project,
                tag,
            }) => {
                assert_eq!(project, "owner/repo");
                assert_eq!(tag, None);
            }
            _ => panic!("Expected Github variant"),
        }
    }

    #[test]
    fn test_platform_url_parse_gitlab_https() {
        let result = PlatformUrl::parse("https://gitlab.com/owner/repo");
        match result {
            Some(PlatformUrl::Gitlab {
                project,
                tag,
            }) => {
                assert_eq!(project, "owner/repo");
                assert_eq!(tag, None);
            }
            _ => panic!("Expected Gitlab variant"),
        }
    }

    #[test]
    fn test_platform_url_parse_gitlab_with_tag() {
        let result = PlatformUrl::parse("https://gitlab.com/owner/repo@v2.0");
        match result {
            Some(PlatformUrl::Gitlab {
                project,
                tag,
            }) => {
                assert_eq!(project, "owner/repo");
                assert_eq!(tag, Some("v2.0".to_string()));
            }
            _ => panic!("Expected Gitlab variant with tag"),
        }
    }

    #[test]
    fn test_platform_url_parse_gitlab_numeric_project() {
        let result = PlatformUrl::parse("https://gitlab.com/12345@v1.0");
        match result {
            Some(PlatformUrl::Gitlab {
                project,
                tag,
            }) => {
                assert_eq!(project, "12345");
                assert_eq!(tag, Some("v1.0".to_string()));
            }
            _ => panic!("Expected Gitlab variant with numeric project"),
        }
    }

    #[test]
    fn test_platform_url_parse_gitlab_nested_groups() {
        let result = PlatformUrl::parse("https://gitlab.com/group/subgroup/repo");
        match result {
            Some(PlatformUrl::Gitlab {
                project,
                tag,
            }) => {
                assert_eq!(project, "group/subgroup/repo");
                assert_eq!(tag, None);
            }
            _ => panic!("Expected Gitlab variant with nested groups"),
        }
    }

    #[test]
    fn test_platform_url_parse_gitlab_api_path_as_direct() {
        let result = PlatformUrl::parse("https://gitlab.com/api/v4/projects/123");
        match result {
            Some(PlatformUrl::Direct {
                url,
            }) => {
                assert_eq!(url, "https://gitlab.com/api/v4/projects/123");
            }
            _ => panic!("Expected Direct variant for API path"),
        }
    }

    #[test]
    fn test_platform_url_parse_gitlab_special_path_as_direct() {
        let result = PlatformUrl::parse("https://gitlab.com/owner/repo/-/releases");
        match result {
            Some(PlatformUrl::Direct {
                url,
            }) => {
                assert_eq!(url, "https://gitlab.com/owner/repo/-/releases");
            }
            _ => panic!("Expected Direct variant for special path"),
        }
    }

    #[test]
    fn test_platform_url_parse_codeberg() {
        let result = PlatformUrl::parse("https://codeberg.org/owner/repo@v1.0");
        match result {
            Some(PlatformUrl::Codeberg {
                project,
                tag,
            }) => {
                assert_eq!(project, "owner/repo");
                assert_eq!(tag, Some("v1.0".to_string()));
            }
            _ => panic!("Expected Codeberg variant"),
        }

        assert!(matches!(
            PlatformUrl::parse("codeberg:owner/repo"),
            Some(PlatformUrl::Codeberg { .. })
        ));
    }

    #[test]
    fn test_platform_url_parse_gitea() {
        let result = PlatformUrl::parse("gitea:git.example.com/owner/repo@v1.0");
        match result {
            Some(PlatformUrl::Gitea {
                instance,
                project,
                tag,
            }) => {
                assert_eq!(instance, "https://git.example.com");
                assert_eq!(project, "owner/repo");
                assert_eq!(tag, Some("v1.0".to_string()));
            }
            _ => panic!("Expected Gitea variant"),
        }

        // An instance served under a path prefix, spelled out in full.
        match PlatformUrl::parse("forgejo:https://example.com/git/owner/repo") {
            Some(PlatformUrl::Gitea {
                instance,
                project,
                ..
            }) => {
                assert_eq!(instance, "https://example.com/git");
                assert_eq!(project, "owner/repo");
            }
            _ => panic!("Expected Gitea variant"),
        }
    }

    #[test]
    fn an_unmarked_host_is_a_direct_download_not_a_gitea_instance() {
        // Nothing tells a Gitea instance apart from any other host, so one is
        // only taken as such when the caller says so.
        assert!(matches!(
            PlatformUrl::parse("https://git.example.com/owner/repo"),
            Some(PlatformUrl::Direct { .. })
        ));
    }

    #[test]
    fn test_platform_url_parse_direct_url() {
        let result = PlatformUrl::parse("https://example.com/download/file.tar.gz");
        match result {
            Some(PlatformUrl::Direct {
                url,
            }) => {
                assert_eq!(url, "https://example.com/download/file.tar.gz");
            }
            _ => panic!("Expected Direct variant"),
        }
    }

    #[test]
    fn test_platform_url_parse_direct_http() {
        let result = PlatformUrl::parse("http://example.com/file.zip");
        match result {
            Some(PlatformUrl::Direct {
                url,
            }) => {
                assert_eq!(url, "http://example.com/file.zip");
            }
            _ => panic!("Expected Direct variant"),
        }
    }

    #[test]
    fn test_platform_url_parse_invalid() {
        assert!(PlatformUrl::parse("not a valid url").is_none());
        assert!(PlatformUrl::parse("").is_none());
        assert!(PlatformUrl::parse("/not/a/url").is_none());
    }

    #[test]
    fn test_platform_url_parse_github_with_spaces_in_tag() {
        let result = PlatformUrl::parse("github.com/owner/repo@v1.0 beta");
        match result {
            Some(PlatformUrl::Github {
                project,
                tag,
            }) => {
                assert_eq!(project, "owner/repo");
                assert_eq!(tag, Some("v1.0 beta".to_string()));
            }
            _ => panic!("Expected Github variant with tag containing spaces"),
        }
    }

    #[test]
    fn test_platform_url_parse_tag_with_special_chars() {
        let result = PlatformUrl::parse("github.com/owner/repo@v1.0-rc.1+build.123");
        match result {
            Some(PlatformUrl::Github {
                project,
                tag,
            }) => {
                assert_eq!(project, "owner/repo");
                assert_eq!(tag, Some("v1.0-rc.1+build.123".to_string()));
            }
            _ => panic!("Expected Github variant with complex tag"),
        }
    }

    #[test]
    fn test_parse_repo_with_quotes() {
        let result = PlatformUrl::parse("github.com/owner/repo@'v1.0'");
        match result {
            Some(PlatformUrl::Github {
                project,
                tag,
            }) => {
                assert_eq!(project, "owner/repo");
                assert_eq!(tag, Some("v1.0".to_string()));
            }
            _ => panic!("Expected quotes to be stripped from tag"),
        }
    }

    #[test]
    fn test_parse_repo_percent_encoded_tag() {
        let result = PlatformUrl::parse("github.com/owner/repo@v1.0%2Bbuild");
        match result {
            Some(PlatformUrl::Github {
                project,
                tag,
            }) => {
                assert_eq!(project, "owner/repo");
                assert_eq!(tag, Some("v1.0+build".to_string()));
            }
            _ => panic!("Expected percent-encoded tag to be decoded"),
        }
    }
}
