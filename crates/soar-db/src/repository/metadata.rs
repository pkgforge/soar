//! Metadata database repository for package queries.

use std::sync::OnceLock;

use diesel::{dsl::sql, prelude::*, sql_types::Text};
use regex::Regex;
use serde_json::json;
use soar_registry::RemotePackage;

/// Regex for extracting name and contact from maintainer string format "Name (contact)".
static MAINTAINER_RE: OnceLock<Regex> = OnceLock::new();

use super::core::SortDirection;
use crate::{
    models::{
        metadata::{
            Maintainer, NewMaintainer, NewPackage, NewPackageMaintainer, NewRepository, Package,
        },
        types::PackageProvide,
    },
    schema::metadata::{maintainers, package_maintainers, packages, repository},
};

/// Helper struct for raw SQL queries returning just pkg_id.
#[derive(Debug, QueryableByName)]
struct PkgIdOnly {
    #[diesel(sql_type = Text)]
    pkg_id: String,
}

/// Repository for package metadata operations.
pub struct MetadataRepository;

impl MetadataRepository {
    /// Lists all packages using Diesel DSL.
    pub fn list_all(conn: &mut SqliteConnection) -> QueryResult<Vec<Package>> {
        packages::table
            .order(packages::pkg_name.asc())
            .select(Package::as_select())
            .load(conn)
    }

    /// Lists packages with pagination and sorting using Diesel DSL.
    pub fn list_paginated(
        conn: &mut SqliteConnection,
        page: i64,
        per_page: i64,
    ) -> QueryResult<Vec<Package>> {
        let offset = (page - 1) * per_page;

        packages::table
            .order(packages::pkg_name.asc())
            .limit(per_page)
            .offset(offset)
            .select(Package::as_select())
            .load(conn)
    }

    /// Gets the repository name from the database.
    pub fn get_repo_name(conn: &mut SqliteConnection) -> QueryResult<Option<String>> {
        repository::table
            .select(repository::name)
            .first(conn)
            .optional()
    }

    /// Gets the repository etag from the database.
    pub fn get_repo_etag(conn: &mut SqliteConnection) -> QueryResult<Option<String>> {
        repository::table
            .select(repository::etag)
            .first(conn)
            .optional()
    }

    /// Updates the repository metadata (name and etag).
    pub fn update_repo_metadata(
        conn: &mut SqliteConnection,
        name: &str,
        etag: &str,
    ) -> QueryResult<usize> {
        diesel::update(repository::table)
            .set((repository::name.eq(name), repository::etag.eq(etag)))
            .execute(conn)
    }

    /// Finds a package by ID using Diesel DSL.
    pub fn find_by_id(conn: &mut SqliteConnection, id: i32) -> QueryResult<Option<Package>> {
        packages::table
            .filter(packages::id.eq(id))
            .select(Package::as_select())
            .first(conn)
            .optional()
    }

    /// Finds packages by name (exact match) using Diesel DSL.
    pub fn find_by_name(conn: &mut SqliteConnection, name: &str) -> QueryResult<Vec<Package>> {
        packages::table
            .filter(packages::pkg_name.eq(name))
            .select(Package::as_select())
            .load(conn)
    }

    /// Finds a package by pkg_id using Diesel DSL.
    pub fn find_by_pkg_id(
        conn: &mut SqliteConnection,
        pkg_id: &str,
    ) -> QueryResult<Option<Package>> {
        packages::table
            .filter(packages::pkg_id.eq(pkg_id))
            .select(Package::as_select())
            .first(conn)
            .optional()
    }

    /// Finds packages that match pkg_name and optionally pkg_id and version using Diesel DSL.
    pub fn find_by_query(
        conn: &mut SqliteConnection,
        pkg_name: Option<&str>,
        pkg_id: Option<&str>,
        version: Option<&str>,
    ) -> QueryResult<Vec<Package>> {
        let mut query = packages::table.into_boxed();

        if let Some(name) = pkg_name {
            query = query.filter(packages::pkg_name.eq(name));
        }
        if let Some(id) = pkg_id {
            if id != "all" {
                query = query.filter(packages::pkg_id.eq(id));
            }
        }
        if let Some(ver) = version {
            query = query.filter(packages::version.eq(ver));
        }

        query.select(Package::as_select()).load(conn)
    }

