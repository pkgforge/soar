//! Release fetching from git forges.

use std::{
    collections::HashMap,
    fmt,
    sync::{LazyLock, RwLock},
};

use releasekit::{
    client::{HeaderMap, HttpClient, Response},
    platform::{GitHub, GitLab, Gitea},
    Forge as _, Release,
};

use crate::{
    error::{describe_request_error, DownloadError},
    http_client::SHARED_AGENT,
};

const CODEBERG_URL: &str = "https://codeberg.org";

/// Release notes are prose, and a long history of them outgrows the default
/// body limit.
const MAX_RESPONSE_SIZE: u64 = 32 * 1024 * 1024;

/// The environment variable holding the token for each Gitea or Forgejo host.
static INSTANCE_TOKEN_VARS: LazyLock<RwLock<HashMap<String, String>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Declares which environment variable holds the token for each Gitea or
/// Forgejo host, keyed by host and, where the instance uses one, port.
///
/// Nothing else is ever sent a token. GitHub, GitLab and Codeberg each run on
/// one known host, so their variables can be read directly, but a Gitea or
/// Forgejo instance is whatever host names itself one: a URL soar was handed
/// rather than asked for could otherwise collect the credential meant for
/// somewhere else.
pub fn set_instance_token_vars(vars: HashMap<String, String>) {
    let mut tokens = INSTANCE_TOKEN_VARS.write().unwrap();
    *tokens = vars
        .into_iter()
        .map(|(host, var)| (host.trim().to_ascii_lowercase(), var))
        .collect();
}

/// The variable holding `instance`'s token, where one was declared.
///
/// An `http://` instance has none: a token is not worth sending in the clear.
fn instance_token_var(instance: &str) -> Option<String> {
    let authority = instance.strip_prefix("https://")?;
    let authority = authority
        .split_once('/')
        .map_or(authority, |(authority, _)| authority)
        .to_ascii_lowercase();

    INSTANCE_TOKEN_VARS.read().unwrap().get(&authority).cloned()
}

/// A git forge soar can fetch releases from.
///
/// Gitea and Forgejo are the same API, so one variant covers both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Forge {
    /// github.com.
    GitHub,
    /// gitlab.com.
    GitLab,
    /// codeberg.org.
    Codeberg,
    /// A Gitea or Forgejo instance, named by its base URL.
    Gitea {
        /// Base URL of the instance, such as `https://git.example.com`.
        instance: String,
    },
}

impl Forge {
    /// Fetches the releases of `project`, or the single release `tag` names.
    ///
    /// `project` is `owner/repo`, or a numeric project id on GitLab.
    pub fn fetch_releases(
        &self,
        project: &str,
        tag: Option<&str>,
    ) -> Result<Vec<Release>, DownloadError> {
        let releases = match self {
            Self::GitHub => {
                GitHub::new(SoarClient)
                    .with_token_from_env(&["GITHUB_TOKEN", "GH_TOKEN"])
                    .fetch_releases(project, tag)
            }
            Self::GitLab => {
                GitLab::new(SoarClient)
                    .with_token_from_env(&["GITLAB_TOKEN", "GL_TOKEN"])
                    .fetch_releases(project, tag)
            }
            Self::Codeberg => {
                Gitea::new(SoarClient, CODEBERG_URL)
                    .with_token_from_env(&["CODEBERG_TOKEN"])
                    .fetch_releases(project, tag)
            }
            Self::Gitea {
                instance,
            } => {
                let mut gitea = Gitea::new(SoarClient, instance);
                if let Some(ref var) = instance_token_var(instance) {
                    gitea = gitea.with_token_from_env(&[var]);
                }
                gitea.fetch_releases(project, tag)
            }
        };

        releases.map_err(Into::into)
    }
}

impl fmt::Display for Forge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GitHub => f.write_str("GitHub"),
            Self::GitLab => f.write_str("GitLab"),
            Self::Codeberg => f.write_str("Codeberg"),
            Self::Gitea {
                instance,
            } => write!(f, "{instance}"),
        }
    }
}

/// The HTTP backend forge requests go out through, so they carry whatever
/// proxy, user agent and timeout soar is configured with.
#[derive(Clone)]
struct SoarClient;

impl HttpClient for SoarClient {
    fn get(&self, url: &str, headers: &HeaderMap) -> releasekit::error::Result<Response> {
        let mut req = SHARED_AGENT.get(url);
        for (key, value) in headers.iter() {
            req = req.header(key, value);
        }

        let mut resp = req.call().map_err(|err| {
            match err {
                ureq::Error::StatusCode(status) => {
                    releasekit::Error::Http {
                        status,
                        url: url.to_string(),
                    }
                }
                other => releasekit::Error::Network(describe_request_error(&other, url)),
            }
        })?;

        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            return Err(releasekit::Error::Http {
                status,
                url: url.to_string(),
            });
        }

        let body = resp
            .body_mut()
            .with_config()
            .limit(MAX_RESPONSE_SIZE)
            .read_to_string()
            .map_err(|err| releasekit::Error::Network(err.to_string()))?;

        Ok(Response {
            status,
            body,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_declared_https_instance_is_sent_a_token() {
        set_instance_token_vars(HashMap::from([
            ("git.example.com".to_string(), "EXAMPLE_TOKEN".to_string()),
            (
                "git.example.com:3000".to_string(),
                "PORTED_TOKEN".to_string(),
            ),
        ]));

        assert_eq!(
            instance_token_var("https://git.example.com").as_deref(),
            Some("EXAMPLE_TOKEN")
        );
        assert_eq!(
            instance_token_var("https://GIT.example.com/gitea").as_deref(),
            Some("EXAMPLE_TOKEN")
        );
        assert_eq!(
            instance_token_var("https://git.example.com:3000").as_deref(),
            Some("PORTED_TOKEN")
        );
        // A host nobody declared, which is what a download URL names.
        assert_eq!(instance_token_var("https://evil.test"), None);
        // The same host, but in the clear.
        assert_eq!(instance_token_var("http://git.example.com"), None);
        // A port turns it into a different host.
        assert_eq!(instance_token_var("https://git.example.com:8443"), None);

        set_instance_token_vars(HashMap::new());
        assert_eq!(instance_token_var("https://git.example.com"), None);
    }

    #[test]
    fn a_forge_names_itself_by_what_the_user_wrote() {
        assert_eq!(Forge::GitHub.to_string(), "GitHub");
        assert_eq!(Forge::Codeberg.to_string(), "Codeberg");
        assert_eq!(
            Forge::Gitea {
                instance: "https://git.example.com".into()
            }
            .to_string(),
            "https://git.example.com"
        );
    }
}
