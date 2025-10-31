use std::{env, sync::LazyLock};

use regex::Regex;
use soar_utils::string::decode_uri;
use ureq::http::header::AUTHORIZATION;
use url::Url;

use crate::{error::DownloadError, http_client::SHARED_AGENT};

pub enum ApiKind {
    Pkgforge,
    Primary,
}

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
    Regex::new(
        r"^(?i)(?:https?://)?(?:github(?:\.com)?[:/])([^/@]+/[^/@]+)(?:@([^/\s]+(?:/[^/\s]*)*)?)?$",
    )
    .expect("unable to compile github release regex")
});
static GITLAB_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?i)(?:https?://)?(?:gitlab(?:\.com)?[:/])((?:\d+)|(?:[^/@]+(?:/[^/@]+)*))(?:@([^/\s]+(?:/[^/\s]*)*)?)?$")
        .expect("unable to compile gitlab release regex")
});

impl PlatformUrl {
    /// Classifies an input string as a platform URL and returns the corresponding `PlatformUrl` variant.
    ///
    /// This inspects the input URL (or reference) and returns:
    /// - `Oci` when the normalized string starts with `ghcr.io/` (treated as an OCI reference),
    /// - `Github` when it matches the GitHub repository pattern, extracting project and optional tag,
    /// - `Gitlab` when it matches the GitLab repository pattern, extracting project and optional tag
    ///   (except when the project looks like an API path or contains `/-/`, which is treated as `Direct`),
    /// - `Direct` when the input parses as a valid URL with a scheme and host.
    /// Returns `None` if the input cannot be classified or parsed as a valid URL.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::platform::PlatformUrl;
    ///
    /// // OCI reference
    /// let p = PlatformUrl::parse("ghcr.io/myorg/myimage:latest").unwrap();
    /// assert!(matches!(p, PlatformUrl::Oci { .. }));
    ///
    /// // GitHub repo
    /// let p = PlatformUrl::parse("https://github.com/owner/repo/releases/tag/v1.0").unwrap();
    /// assert!(matches!(p, PlatformUrl::Github { .. }));
    ///
    /// // Direct URL
    /// let p = PlatformUrl::parse("https://example.com/resource").unwrap();
    /// assert!(matches!(p, PlatformUrl::Direct { .. }));
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
    ///
    /// # Examples
    ///
    /// ```
    /// use regex::Regex;
    ///
    /// let re = Regex::new(r"^/?([^@#]+)(?:[@#](.+))?$").unwrap();
    /// let url = "owner/repo@v1.2.3";
    /// let (project, tag) = super::parse_repo(&re, url).unwrap();
    /// assert_eq!(project, "owner/repo");
    /// assert_eq!(tag.as_deref(), Some("v1.2.3"));
    /// ```
    fn parse_repo(re: &Regex, url: &str) -> Option<(String, Option<String>)> {
        let caps = re.captures(url)?;
        let project = caps.get(1)?.as_str().to_string();
        let tag = caps
            .get(2)
            .map(|m| m.as_str().trim_matches(&['\'', '"', ' '][..]))
            .filter(|s| !s.is_empty())
            .map(decode_uri);

        Some((project, tag))
    }
}

/// Fetches JSON from a fallback base URL then, on retryable HTTP failures, from a primary base URL using an optional Bearer token, and returns the deserialized items as a Vec<T>.
///
/// The function first requests `fallback + path` without a token. If that request fails with a status that qualifies for retry (429, 401, 403, or any 5xx), it retries `primary + path` and, if the environment variable named by `token_env` is set, includes it as a `Authorization: Bearer <token>` header. The response body must be either a JSON array (mapped to `Vec<T>`) or a single JSON object (mapped to a one-element `Vec<T>`); other shapes produce `DownloadError::InvalidResponse`. Non-success HTTP statuses produce `DownloadError::HttpError`.
///
/// # Examples
///
/// ```no_run
/// use serde::Deserialize;
/// #[derive(Deserialize)]
/// struct Item { id: u32 }
///
/// // Attempts fallback first, then primary with token from "API_TOKEN" if needed.
/// let result: Result<Vec<Item>, _> = fetch_with_fallback("/api/items", "https://primary.example.com", "https://fallback.example.com", "API_TOKEN");
/// ```
pub fn fetch_with_fallback<T>(
    path: &str,
    primary: &str,
    fallback: &str,
    token_env: &str,
) -> Result<Vec<T>, DownloadError>
where
    T: serde::de::DeserializeOwned,
{
    let try_fetch = |base: &str, use_token: bool| -> Result<Vec<T>, DownloadError> {
        let url = format!("{}{}", base, path);
        let mut req = SHARED_AGENT.get(&url);

        if use_token {
            if let Ok(token) = env::var(token_env) {
                req = req.header(AUTHORIZATION, &format!("Bearer {}", token));
            }
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
    };

    try_fetch(fallback, false).or_else(|e| {
        if should_fallback_status(&e) {
            try_fetch(primary, true)
        } else {
            Err(e)
        }
    })
}

/// Determines whether a download `HttpError` status should cause a fallback attempt.
///
/// # Returns
///
/// `true` if the error is an HTTP status of 429, 401, 403, or any status >= 500; `false` otherwise.
fn should_fallback_status(e: &DownloadError) -> bool {
    matches!(e, DownloadError::HttpError { status, .. }
        if *status == 429 || *status == 401 || *status == 403 || *status >= 500)
}