    /// Searches packages by pattern (case-insensitive LIKE query) using Diesel DSL.
    /// Searches across pkg_name and pkg_id fields.
    pub fn search(
        conn: &mut SqliteConnection,
        pattern: &str,
        limit: Option<i64>,
    ) -> QueryResult<Vec<Package>> {
        let like_pattern = format!("%{}%", pattern.to_lowercase());

        let mut query = packages::table
            .filter(
                sql::<diesel::sql_types::Bool>("LOWER(pkg_name) LIKE ")
                    .bind::<Text, _>(&like_pattern)
                    .sql(" OR LOWER(pkg_id) LIKE ")
                    .bind::<Text, _>(&like_pattern),
            )
            .order(packages::pkg_name.asc())
            .into_boxed();

        if let Some(lim) = limit {
            query = query.limit(lim);
        }

        query.select(Package::as_select()).load(conn)
    }

    /// Searches packages (case-sensitive LIKE query) using Diesel DSL.
    pub fn search_case_sensitive(
        conn: &mut SqliteConnection,
        pattern: &str,
        limit: Option<i64>,
    ) -> QueryResult<Vec<Package>> {
        let like_pattern = format!("%{}%", pattern);

        let mut query = packages::table
            .filter(
                packages::pkg_name
                    .like(&like_pattern)
                    .or(packages::pkg_id.like(&like_pattern)),
            )
            .order(packages::pkg_name.asc())
            .into_boxed();

        if let Some(lim) = limit {
            query = query.limit(lim);
        }

        query.select(Package::as_select()).load(conn)
    }

    /// Checks if a package exists that replaces the given pkg_id.
    /// Returns the pkg_id of the replacement package if found.
    /// Uses raw SQL for JSON array search since Diesel doesn't support json_each.
    pub fn find_replacement_pkg_id(
        conn: &mut SqliteConnection,
        pkg_id: &str,
    ) -> QueryResult<Option<String>> {
        let query = "SELECT pkg_id FROM packages WHERE EXISTS \
                     (SELECT 1 FROM json_each(replaces) WHERE json_each.value = ?) LIMIT 1";

        diesel::sql_query(query)
            .bind::<Text, _>(pkg_id)
            .load::<PkgIdOnly>(conn)
            .map(|mut v| v.pop().map(|p| p.pkg_id))
    }

    /// Counts total packages.
    pub fn count(conn: &mut SqliteConnection) -> QueryResult<i64> {
        packages::table.count().get_result(conn)
    }

    /// Counts packages matching a search pattern using Diesel DSL.
    pub fn count_search(conn: &mut SqliteConnection, pattern: &str) -> QueryResult<i64> {
        let like_pattern = format!("%{}%", pattern.to_lowercase());

        packages::table
            .filter(
                sql::<diesel::sql_types::Bool>("LOWER(pkg_name) LIKE ")
                    .bind::<Text, _>(&like_pattern)
                    .sql(" OR LOWER(pkg_id) LIKE ")
                    .bind::<Text, _>(&like_pattern),
            )
            .count()
            .get_result(conn)
    }

    /// Inserts a new package.
    pub fn insert(conn: &mut SqliteConnection, package: &NewPackage) -> QueryResult<usize> {
        diesel::insert_into(packages::table)
            .values(package)
            .execute(conn)
    }

    /// Gets the last inserted package ID.
    pub fn last_insert_id(conn: &mut SqliteConnection) -> QueryResult<i32> {
        diesel::select(sql::<diesel::sql_types::Integer>("last_insert_rowid()")).get_result(conn)
    }

    /// Finds or creates a maintainer.
    pub fn find_or_create_maintainer(
        conn: &mut SqliteConnection,
        contact: &str,
        name: &str,
    ) -> QueryResult<i32> {
        let existing: Option<Maintainer> = maintainers::table
            .filter(maintainers::contact.eq(contact))
            .select(Maintainer::as_select())
            .first(conn)
            .optional()?;

        if let Some(m) = existing {
            return Ok(m.id);
        }

        let new_maintainer = NewMaintainer {
            contact,
            name,
        };
        diesel::insert_into(maintainers::table)
            .values(&new_maintainer)
            .execute(conn)?;

        Self::last_insert_id(conn)
    }

