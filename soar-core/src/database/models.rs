use std::fmt::Display;

use rusqlite::types::Value;
use serde::{de, Deserialize, Deserializer, Serialize};

use super::packages::{PackageProvide, ProvideStrategy};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Maintainer {
    pub name: String,
    pub contact: String,
}

impl Display for Maintainer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.name, self.contact)
    }
}

#[derive(Debug, Clone)]
pub struct Package {
    pub id: u64,
    pub repo_name: String,
    pub disabled: bool,
    pub disabled_reason: Option<Value>,
    pub rank: Option<u64>,
    pub pkg: String,
    pub pkg_id: String,
    pub pkg_name: String,
    pub pkg_family: String,
    pub pkg_type: String,
    pub pkg_webpage: Option<String>,
    pub app_id: Option<String>,
    pub description: String,
    pub version: String,
    pub version_upstream: Option<String>,
    pub licenses: Option<Vec<String>>,
    pub download_url: String,
    pub size: u64,
    pub ghcr_pkg: Option<String>,
    pub ghcr_size: Option<u64>,
    pub ghcr_files: Option<Vec<String>>,
    pub ghcr_blob: Option<String>,
    pub ghcr_url: Option<String>,
    pub bsum: String,
    pub shasum: String,
    pub homepages: Option<Vec<String>>,
    pub notes: Option<Vec<String>>,
    pub source_urls: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub categories: Option<Vec<String>>,
    pub icon: Option<String>,
    pub desktop: Option<String>,
    pub appstream: Option<String>,
    pub build_id: Option<String>,
    pub build_date: Option<String>,
    pub build_action: Option<String>,
    pub build_script: Option<String>,
    pub build_log: Option<String>,
    pub provides: Option<Vec<PackageProvide>>,
    pub snapshots: Option<Vec<String>>,
    pub repology: Option<Vec<String>>,
    pub download_count: Option<u64>,
    pub download_count_month: Option<u64>,
    pub download_count_week: Option<u64>,
    pub maintainers: Vec<Maintainer>,
}

impl Package {
    pub fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        let parse_json_vec = |idx: &str| -> rusqlite::Result<Option<Vec<String>>> {
            Ok(row
                .get::<_, Option<String>>(idx)?
                .and_then(|json| serde_json::from_str(&json).ok()))
        };

        let parse_provides = |idx: &str| -> rusqlite::Result<Option<Vec<PackageProvide>>> {
            Ok(row
                .get::<_, Option<String>>(idx)?
                .and_then(|json| serde_json::from_str(&json).ok()))
        };

        let maintainers: Vec<Maintainer> = row
            .get::<_, Option<String>>("maintainers")?
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();

        let licenses = parse_json_vec("licenses")?;
        let ghcr_files = parse_json_vec("ghcr_files")?;
        let homepages = parse_json_vec("homepages")?;
        let notes = parse_json_vec("notes")?;
        let source_urls = parse_json_vec("source_urls")?;
        let tags = parse_json_vec("tags")?;
        let categories = parse_json_vec("categories")?;
        let provides = parse_provides("provides")?;
        let snapshots = parse_json_vec("snapshots")?;
        let repology = parse_json_vec("repology")?;

        Ok(Package {
            id: row.get("id")?,
            disabled: row.get("disabled")?,
            disabled_reason: row.get("disabled_reason")?,
            rank: row.get("rank")?,
            pkg: row.get("pkg")?,
            pkg_id: row.get("pkg_id")?,
            pkg_name: row.get("pkg_name")?,
            pkg_family: row.get("pkg_family")?,
            pkg_type: row.get("pkg_type")?,
            pkg_webpage: row.get("pkg_webpage")?,
            app_id: row.get("app_id")?,
            description: row.get("description")?,
            version: row.get("version")?,
            version_upstream: row.get("version_upstream")?,
            licenses,
            download_url: row.get("download_url")?,
            size: row.get("size")?,
            ghcr_pkg: row.get("ghcr_pkg")?,
            ghcr_size: row.get("ghcr_size")?,
            ghcr_files,
            ghcr_blob: row.get("ghcr_blob")?,
            ghcr_url: row.get("ghcr_url")?,
            bsum: row.get("bsum")?,
            shasum: row.get("shasum")?,
            icon: row.get("icon")?,
            desktop: row.get("desktop")?,
            appstream: row.get("appstream")?,
            homepages,
            notes,
            source_urls,
            tags,
            categories,
            build_id: row.get("build_id")?,
            build_date: row.get("build_date")?,
            build_action: row.get("build_action")?,
            build_script: row.get("build_script")?,
            build_log: row.get("build_log")?,
            provides,
            snapshots,
            repology,
            download_count: row.get("download_count")?,
            download_count_week: row.get("download_count_week")?,
            download_count_month: row.get("download_count_month")?,
            repo_name: row.get("repo_name")?,
            maintainers,
        })
    }
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

