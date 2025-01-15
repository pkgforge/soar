use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct Package {
    pub id: u64,
    pub repo_name: String,
    pub pkg: String,
    pub pkg_id: String,
    pub pkg_name: String,
    pub app_id: Option<String>,
    pub description: String,
    pub version: String,
    pub size: u64,
    pub checksum: String,
    pub note: String,
    pub download_url: String,
    pub build_date: String,
    pub build_script: String,
    pub build_log: String,
    pub homepage: String,
    pub source_url: String,
    pub icon: Option<String>,
    pub desktop: Option<String>,
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
    pub pinned: bool,
    pub is_installed: bool,
    pub installed_with_family: bool,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct RemotePackage {
    pub pkg: String,
    pub pkg_id: String,
    pub pkg_name: String,
    pub description: String,

    #[serde(alias = "note")]
    pub notes: Option<Vec<String>>,

    pub version: String,
    pub download_url: String,
    pub size_raw: String,

    pub bsum: String,
    pub build_date: String,

    #[serde(alias = "src_url")]
    pub src_urls: Vec<String>,

    #[serde(alias = "homepage")]
    pub homepages: Vec<String>,

    #[serde(alias = "license")]
    pub licenses: Option<Vec<String>>,

    #[serde(alias = "maintainer")]
    pub maintainers: Vec<String>,

    #[serde(alias = "tag")]
    pub tags: Vec<String>,

    pub build_script: String,
    pub build_log: String,

    #[serde(alias = "category")]
    pub categories: Vec<String>,

    pub provides: Vec<String>,
    pub icon: Option<String>,
    pub desktop: Option<String>,
    pub app_id: Option<String>,
}