    /// Links a maintainer to a package.
    pub fn link_maintainer(
        conn: &mut SqliteConnection,
        package_id: i32,
        maintainer_id: i32,
    ) -> QueryResult<usize> {
        let link = NewPackageMaintainer {
            package_id,
            maintainer_id,
        };
        diesel::insert_into(package_maintainers::table)
            .values(&link)
            .on_conflict_do_nothing()
            .execute(conn)
    }

    /// Gets maintainers for a package.
    pub fn get_maintainers(
        conn: &mut SqliteConnection,
        package_id: i32,
    ) -> QueryResult<Vec<Maintainer>> {
        maintainers::table
            .inner_join(
                package_maintainers::table
                    .on(maintainers::id.eq(package_maintainers::maintainer_id)),
            )
            .filter(package_maintainers::package_id.eq(package_id))
            .select(Maintainer::as_select())
            .load(conn)
    }

    /// Deletes all packages (for reimport).
    pub fn delete_all(conn: &mut SqliteConnection) -> QueryResult<usize> {
        diesel::delete(packages::table).execute(conn)
    }

    /// Finds packages with flexible filtering using Diesel DSL.
    pub fn find_filtered(
        conn: &mut SqliteConnection,
        pkg_name: Option<&str>,
        pkg_id: Option<&str>,
        version: Option<&str>,
        limit: Option<i64>,
        sort_by_name: Option<SortDirection>,
    ) -> QueryResult<Vec<Package>> {
        let mut query = packages::table.into_boxed();

        if let Some(name) = pkg_name {
            query = query.filter(packages::pkg_name.eq(name));
        }
        if let Some(id) = pkg_id {
            if id != "all" {
                query = query.filter(packages::pkg_id.eq(id));
            }
        }
        if let Some(ver) = version {
            query = query.filter(packages::version.eq(ver));
        }

        if let Some(direction) = sort_by_name {
            query = match direction {
                SortDirection::Asc => query.order(packages::pkg_name.asc()),
                SortDirection::Desc => query.order(packages::pkg_name.desc()),
            };
        }

        if let Some(lim) = limit {
            query = query.limit(lim);
        }

        query.select(Package::as_select()).load(conn)
    }

    /// Finds packages with a newer version than the given version.
    /// Used for update checking.
    /// Uses Diesel DSL with raw SQL filter for version comparison.
    pub fn find_newer_version(
        conn: &mut SqliteConnection,
        pkg_name: &str,
        pkg_id: &str,
        current_version: &str,
    ) -> QueryResult<Option<Package>> {
        // Handle both regular versions and HEAD- versions
        let head_version = if current_version.starts_with("HEAD-") && current_version.len() > 14 {
            current_version[14..].to_string()
        } else {
            String::new()
        };

        packages::table
            .filter(packages::pkg_name.eq(pkg_name))
            .filter(packages::pkg_id.eq(pkg_id))
            .filter(
                sql::<diesel::sql_types::Bool>("version > ")
                    .bind::<Text, _>(current_version)
                    .sql(" OR (version LIKE 'HEAD-%' AND substr(version, 14) > ")
                    .bind::<Text, _>(&head_version)
                    .sql(")"),
            )
            .order(packages::version.desc())
            .select(Package::as_select())
            .first(conn)
            .optional()
    }

    /// Checks if a package with the given pkg_id exists.
    pub fn exists_by_pkg_id(conn: &mut SqliteConnection, pkg_id: &str) -> QueryResult<bool> {
        diesel::select(diesel::dsl::exists(
            packages::table.filter(packages::pkg_id.eq(pkg_id)),
        ))
        .get_result(conn)
    }

