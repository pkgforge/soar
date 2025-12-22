use diesel::{prelude::*, sqlite::Sqlite};
use serde_json::Value;

use crate::{json_vec, models::types::PackageProvide, schema::metadata::*};

#[derive(Debug, Clone, Selectable)]
pub struct Package {
    pub id: i32,
    pub pkg_id: String,
    pub pkg_name: String,
    pub pkg_type: Option<String>,
    pub pkg_webpage: Option<String>,
    pub app_id: Option<String>,
    pub description: Option<String>,
    pub version: String,
    pub version_upstream: Option<String>,
    pub licenses: Option<Vec<String>>,
    pub download_url: String,
    pub size: Option<i64>,
    pub ghcr_pkg: Option<String>,
    pub ghcr_size: Option<i64>,
    pub ghcr_blob: Option<String>,
    pub ghcr_url: Option<String>,
    pub bsum: Option<String>,
    pub icon: Option<String>,
    pub desktop: Option<String>,
    pub appstream: Option<String>,
    pub homepages: Option<Vec<String>>,
    pub notes: Option<Vec<String>>,
    pub source_urls: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub categories: Option<Vec<String>>,
    pub build_id: Option<String>,
    pub build_date: Option<String>,
    pub build_action: Option<String>,
    pub build_script: Option<String>,
    pub build_log: Option<String>,
    pub provides: Option<Vec<PackageProvide>>,
    pub snapshots: Option<Vec<String>>,
    pub replaces: Option<Vec<String>>,
    pub soar_syms: bool,
    pub desktop_integration: Option<bool>,
    pub portable: Option<bool>,
    pub recurse_provides: Option<bool>,
}

impl Queryable<packages::SqlType, Sqlite> for Package {
    type Row = (
        i32,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
        Option<String>,
        Option<Value>,
        String,
        Option<i64>,
        Option<String>,
        Option<i64>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<Value>,
        Option<Value>,
        Option<Value>,
        Option<Value>,
        Option<Value>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<Value>,
        Option<Value>,
        Option<Value>,
        bool,
        Option<bool>,
        Option<bool>,
        Option<bool>,
    );

    fn build(row: Self::Row) -> diesel::deserialize::Result<Self> {
        Ok(Self {
            id: row.0,
            pkg_id: row.1,
            pkg_name: row.2,
            pkg_type: row.3,
            pkg_webpage: row.4,
            app_id: row.5,
            description: row.6,
            version: row.7,
            version_upstream: row.8,
            licenses: json_vec!(row.9),
            download_url: row.10,
            size: row.11,
            ghcr_pkg: row.12,
            ghcr_size: row.13,
            ghcr_blob: row.14,
            ghcr_url: row.15,
            bsum: row.16,
            icon: row.17,
            desktop: row.18,
            appstream: row.19,
            homepages: json_vec!(row.20),
            notes: json_vec!(row.21),
            source_urls: json_vec!(row.22),
            tags: json_vec!(row.23),
            categories: json_vec!(row.24),
            build_id: row.25,
            build_date: row.26,
            build_action: row.27,
            build_script: row.28,
            build_log: row.29,
            provides: json_vec!(row.30),
            snapshots: json_vec!(row.31),
            replaces: json_vec!(row.32),
            soar_syms: row.33,
            desktop_integration: row.34,
            portable: row.35,
            recurse_provides: row.36,
        })
    }
}

/// Package with repository name attached.
/// This is used when querying across multiple repositories.
#[derive(Debug, Clone)]
pub struct PackageWithRepo {
    pub repo_name: String,
    pub package: Package,
}

impl PackageWithRepo {
    pub fn new(repo_name: String, package: Package) -> Self {
        Self {
            repo_name,
            package,
        }
    }
}

#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = maintainers)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct Maintainer {
    pub id: i32,
    pub contact: String,
    pub name: String,
}

#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = package_maintainers)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct PackageMaintainer {
    pub maintainer_id: i32,
    pub package_id: i32,
}

#[derive(Default, Insertable)]
#[diesel(table_name = packages)]
pub struct NewPackage<'a> {
    pub pkg_id: &'a str,
    pub pkg_name: &'a str,
    pub pkg_type: Option<&'a str>,
    pub pkg_webpage: Option<&'a str>,
    pub app_id: Option<&'a str>,
    pub description: Option<&'a str>,
    pub version: &'a str,
    pub version_upstream: Option<&'a str>,
    pub licenses: Option<Value>,
    pub download_url: &'a str,
    pub size: Option<i64>,
    pub ghcr_pkg: Option<&'a str>,
    pub ghcr_size: Option<i64>,
    pub ghcr_blob: Option<&'a str>,
    pub ghcr_url: Option<&'a str>,
    pub bsum: Option<&'a str>,
    pub icon: Option<&'a str>,
    pub desktop: Option<&'a str>,
    pub appstream: Option<&'a str>,
    pub homepages: Option<Value>,
    pub notes: Option<Value>,
    pub source_urls: Option<Value>,
    pub tags: Option<Value>,
    pub categories: Option<Value>,
    pub build_id: Option<&'a str>,
    pub build_date: Option<&'a str>,
    pub build_action: Option<&'a str>,
    pub build_script: Option<&'a str>,
    pub build_log: Option<&'a str>,
    pub provides: Option<Value>,
    pub snapshots: Option<Value>,
    pub replaces: Option<Value>,
    pub soar_syms: bool,
    pub desktop_integration: Option<bool>,
    pub portable: Option<bool>,
    pub recurse_provides: Option<bool>,
}

#[derive(Default, Insertable)]
#[diesel(table_name = maintainers)]
pub struct NewMaintainer<'a> {
    pub contact: &'a str,
    pub name: &'a str,
}

#[derive(Default, Insertable)]
#[diesel(table_name = package_maintainers)]
pub struct NewPackageMaintainer {
    pub maintainer_id: i32,
    pub package_id: i32,
}

#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = repository)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct Repository {
    pub rowid: i32,
    pub name: String,
    pub etag: String,
}

#[derive(Default, Insertable)]
#[diesel(table_name = repository)]
pub struct NewRepository<'a> {
    pub name: &'a str,
    pub etag: &'a str,
}
