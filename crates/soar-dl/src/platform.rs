use std::{env, sync::LazyLock};

use percent_encoding::percent_decode_str;
use regex::Regex;
use ureq::http::header::AUTHORIZATION;
use url::Url;

use crate::{error::DownloadError, http_client::SHARED_AGENT};

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

impl PlatformUrl {
    /// Classifies an input string as a platform URL and returns the corresponding `PlatformUrl` variant.
    ///
    /// This inspects the input URL (or reference) and returns:
    /// - `Oci` when the normalized string starts with `ghcr.io/` (treated as an OCI reference).
    /// - `Github` when it matches the GitHub repository pattern, extracting project and optional tag.
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

        if let Some((project, tag)) = Self::parse_repo(&GITHUB_RE, url) {
            return Some(Self::Github {
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
        let tag = caps
            .get(2)
            .map(|m| m.as_str().trim_matches(&['\'', '"', ' '][..]))
            .filter(|s| !s.is_empty())
            .and_then(|s| {
                percent_decode_str(s)
                    .decode_utf8()
                    .ok()
                    .map(|cow| cow.into_owned())
            });

        Some((project, tag))
    }
}

/// Fetches JSON from an API base URL with an optional Bearer token and returns the deserialized
/// items as a `Vec<T>`.
///
/// If the environment variable named by `token_env[0]` (or `token_env[1]` as fallback) is set,
/// it is included as an `Authorization: Bearer <token>` header. The response body must be either
/// a JSON array (mapped to `Vec<T>`) or a single JSON object (mapped to a one-element `Vec<T>`);
/// other shapes produce `DownloadError::InvalidResponse`. Non-success HTTP statuses produce
/// `DownloadError::HttpError`.
pub fn fetch_releases_json<T>(
    path: &str,
    base: &str,
    token_env: [&str; 2],
) -> Result<Vec<T>, DownloadError>
where
    T: serde::de::DeserializeOwned,
{
    let url = format!("{}{}", base, path);
    let mut req = SHARED_AGENT.get(&url);

    if let Ok(token) = env::var(token_env[0]).or_else(|_| env::var(token_env[1])) {
        req = req.header(AUTHORIZATION, &format!("Bearer {}", token.trim()));
    }

    let mut resp = req.call()?;
    let status = resp.status();

    if !status.is_success() {
        return Err(DownloadError::HttpError {
            status: status.as_u16(),
            url: url.clone(),
        });
    }

    let json: serde_json::Value = resp
        .body_mut()
        .read_json()
        .map_err(|_| DownloadError::InvalidResponse)?;

    match json {
        serde_json::Value::Array(_) => {
            serde_json::from_value(json).map_err(|_| DownloadError::InvalidResponse)
        }
        serde_json::Value::Object(_) => {
            let single: T =
                serde_json::from_value(json).map_err(|_| DownloadError::InvalidResponse)?;
            Ok(vec![single])
        }
        _ => Err(DownloadError::InvalidResponse),
    }
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
