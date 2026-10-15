use std::path::PathBuf;

use documented::{Documented, DocumentedFields};
use serde::{Deserialize, Serialize};
use soar_utils::{path::is_safe_component, time::parse_duration};

use crate::{
    config::get_config,
    error::{ConfigError, Result},
};

const DEFAULT_SYNC_INTERVAL: u128 = 3 * 3_600_000;

/// The milliseconds a `sync_interval` value stands for, or `None` when it is
/// not one.
pub fn parse_sync_interval(value: &str) -> Option<u128> {
    match value {
        "always" => Some(0),
        "never" => Some(u128::MAX),
        "auto" => Some(DEFAULT_SYNC_INTERVAL),
        value => parse_duration(value),
    }
}

/// Whether a repository must carry a pubkey: verification was asked for, by
/// the repository or else globally, and not turned off for it.
pub fn requires_pubkey(global: Option<bool>, repo: Option<bool>) -> bool {
    global != Some(false) && repo.unwrap_or(global == Some(true))
}

/// Defines a remote repository that provides packages.
#[derive(Clone, Deserialize, Serialize, Documented, DocumentedFields)]
pub struct Repository {
    /// Unique name of the repository.
    pub name: String,

    /// URL to the repository's metadata file.
    pub url: String,

    /// Enables desktop integration for packages from this repository.
    /// Default: true
    pub desktop_integration: Option<bool>,

    /// Base64-encoded public key for signature verification (minisign format).
    pub pubkey: Option<String>,

    /// Whether the repository is enabled.
    /// Default: true
    pub enabled: Option<bool>,

    /// Enables signature verification for this repository.
    /// Default is derived based on the existence of `pubkey`
    pub signature_verification: Option<bool>,

    /// Optional sync interval (e.g., "1h", "12h", "1d").
    /// Default: "3h"
    pub sync_interval: Option<String>,
}

impl Repository {
    pub fn get_path(&self) -> Result<PathBuf> {
        if !is_safe_component(&self.name) {
            return Err(ConfigError::InvalidRepositoryName(self.name.clone()));
        }
        Ok(get_config().get_repositories_path()?.join(&self.name))
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.unwrap_or(true)
    }

    /// Whether this repository's metadata is checked against its pubkey.
    ///
    /// A global `false` turns checking off everywhere. Otherwise the
    /// repository's own setting wins over the global one, and a repository
    /// without a pubkey has nothing to check against.
    pub fn signature_verification(&self) -> bool {
        match (
            get_config().signature_verification,
            self.signature_verification,
        ) {
            (Some(false), _) => false,
            _ if self.pubkey.is_none() => false,
            (_, Some(repo)) => repo,
            _ => true,
        }
    }

    /// The interval between syncs, in milliseconds. Values are checked when
    /// the configuration loads, so one that fails to parse here falls back to
    /// the default.
    pub fn sync_interval(&self) -> u128 {
        get_config()
            .sync_interval
            .clone()
            .or(self.sync_interval.clone())
            .as_deref()
            .and_then(parse_sync_interval)
            .unwrap_or(DEFAULT_SYNC_INTERVAL)
    }
}

#[derive(Default)]
pub struct DefaultRepositoryInfo {
    pub name: &'static str,
    pub url_template: &'static str,
    pub pubkey: Option<&'static str>,
    pub desktop_integration: Option<bool>,
    pub enabled: Option<bool>,
    pub signature_verification: Option<bool>,
    pub sync_interval: Option<&'static str>,
    pub platforms: Vec<&'static str>,
}

/// Base64-encoded minisign public key for the soarpkgs repository.
pub const SOARPKGS_PUBKEY: &str = "RWQ109gKujRqohsA7RERlXFfeJi23EcHN3Dz8TxyPAywa5mLw/fbcbU4";

pub fn get_platform_repositories() -> Vec<DefaultRepositoryInfo> {
    vec![DefaultRepositoryInfo {
        name: "soarpkgs",
        url_template: "https://github.com/pkgforge/soarpkgs/releases/latest/download/metadata-{}.sdb.zstd",
        pubkey: Some(SOARPKGS_PUBKEY),
        desktop_integration: Some(true),
        platforms: vec!["aarch64-linux", "riscv64-linux", "x86_64-linux"],
        ..DefaultRepositoryInfo::default()
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_repository_opt_out_needs_no_pubkey() {
        assert!(!requires_pubkey(Some(true), Some(false)));
        assert!(requires_pubkey(Some(true), None));
        assert!(requires_pubkey(None, Some(true)));
        assert!(!requires_pubkey(None, None));
        assert!(!requires_pubkey(Some(false), Some(true)));
    }

    #[test]
    fn get_path_rejects_an_unsafe_name() {
        for name in ["..", "a/b", ""] {
            let repo = Repository {
                name: name.to_string(),
                url: "https://example.com".to_string(),
                desktop_integration: None,
                pubkey: None,
                enabled: Some(true),
                signature_verification: None,
                sync_interval: None,
            };
            assert!(
                matches!(repo.get_path(), Err(ConfigError::InvalidRepositoryName(_))),
                "{name:?}"
            );
        }
    }

    #[test]
    fn test_repository_is_enabled() {
        let repo = Repository {
            name: "test".to_string(),
            url: "https://example.com".to_string(),
            desktop_integration: None,
            pubkey: None,
            enabled: Some(true),
            signature_verification: None,
            sync_interval: None,
        };

        assert!(repo.is_enabled());

        let disabled = Repository {
            enabled: Some(false),
            ..repo.clone()
        };
        assert!(!disabled.is_enabled());

        let default = Repository {
            enabled: None,
            ..repo
        };
        assert!(default.is_enabled());
    }

    #[test]
    fn test_repository_sync_interval() {
        let repo = Repository {
            name: "test".to_string(),
            url: "https://example.com".to_string(),
            desktop_integration: None,
            pubkey: None,
            enabled: Some(true),
            signature_verification: None,
            sync_interval: Some("always".to_string()),
        };

        assert_eq!(repo.sync_interval(), 0);
    }

    #[test]
    fn test_get_platform_repositories() {
        let repos = get_platform_repositories();

        assert!(!repos.is_empty());
        assert!(repos.iter().any(|r| r.name == "soarpkgs"));
    }

    #[test]
    fn test_repository_info_platforms() {
        let repos = get_platform_repositories();

        for repo in repos {
            assert!(!repo.platforms.is_empty());
            assert!(!repo.name.is_empty());
            assert!(!repo.url_template.is_empty());
        }
    }
}
