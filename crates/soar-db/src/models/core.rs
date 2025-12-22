use diesel::{prelude::*, sqlite::Sqlite};
use serde_json::Value;

use crate::{models::types::PackageProvide, schema::core::*};

#[derive(Debug, Selectable)]
pub struct Package {
    pub id: i32,
    pub repo_name: String,
    pub pkg_id: String,
    pub pkg_name: String,
    pub pkg_type: Option<String>,
    pub version: String,
    pub size: i64,
    pub checksum: Option<String>,
    pub installed_path: String,
    pub installed_date: String,
    pub profile: String,
    pub pinned: bool,
    pub is_installed: bool,
    pub with_pkg_id: bool,
    pub detached: bool,
    pub unlinked: bool,
    pub provides: Option<Vec<PackageProvide>>,
    pub install_patterns: Option<Vec<String>>,
}

impl Queryable<packages::SqlType, Sqlite> for Package {
    type Row = (
        i32,
        String,
        String,
        String,
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
        bool,
        Option<Value>,
        Option<Value>,
    );

    fn build(row: Self::Row) -> diesel::deserialize::Result<Self> {
        Ok(Self {
            id: row.0,
            repo_name: row.1,
            pkg_id: row.2,
            pkg_name: row.3,
            pkg_type: row.4,
            version: row.5,
            size: row.6,
            checksum: row.7,
            installed_path: row.8,
            installed_date: row.9,
            profile: row.10,
            pinned: row.11,
            is_installed: row.12,
            with_pkg_id: row.13,
            detached: row.14,
            unlinked: row.15,
            provides: row
                .16
                .map(|v| serde_json::from_value(v).unwrap_or_default()),
            install_patterns: row
                .17
                .map(|v| serde_json::from_value(v).unwrap_or_default()),
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
    pub pkg_id: &'a str,
    pub pkg_name: &'a str,
    pub pkg_type: Option<&'a str>,
    pub version: &'a str,
    pub size: i64,
    pub checksum: Option<&'a str>,
    pub installed_path: &'a str,
    pub installed_date: &'a str,
    pub profile: &'a str,
    pub pinned: bool,
    pub is_installed: bool,
    pub with_pkg_id: bool,
    pub detached: bool,
    pub unlinked: bool,
    pub provides: Option<Value>,
    pub install_patterns: Option<Value>,
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
