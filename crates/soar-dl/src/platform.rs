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
    pub fn parse(url: impl AsRef<str>) -> Option<Self> {
        let url = url.as_ref();

        if url.starts_with("ghcr.io") {
            return Some(Self::Oci {
                reference: url.to_string(),
            });
        }

        if let Some((project, tag)) = Self::parse_repo(&GITHUB_RE, url) {
            return Some(Self::Github {
                project,
                tag,
            });
        }

        if let Some((project, tag)) = Self::parse_repo(&GITLAB_RE, url) {
            if project.starts_with("api") || project.contains("/-/") {
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

fn should_fallback_status(e: &DownloadError) -> bool {
    matches!(e, DownloadError::HttpError { status, .. }
        if *status == 429 || *status == 401 || *status == 403 || *status >= 500)
}
