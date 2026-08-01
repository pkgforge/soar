use diesel::{prelude::*, sqlite::Sqlite};
use serde_json::Value;

use crate::{json_vec, models::types::PackageProvide, schema::core::*};

#[derive(Debug, Selectable)]
pub struct Package {
    pub id: i32,
    pub repo_name: String,
    pub pkg_id: Option<String>,
    pub pkg_name: String,
    pub pkg_family: Option<String>,
    pub pkg_type: Option<String>,
    pub version: String,
    pub size: i64,
    pub checksum: Option<String>,
    pub installed_path: String,
    pub installed_date: String,
    pub profile: String,
    pub pinned: bool,
    pub is_installed: bool,
    pub detached: bool,
    pub unlinked: bool,
    pub provides: Option<Vec<PackageProvide>>,
    pub install_patterns: Option<Vec<String>>,
    /// Where a URL or local install came from, so it can be checked again.
    pub download_url: Option<String>,
    /// The AppImage `.upd_info` string, which names a zsync feed.
    pub update_info: Option<String>,
}

impl Queryable<packages::SqlType, Sqlite> for Package {
    type Row = (
        i32,
        String,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        String,
        i64,
        Option<String>,
        String,
        String,
        String,
        bool,
        bool,
        bool,
        bool,
        Option<Value>,
        Option<Value>,
        Option<String>,
        Option<String>,
    );

    fn build(row: Self::Row) -> diesel::deserialize::Result<Self> {
        Ok(Self {
            id: row.0,
            repo_name: row.1,
            pkg_id: row.2,
            pkg_name: row.3,
            pkg_family: row.4,
            pkg_type: row.5,
            version: row.6,
            size: row.7,
            checksum: row.8,
            installed_path: row.9,
            installed_date: row.10,
            profile: row.11,
            pinned: row.12,
            is_installed: row.13,
            detached: row.14,
            unlinked: row.15,
            provides: json_vec!(row.16),
            install_patterns: json_vec!(row.17),
            download_url: row.18,
            update_info: row.19,
        })
    }
}

#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = portable_package)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct PortablePackage {
    pub package_id: i32,
    pub portable_path: Option<String>,
    pub portable_home: Option<String>,
    pub portable_config: Option<String>,
    pub portable_share: Option<String>,
    pub portable_cache: Option<String>,
}

#[derive(Default, Insertable)]
#[diesel(table_name = packages)]
pub struct NewPackage<'a> {
    pub repo_name: &'a str,
    pub pkg_id: Option<&'a str>,
    pub pkg_name: &'a str,
    pub pkg_family: Option<&'a str>,
    pub pkg_type: Option<&'a str>,
    pub version: &'a str,
    pub size: i64,
    pub checksum: Option<&'a str>,
    pub installed_path: &'a str,
    pub installed_date: &'a str,
    pub profile: &'a str,
    pub pinned: bool,
    pub is_installed: bool,
    pub detached: bool,
    pub unlinked: bool,
    pub provides: Option<Value>,
    pub install_patterns: Option<Value>,
    pub download_url: Option<&'a str>,
    pub update_info: Option<&'a str>,
}

#[derive(Default, Insertable)]
#[diesel(table_name = portable_package)]
pub struct NewPortablePackage<'a> {
    pub package_id: i32,
    pub portable_path: Option<&'a str>,
    pub portable_home: Option<&'a str>,
    pub portable_config: Option<&'a str>,
    pub portable_share: Option<&'a str>,
    pub portable_cache: Option<&'a str>,
}