impl InstalledPackage {
    pub fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        let parse_provides = |idx: &str| -> rusqlite::Result<Option<Vec<PackageProvide>>> {
            let value: Option<String> = row.get(idx)?;
            Ok(value.and_then(|s| serde_json::from_str(&s).ok()))
        };

        let provides = parse_provides("provides")?;

        Ok(InstalledPackage {
            id: row.get("id")?,
            repo_name: row.get("repo_name")?,
            pkg: row.get("pkg")?,
            pkg_id: row.get("pkg_id")?,
            pkg_name: row.get("pkg_name")?,
            version: row.get("version")?,
            size: row.get("size")?,
            checksum: row.get("checksum")?,
            installed_path: row.get("installed_path")?,
            installed_date: row.get("installed_date")?,
            bin_path: row.get("bin_path")?,
            icon_path: row.get("icon_path")?,
            desktop_path: row.get("desktop_path")?,
            appstream_path: row.get("appstream_path")?,
            profile: row.get("profile")?,
            pinned: row.get("pinned")?,
            is_installed: row.get("is_installed")?,
            with_pkg_id: row.get("with_pkg_id")?,
            detached: row.get("detached")?,
            unlinked: row.get("unlinked")?,
            provides,
        })
    }
}

fn empty_is_none<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    Ok(s.filter(|s| !s.is_empty()))
}

fn optional_number<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    Ok(s.filter(|s| !s.is_empty())
        .and_then(|s| s.parse::<i64>().ok())
        .filter(|&n| n >= 0)
        .map(|n| n as u64))
}

fn number_from_string<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    s.parse::<i64>()
        .map_err(|_| de::Error::invalid_value(de::Unexpected::Str(&s), &"a valid number"))
        .and_then(|n| {
            if n >= 0 {
                Ok(n as u64)
            } else {
                Err(de::Error::invalid_value(
                    de::Unexpected::Signed(n),
                    &"a non-negative u64",
                ))
            }
        })
}

fn boolean_from_string<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    match s.to_lowercase().as_str() {
        "true" | "yes" | "1" => Ok(true),
        "false" | "no" | "0" => Ok(false),
        _ => Err(de::Error::invalid_value(
            de::Unexpected::Str(&s),
            &"a valid boolean (true/false, yes/no, 1/0)",
        )),
    }
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct RemotePackage {
    #[serde(deserialize_with = "boolean_from_string", alias = "_disabled")]
    pub disabled: bool,

    #[serde(alias = "_disabled_reason")]
    pub disabled_reason: Option<serde_json::Value>,

    #[serde(default, deserialize_with = "optional_number")]
    pub rank: Option<u64>,

    pub pkg: String,
    pub pkg_id: String,
    pub pkg_name: String,
    pub pkg_family: String,
    pub pkg_type: String,

    #[serde(default, deserialize_with = "empty_is_none")]
    pub pkg_webpage: Option<String>,

    pub description: String,
    pub version: String,
    pub version_upstream: Option<String>,
    pub download_url: String,

    #[serde(default, deserialize_with = "number_from_string")]
    pub size_raw: u64,

    #[serde(default, deserialize_with = "empty_is_none")]
    pub ghcr_pkg: Option<String>,

    #[serde(default, deserialize_with = "optional_number")]
    pub ghcr_size_raw: Option<u64>,

    pub ghcr_files: Option<Vec<String>>,
    pub ghcr_blob: Option<String>,
    pub ghcr_url: Option<String>,

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
    pub shasum: String,

    #[serde(default, deserialize_with = "empty_is_none")]
    pub build_id: Option<String>,

    #[serde(default, deserialize_with = "empty_is_none")]
    pub build_date: Option<String>,

    #[serde(default, deserialize_with = "empty_is_none", alias = "build_gha")]
    pub build_action: Option<String>,

    #[serde(default, deserialize_with = "empty_is_none")]
    pub build_script: Option<String>,

    #[serde(default, deserialize_with = "empty_is_none")]
    pub build_log: Option<String>,

    #[serde(alias = "category")]
    pub categories: Vec<String>,

    pub provides: Option<Vec<String>>,

    #[serde(default, deserialize_with = "empty_is_none")]
    pub icon: Option<String>,

    #[serde(default, deserialize_with = "empty_is_none")]
    pub desktop: Option<String>,

    #[serde(default, deserialize_with = "empty_is_none")]
    pub appstream: Option<String>,

    #[serde(default, deserialize_with = "empty_is_none")]
    pub app_id: Option<String>,

    #[serde(default, deserialize_with = "optional_number")]
    pub download_count: Option<u64>,

    #[serde(default, deserialize_with = "optional_number")]
    pub download_count_month: Option<u64>,

    #[serde(default, deserialize_with = "optional_number")]
    pub download_count_week: Option<u64>,

    pub repology: Option<Vec<String>>,
    pub snapshots: Option<Vec<String>>,
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
