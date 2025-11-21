use std::path::PathBuf;

use documented::{Documented, DocumentedFields};
use serde::{Deserialize, Serialize};
use soar_utils::time::parse_duration;

use crate::{config::get_config, error::ConfigError};

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
    pub fn get_path(&self) -> Result<PathBuf, ConfigError> {
        Ok(get_config().get_repositories_path()?.join(&self.name))
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.unwrap_or(true)
    }

    pub fn signature_verification(&self) -> bool {
        if let Some(global_override) = get_config().signature_verification {
            return global_override;
        }
        if self.pubkey.is_none() {
            return false;
        };
        self.signature_verification.unwrap_or(true)
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
            value => parse_duration(value).unwrap_or(3_600_000),
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
        DefaultRepositoryInfo {
            name: "pkgforge-cargo",
            url_template: "https://meta.pkgforge.dev/external/pkgforge-cargo/{}.sdb.zstd",
            desktop_integration: Some(false),
            platforms: vec![
                "aarch64-Linux",
                "loongarch64-Linux",
                "riscv64-Linux",
                "x86_64-Linux",
            ],
            is_core: true,
            ..DefaultRepositoryInfo::default()
        },
        DefaultRepositoryInfo {
            name: "pkgforge-go",
            url_template: "https://meta.pkgforge.dev/external/pkgforge-go/{}.sdb.zstd",
            desktop_integration: Some(false),
            platforms: vec![
                "aarch64-Linux",
                "loongarch64-Linux",
                "riscv64-Linux",
                "x86_64-Linux",
            ],
            is_core: true,
            ..DefaultRepositoryInfo::default()
        },
        DefaultRepositoryInfo {
            name: "ivan-hc-am",
            url_template: "https://meta.pkgforge.dev/external/am/{}.sdb.zstd",
            desktop_integration: Some(true),
            platforms: vec!["x86_64-Linux"],
            is_core: false,
            ..DefaultRepositoryInfo::default()
        },
        DefaultRepositoryInfo {
            name: "appimage-github-io",
            url_template: "https://meta.pkgforge.dev/external/appimage.github.io/{}.sdb.zstd",
            desktop_integration: Some(true),
            platforms: vec!["aarch64-Linux", "x86_64-Linux"],
            is_core: false,
            ..DefaultRepositoryInfo::default()
        },
    ]
}
