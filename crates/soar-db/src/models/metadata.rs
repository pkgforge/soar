use diesel::prelude::*;

use crate::{
    models::types::{JsonValue, PackageProvide},
    schema::metadata::*,
};

#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = packages)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct Package {
    pub id: i32,
    pub rank: Option<i32>,
    pub pkg_id: String,
    pub pkg_name: String,
    pub pkg_type: Option<String>,
    pub pkg_webpage: Option<String>,
    pub app_id: Option<String>,
    pub description: Option<String>,
    pub version: String,
    pub version_upstream: Option<String>,
    pub licenses: Option<JsonValue<Vec<String>>>,
    pub download_url: String,
    pub size: Option<i64>,
    pub ghcr_pkg: Option<String>,
    pub ghcr_size: Option<i64>,
    pub ghcr_blob: Option<String>,
    pub ghcr_url: Option<String>,
    pub checksum: Option<String>,
    pub icon: Option<String>,
    pub desktop: Option<String>,
    pub appstream: Option<String>,
    pub homepages: Option<JsonValue<Vec<String>>>,
    pub notes: Option<JsonValue<Vec<String>>>,
    pub source_urls: Option<JsonValue<Vec<String>>>,
    pub tags: Option<JsonValue<Vec<String>>>,
    pub categories: Option<JsonValue<Vec<String>>>,
    pub build_id: Option<String>,
    pub build_date: Option<String>,
    pub build_action: Option<String>,
    pub build_script: Option<String>,
    pub build_log: Option<String>,
    pub provides: Option<JsonValue<Vec<PackageProvide>>>,
    pub snapshots: Option<JsonValue<Vec<String>>>,
    pub replaces: Option<JsonValue<Vec<String>>>,
    pub soar_syms: bool,
    pub desktop_integration: Option<bool>,
    pub portable: Option<bool>,
    pub recurse_provides: Option<bool>,
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
    pub rank: Option<i32>,
    pub pkg_id: &'a str,
    pub pkg_name: &'a str,
    pub pkg_type: Option<&'a str>,
    pub pkg_webpage: Option<&'a str>,
    pub app_id: Option<&'a str>,
    pub description: Option<&'a str>,
    pub version: &'a str,
    pub version_upstream: Option<&'a str>,
    pub licenses: Option<JsonValue<Vec<&'a str>>>,
    pub download_url: &'a str,
    pub size: Option<i64>,
    pub ghcr_pkg: Option<&'a str>,
    pub ghcr_blob: Option<&'a str>,
    pub ghcr_url: Option<&'a str>,
    pub checksum: Option<&'a str>,
    pub icon: Option<&'a str>,
    pub desktop: Option<&'a str>,
    pub appstream: Option<&'a str>,
    pub homepages: Option<JsonValue<Vec<&'a str>>>,
    pub notes: Option<JsonValue<Vec<&'a str>>>,
    pub source_urls: Option<JsonValue<Vec<&'a str>>>,
    pub tags: Option<JsonValue<Vec<&'a str>>>,
    pub categories: Option<JsonValue<Vec<&'a str>>>,
    pub build_id: Option<&'a str>,
    pub build_date: Option<&'a str>,
    pub build_action: Option<&'a str>,
    pub build_script: Option<&'a str>,
    pub build_log: Option<&'a str>,
    pub provides: Option<JsonValue<Vec<PackageProvide>>>,
    pub snapshots: Option<JsonValue<Vec<&'a str>>>,
    pub replaces: Option<JsonValue<Vec<&'a str>>>,
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
