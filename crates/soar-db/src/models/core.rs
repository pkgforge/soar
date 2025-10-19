use diesel::prelude::*;

use crate::{
    models::types::{JsonValue, PackageProvide},
    schema::core::*,
};

#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = packages)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct Package {
    pub id: i32,
    pub repo_name: String,
    pub pkg: Option<String>,
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
    pub provides: Option<JsonValue<Vec<PackageProvide>>>,
    pub install_patterns: Option<JsonValue<Vec<String>>>,
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
    pub pkg: Option<&'a str>,
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
    pub provides: Option<JsonValue<Vec<PackageProvide>>>,
    pub install_patterns: Option<JsonValue<Vec<String>>>,
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