    /// Imports packages from remote metadata (JSON format).
    pub fn import_packages(
        conn: &mut SqliteConnection,
        metadata: &[RemotePackage],
        repo_name: &str,
    ) -> QueryResult<()> {
        conn.transaction(|conn| {
            diesel::insert_into(repository::table)
                .values(NewRepository {
                    name: repo_name,
                    etag: "",
                })
                .on_conflict(repository::name)
                .do_update()
                .set(repository::etag.eq(""))
                .execute(conn)?;

            for package in metadata {
                Self::insert_remote_package(conn, package)?;
            }
            Ok(())
        })
    }

    /// Inserts a single remote package.
    fn insert_remote_package(
        conn: &mut SqliteConnection,
        package: &RemotePackage,
    ) -> QueryResult<()> {
        const PROVIDES_DELIMITERS: &[&str] = &["==", "=>", ":"];

        let provides = package.provides.as_ref().map(|vec| {
            vec.iter()
                .filter_map(|p| {
                    let include = *p == package.pkg_name
                        || matches!(package.recurse_provides, Some(true))
                        || p.strip_prefix(&package.pkg_name).is_some_and(|rest| {
                            PROVIDES_DELIMITERS.iter().any(|d| rest.starts_with(d))
                        });

                    include.then(|| PackageProvide::from_string(p))
                })
                .collect::<Vec<_>>()
        });

        let new_package = NewPackage {
            pkg_id: &package.pkg_id,
            pkg_name: &package.pkg_name,
            pkg_type: package.pkg_type.as_deref(),
            pkg_webpage: package.pkg_webpage.as_deref(),
            app_id: package.app_id.as_deref(),
            description: Some(&package.description),
            version: &package.version,
            version_upstream: package.version_upstream.as_deref(),
            licenses: Some(json!(package.licenses)),
            download_url: &package.download_url,
            size: package.size_raw.map(|s| s as i64),
            ghcr_pkg: package.ghcr_pkg.as_deref(),
            ghcr_size: package.ghcr_size_raw.map(|s| s as i64),
            ghcr_blob: package.ghcr_blob.as_deref(),
            ghcr_url: package.ghcr_url.as_deref(),
            bsum: package.bsum.as_deref(),
            icon: package.icon.as_deref(),
            desktop: package.desktop.as_deref(),
            appstream: package.appstream.as_deref(),
            homepages: Some(json!(package.homepages)),
            notes: Some(json!(package.notes)),
            source_urls: Some(json!(package.src_urls)),
            tags: Some(json!(&package.tags)),
            categories: Some(json!(package.categories)),
            build_id: package.build_id.as_deref(),
            build_date: package.build_date.as_deref(),
            build_action: package.build_action.as_deref(),
            build_script: package.build_script.as_deref(),
            build_log: package.build_log.as_deref(),
            provides: Some(json!(provides)),
            snapshots: Some(json!(package.snapshots)),
            replaces: Some(json!(package.replaces)),
            soar_syms: package.soar_syms.unwrap_or(false),
            desktop_integration: package.desktop_integration,
            portable: package.portable,
            recurse_provides: package.recurse_provides,
        };

        let inserted = diesel::insert_into(packages::table)
            .values(&new_package)
            .on_conflict((packages::pkg_id, packages::pkg_name, packages::version))
            .do_nothing()
            .execute(conn)?;

        if inserted == 0 {
            return Ok(());
        }

        let package_id = Self::last_insert_id(conn)?;

        if let Some(maintainers) = &package.maintainers {
            for maintainer in maintainers {
                if let Some((name, contact)) = Self::extract_name_and_contact(maintainer) {
                    let maintainer_id = Self::find_or_create_maintainer(conn, &contact, &name)?;
                    Self::link_maintainer(conn, package_id, maintainer_id)?;
                }
            }
        }

        Ok(())
    }

    /// Extracts name and contact from maintainer string format "Name (contact)".
    fn extract_name_and_contact(input: &str) -> Option<(String, String)> {
        let re = MAINTAINER_RE.get_or_init(|| Regex::new(r"^([^()]+) \(([^)]+)\)$").unwrap());

        if let Some(captures) = re.captures(input) {
            let name = captures.get(1).map_or("", |m| m.as_str()).to_string();
            let contact = captures.get(2).map_or("", |m| m.as_str()).to_string();
            Some((name, contact))
        } else {
            None
        }
    }
}
