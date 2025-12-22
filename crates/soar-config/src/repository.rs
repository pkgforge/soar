use std::path::PathBuf;

use documented::{Documented, DocumentedFields};
use serde::{Deserialize, Serialize};
use soar_utils::time::parse_duration;

use crate::{config::get_config, error::Result};

/// Defines a remote repository that provides packages.
#[derive(Clone, Deserialize, Serialize, Documented, DocumentedFields)]
pub struct Repository {
    /// Unique name of the repository.
    pub name: String,

    /// URL to the repository's metadata file.
    pub url: String,

    /// Enables desktop integration for packages from this repository.
    /// Default: false
    pub desktop_integration: Option<bool>,

    /// URL to the repository's public key (for signature verification).
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
        Ok(get_config().get_repositories_path()?.join(&self.name))
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.unwrap_or(true)
    }

    pub fn signature_verification(&self) -> bool {
        let config = get_config();

        match config.signature_verification {
            Some(false) => false,
            _ if self.pubkey.is_none() => false,
            Some(true) => true,
            _ => self.signature_verification.unwrap_or(true),
        }
    }

    pub fn sync_interval(&self) -> u128 {
        match get_config()
            .sync_interval
            .clone()
            .or(self.sync_interval.clone())
            .as_deref()
            .unwrap_or("3h")
        {
            "always" => 0,
            "never" => u128::MAX,
            "auto" => 3 * 3_600_000,
            value => parse_duration(value).unwrap_or(3 * 3_600_000),
        }
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
    pub is_core: bool,
}

pub fn get_platform_repositories() -> Vec<DefaultRepositoryInfo> {
    vec![
        DefaultRepositoryInfo {
            name: "bincache",
            url_template: "https://meta.pkgforge.dev/bincache/{}.sdb.zstd",
            pubkey: Some("https://meta.pkgforge.dev/bincache/minisign.pub"),
            desktop_integration: Some(false),
            enabled: Some(true),
            signature_verification: Some(true),
            sync_interval: Some("3h"),
            platforms: vec!["aarch64-Linux", "riscv64-Linux", "x86_64-Linux"],
            is_core: true,
        },
        DefaultRepositoryInfo {
            name: "pkgcache",
            url_template: "https://meta.pkgforge.dev/pkgcache/{}.sdb.zstd",
            pubkey: Some("https://meta.pkgforge.dev/pkgcache/minisign.pub"),
            desktop_integration: Some(true),
            platforms: vec!["aarch64-Linux", "riscv64-Linux", "x86_64-Linux"],
            is_core: true,
            ..DefaultRepositoryInfo::default()
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(repos.iter().any(|r| r.name == "bincache"));
        assert!(repos.iter().any(|r| r.name == "pkgcache"));
        assert!(repos.iter().any(|r| r.is_core));
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
