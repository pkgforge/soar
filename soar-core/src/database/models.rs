use rusqlite::types::Value;
use serde::{Deserialize, Serialize};

use super::packages::{PackageProvide, ProvideStrategy};

#[derive(Debug, Clone)]
pub struct Package {
    pub id: u64,
    pub repo_name: String,
    pub disabled: bool,
    pub disabled_reason: Option<Value>,
    pub pkg: String,
    pub pkg_id: String,
    pub pkg_name: String,
    pub pkg_type: String,
    pub pkg_webpage: Option<String>,
    pub app_id: Option<String>,
    pub description: String,
    pub version: String,
    pub download_url: String,
    pub size: u64,
    pub ghcr_pkg: Option<String>,
    pub ghcr_size: Option<u64>,
    pub checksum: String,
    pub homepages: Option<Vec<String>>,
    pub notes: Option<Vec<String>>,
    pub source_urls: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub categories: Option<Vec<String>>,
    pub icon: Option<String>,
    pub desktop: Option<String>,
    pub build_id: Option<String>,
    pub build_date: Option<String>,
    pub build_script: Option<String>,
    pub build_log: Option<String>,
    pub provides: Option<Vec<PackageProvide>>,
}

#[derive(Debug, Clone)]
pub struct InstalledPackage {
    pub id: u64,
    pub repo_name: String,
    pub pkg: String,
    pub pkg_id: String,
    pub pkg_name: String,
    pub version: String,
    pub size: u64,
    pub checksum: String,
    pub installed_path: String,
    pub installed_date: Option<String>,
    pub bin_path: Option<String>,
    pub icon_path: Option<String>,
    pub desktop_path: Option<String>,
    pub appstream_path: Option<String>,
    pub profile: String,
    pub pinned: bool,
    pub is_installed: bool,
    pub with_pkg_id: bool,
    pub detached: bool,
    pub unlinked: bool,
    pub provides: Option<Vec<PackageProvide>>,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct RemotePackage {
    #[serde(alias = "_disabled")]
    pub disabled: String,

    #[serde(alias = "_disabled_reason")]
    pub disabled_reason: Option<serde_json::Value>,

    pub pkg: String,
    pub pkg_id: String,
    pub pkg_name: String,
    pub pkg_type: String,
    pub pkg_webpage: Option<String>,
    pub description: String,
    pub version: String,
    pub download_url: String,
    pub size_raw: String,
    pub ghcr_pkg: Option<String>,
    pub ghcr_size_raw: Option<String>,

    #[serde(alias = "src_url")]
    pub src_urls: Vec<String>,

    #[serde(alias = "homepage")]
    pub homepages: Vec<String>,

    #[serde(alias = "license")]
    pub licenses: Option<Vec<String>>,

    #[serde(alias = "maintainer")]
    pub maintainers: Vec<String>,

    #[serde(alias = "note")]
    pub notes: Option<Vec<String>>,

    #[serde(alias = "tag")]
    pub tags: Option<Vec<String>>,

    pub bsum: String,
    pub build_id: Option<String>,
    pub build_date: Option<String>,
    pub build_script: Option<String>,
    pub build_log: Option<String>,

    #[serde(alias = "category")]
    pub categories: Vec<String>,

    pub provides: Option<Vec<String>>,
    pub icon: Option<String>,
    pub desktop: Option<String>,
    pub app_id: Option<String>,
}

impl Package {
    pub fn should_create_original_symlink(&self) -> bool {
        self.provides
            .as_ref()
            .map(|links| {
                !links
                    .iter()
                    .any(|link| matches!(link.strategy, ProvideStrategy::KeepTargetOnly))
            })
            .unwrap_or(true)
    }
}
