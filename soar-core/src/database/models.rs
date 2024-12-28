use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct Package {
    pub id: u64,
    pub repo_name: String,
    pub collection: String,
    pub pkg: String,
    pub pkg_id: String,
    pub pkg_name: String,
    pub app_id: Option<String>,
    pub family: String,
    pub description: String,
    pub version: String,
    pub size: String,
    pub checksum: String,
    pub note: String,
    pub download_url: String,
    pub build_date: String,
    pub build_script: String,
    pub build_log: String,
    pub homepage: String,
    pub category: String,
    pub source_url: String,
    pub icon: Option<String>,
    pub desktop: Option<String>,
}

#[derive(Debug, Clone)]
pub struct InstalledPackage {
    pub id: u64,
    pub repo_name: String,
    pub collection: String,
    pub family: String,
    pub pkg_name: String,
    pub pkg: String,
    pub pkg_id: Option<String>,
    pub app_id: Option<String>,
    pub description: String,
    pub version: String,
    pub size: String,
    pub checksum: String,
    pub build_date: String,
    pub build_script: String,
    pub build_log: String,
    pub category: String,
    pub bin_path: Option<String>,
    pub installed_path: String,
    pub installed_date: Option<String>,
    pub disabled: bool,
    pub pinned: bool,
    pub is_installed: bool,
    pub installed_with_family: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RemotePackageMetadata {
    #[serde(flatten)]
    pub collection: HashMap<String, Vec<RemotePackage>>,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct RemotePackage {
    pub pkg: String,
    pub pkg_name: String,
    pub description: String,
    pub note: String,
    pub version: String,
    pub download_url: String,
    pub size: String,
    pub bsum: String,
    pub build_date: String,
    pub src_url: String,
    pub homepage: String,
    pub build_script: String,
    pub build_log: String,
    pub category: String,
    pub provides: String,
    pub icon: String,
    pub desktop: Option<String>,
    pub pkg_id: Option<String>,
    pub pkg_family: Option<String>,
    pub app_id: Option<String>,
}
