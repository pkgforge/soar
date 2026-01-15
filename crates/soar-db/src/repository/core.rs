//! Core database repository for installed packages.

use diesel::{prelude::*, sql_types::Bool, sqlite::Sqlite};

use crate::{
    models::{
        core::{NewPackage, NewPortablePackage, Package, PortablePackage},
        types::PackageProvide,
    },
    schema::core::{packages, portable_package},
};

/// Sort direction for queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Asc,
    Desc,
}

/// Type alias for installed package (for clarity).
pub type InstalledPackage = Package;
/// Type alias for new installed package (for clarity).
pub type NewInstalledPackage<'a> = NewPackage<'a>;

/// Installed package with portable configuration joined.
#[derive(Debug, Clone)]
pub struct InstalledPackageWithPortable {
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
    pub detached: bool,
    pub unlinked: bool,
    pub provides: Option<Vec<PackageProvide>>,
    pub install_patterns: Option<Vec<String>>,
    pub portable_path: Option<String>,
    pub portable_home: Option<String>,
    pub portable_config: Option<String>,
    pub portable_share: Option<String>,
    pub portable_cache: Option<String>,
}

impl From<(Package, Option<PortablePackage>)> for InstalledPackageWithPortable {
    fn from((pkg, portable): (Package, Option<PortablePackage>)) -> Self {
        Self {
            id: pkg.id,
            repo_name: pkg.repo_name,
            pkg_id: pkg.pkg_id,
            pkg_name: pkg.pkg_name,
            pkg_type: pkg.pkg_type,
            version: pkg.version,
            size: pkg.size,
            checksum: pkg.checksum,
            installed_path: pkg.installed_path,
            installed_date: pkg.installed_date,
            profile: pkg.profile,
            pinned: pkg.pinned,
            is_installed: pkg.is_installed,
            detached: pkg.detached,
            unlinked: pkg.unlinked,
            provides: pkg.provides,
            install_patterns: pkg.install_patterns,
            portable_path: portable.as_ref().and_then(|p| p.portable_path.clone()),
            portable_home: portable.as_ref().and_then(|p| p.portable_home.clone()),
            portable_config: portable.as_ref().and_then(|p| p.portable_config.clone()),
            portable_share: portable.as_ref().and_then(|p| p.portable_share.clone()),
            portable_cache: portable.as_ref().and_then(|p| p.portable_cache.clone()),
        }
    }
}

/// Repository for installed package operations.
pub struct CoreRepository;

impl CoreRepository {
    /// Lists all installed packages.
    pub fn list_all(conn: &mut SqliteConnection) -> QueryResult<Vec<Package>> {
        packages::table.select(Package::as_select()).load(conn)
    }

    /// Lists installed packages with flexible filtering.
    #[allow(clippy::too_many_arguments)]
    pub fn list_filtered(
        conn: &mut SqliteConnection,
        repo_name: Option<&str>,
        pkg_name: Option<&str>,
        pkg_id: Option<&str>,
        version: Option<&str>,
        is_installed: Option<bool>,
        pinned: Option<bool>,
        limit: Option<i64>,
        sort_by_id: Option<SortDirection>,
    ) -> QueryResult<Vec<InstalledPackageWithPortable>> {
        let mut query = packages::table
            .left_join(portable_package::table)
            .into_boxed();

        if let Some(repo) = repo_name {
            query = query.filter(packages::repo_name.eq(repo));
        }
        if let Some(name) = pkg_name {
            query = query.filter(packages::pkg_name.eq(name));
        }
        if let Some(id) = pkg_id {
            query = query.filter(packages::pkg_id.eq(id));
        }
        if let Some(ver) = version {
            query = query.filter(packages::version.eq(ver));
        }
        if let Some(installed) = is_installed {
            query = query.filter(packages::is_installed.eq(installed));
        }
        if let Some(pin) = pinned {
            query = query.filter(packages::pinned.eq(pin));
        }

        if let Some(direction) = sort_by_id {
            query = match direction {
                SortDirection::Asc => query.order(packages::id.asc()),
                SortDirection::Desc => query.order(packages::id.desc()),
            };
        }

        if let Some(lim) = limit {
            query = query.limit(lim);
        }

        let results: Vec<(Package, Option<PortablePackage>)> = query
            .select((Package::as_select(), Option::<PortablePackage>::as_select()))
            .load(conn)?;

        Ok(results.into_iter().map(Into::into).collect())
    }

    /// Lists broken packages (is_installed = false).
    pub fn list_broken(
        conn: &mut SqliteConnection,
    ) -> QueryResult<Vec<InstalledPackageWithPortable>> {
        let results: Vec<(Package, Option<PortablePackage>)> = packages::table
            .left_join(portable_package::table)
            .filter(packages::is_installed.eq(false))
            .select((Package::as_select(), Option::<PortablePackage>::as_select()))
            .load(conn)?;

        Ok(results.into_iter().map(Into::into).collect())
    }

    /// Lists installed packages that are not pinned (for updates).
    pub fn list_updatable(
        conn: &mut SqliteConnection,
    ) -> QueryResult<Vec<InstalledPackageWithPortable>> {
        let results: Vec<(Package, Option<PortablePackage>)> = packages::table
            .left_join(portable_package::table)
            .filter(packages::is_installed.eq(true))
            .filter(packages::pinned.eq(false))
            .select((Package::as_select(), Option::<PortablePackage>::as_select()))
            .load(conn)?;

        Ok(results.into_iter().map(Into::into).collect())
    }

    /// Finds an installed package by exact match on repo_name, pkg_name, pkg_id, and version.
    pub fn find_exact(
        conn: &mut SqliteConnection,
        repo_name: &str,
        pkg_name: &str,
        pkg_id: &str,
        version: &str,
    ) -> QueryResult<Option<InstalledPackageWithPortable>> {
        let result: Option<(Package, Option<PortablePackage>)> = packages::table
            .left_join(portable_package::table)
            .filter(packages::repo_name.eq(repo_name))
            .filter(packages::pkg_name.eq(pkg_name))
            .filter(packages::pkg_id.eq(pkg_id))
            .filter(packages::version.eq(version))
            .select((Package::as_select(), Option::<PortablePackage>::as_select()))
            .first(conn)
            .optional()?;

        Ok(result.map(Into::into))
    }

    /// Lists all installed packages with portable configuration.
    pub fn list_all_with_portable(
        conn: &mut SqliteConnection,
    ) -> QueryResult<Vec<InstalledPackageWithPortable>> {
        let results: Vec<(Package, Option<PortablePackage>)> = packages::table
            .left_join(portable_package::table)
            .select((Package::as_select(), Option::<PortablePackage>::as_select()))
            .load(conn)?;

        Ok(results.into_iter().map(Into::into).collect())
    }

    /// Lists installed packages filtered by repo_name.
    pub fn list_by_repo(conn: &mut SqliteConnection, repo_name: &str) -> QueryResult<Vec<Package>> {
        packages::table
            .filter(packages::repo_name.eq(repo_name))
            .select(Package::as_select())
            .load(conn)
    }

    /// Lists installed packages filtered by repo_name with portable configuration.
    pub fn list_by_repo_with_portable(
        conn: &mut SqliteConnection,
        repo_name: &str,
    ) -> QueryResult<Vec<InstalledPackageWithPortable>> {
        let results: Vec<(Package, Option<PortablePackage>)> = packages::table
            .left_join(portable_package::table)
            .filter(packages::repo_name.eq(repo_name))
            .select((Package::as_select(), Option::<PortablePackage>::as_select()))
            .load(conn)?;

        Ok(results.into_iter().map(Into::into).collect())
    }

    /// Counts installed packages.
    pub fn count(conn: &mut SqliteConnection) -> QueryResult<i64> {
        packages::table.count().get_result(conn)
    }

    /// Counts distinct installed packages.
    pub fn count_distinct_installed(
        conn: &mut SqliteConnection,
        repo_name: Option<&str>,
    ) -> QueryResult<i64> {
        use diesel::dsl::sql;

        let mut query = packages::table
            .filter(packages::is_installed.eq(true))
            .into_boxed();

        if let Some(repo) = repo_name {
            query = query.filter(packages::repo_name.eq(repo));
        }

        query
            .select(sql::<diesel::sql_types::BigInt>(
                "COUNT(DISTINCT pkg_id || '\x00' || pkg_name)",
            ))
            .first(conn)
    }

    /// Finds an installed package by ID.
    pub fn find_by_id(conn: &mut SqliteConnection, id: i32) -> QueryResult<Option<Package>> {
        packages::table
            .filter(packages::id.eq(id))
            .select(Package::as_select())
            .first(conn)
            .optional()
    }

    /// Finds an installed package by ID with portable configuration.
    pub fn find_by_id_with_portable(
        conn: &mut SqliteConnection,
        id: i32,
    ) -> QueryResult<Option<InstalledPackageWithPortable>> {
        let result: Option<(Package, Option<PortablePackage>)> = packages::table
            .left_join(portable_package::table)
            .filter(packages::id.eq(id))
            .select((Package::as_select(), Option::<PortablePackage>::as_select()))
            .first(conn)
            .optional()?;

        Ok(result.map(Into::into))
    }

    /// Finds installed packages by name.
    pub fn find_by_name(conn: &mut SqliteConnection, name: &str) -> QueryResult<Vec<Package>> {
        packages::table
            .filter(packages::pkg_name.eq(name))
            .select(Package::as_select())
            .load(conn)
    }

    /// Finds installed packages by name with portable configuration.
    pub fn find_by_name_with_portable(
        conn: &mut SqliteConnection,
        name: &str,
    ) -> QueryResult<Vec<InstalledPackageWithPortable>> {
        let results: Vec<(Package, Option<PortablePackage>)> = packages::table
            .left_join(portable_package::table)
            .filter(packages::pkg_name.eq(name))
            .select((Package::as_select(), Option::<PortablePackage>::as_select()))
            .load(conn)?;

        Ok(results.into_iter().map(Into::into).collect())
    }

    /// Finds installed packages by name, excluding specific pkg_id and version.
    pub fn find_alternates(
        conn: &mut SqliteConnection,
        pkg_name: &str,
        exclude_pkg_id: &str,
        exclude_version: &str,
    ) -> QueryResult<Vec<InstalledPackageWithPortable>> {
        let results: Vec<(Package, Option<PortablePackage>)> = packages::table
            .left_join(portable_package::table)
            .filter(packages::pkg_name.eq(pkg_name))
            .filter(packages::pkg_id.ne(exclude_pkg_id))
            .filter(packages::version.ne(exclude_version))
            .select((Package::as_select(), Option::<PortablePackage>::as_select()))
            .load(conn)?;

        Ok(results.into_iter().map(Into::into).collect())
    }

    /// Finds an installed package by pkg_id and repo_name.
    pub fn find_by_pkg_id_and_repo(
        conn: &mut SqliteConnection,
        pkg_id: &str,
        repo_name: &str,
    ) -> QueryResult<Option<Package>> {
        packages::table
            .filter(packages::pkg_id.eq(pkg_id))
            .filter(packages::repo_name.eq(repo_name))
            .select(Package::as_select())
            .first(conn)
            .optional()
    }

    /// Finds an installed package by pkg_id, pkg_name, and repo_name.
    pub fn find_by_pkg_id_name_and_repo(
        conn: &mut SqliteConnection,
        pkg_id: &str,
        pkg_name: &str,
        repo_name: &str,
    ) -> QueryResult<Option<Package>> {
        packages::table
            .filter(packages::pkg_id.eq(pkg_id))
            .filter(packages::pkg_name.eq(pkg_name))
            .filter(packages::repo_name.eq(repo_name))
            .select(Package::as_select())
            .first(conn)
            .optional()
    }

    /// Inserts a new installed package and returns the inserted ID.
    pub fn insert(conn: &mut SqliteConnection, package: &NewPackage) -> QueryResult<i32> {
        diesel::insert_into(packages::table)
            .values(package)
            .returning(packages::id)
            .get_result(conn)
    }

    /// Updates an installed package's version.
    pub fn update_version(
        conn: &mut SqliteConnection,
        id: i32,
        new_version: &str,
    ) -> QueryResult<usize> {
        diesel::update(packages::table.filter(packages::id.eq(id)))
            .set(packages::version.eq(new_version))
            .execute(conn)
    }

    /// Updates an installed package after successful installation.
    /// Only updates the record with is_installed=false (the newly created one).
    #[allow(clippy::too_many_arguments)]
    pub fn record_installation(
        conn: &mut SqliteConnection,
        repo_name: &str,
        pkg_name: &str,
        pkg_id: &str,
        version: &str,
        size: i64,
        provides: Option<Vec<PackageProvide>>,
        checksum: Option<&str>,
        installed_date: &str,
        installed_path: &str,
    ) -> QueryResult<Option<i32>> {
        let provides = provides.map(|v| serde_json::to_value(v).unwrap_or_default());
        diesel::update(
            packages::table
                .filter(packages::repo_name.eq(repo_name))
                .filter(packages::pkg_name.eq(pkg_name))
                .filter(packages::pkg_id.eq(pkg_id))
                .filter(packages::version.eq(version))
                .filter(packages::is_installed.eq(false)),
        )
        .set((
            packages::size.eq(size),
            packages::installed_date.eq(installed_date),
            packages::is_installed.eq(true),
            packages::provides.eq(provides),
            packages::checksum.eq(checksum),
            packages::installed_path.eq(installed_path),
        ))
        .returning(packages::id)
        .get_result(conn)
        .optional()
    }

    /// Sets the pinned status of a package.
    pub fn set_pinned(conn: &mut SqliteConnection, id: i32, pinned: bool) -> QueryResult<usize> {
        diesel::update(packages::table.filter(packages::id.eq(id)))
            .set(packages::pinned.eq(pinned))
            .execute(conn)
    }

    /// Sets the unlinked status of a package.
    pub fn set_unlinked(
        conn: &mut SqliteConnection,
        id: i32,
        unlinked: bool,
    ) -> QueryResult<usize> {
        diesel::update(packages::table.filter(packages::id.eq(id)))
            .set(packages::unlinked.eq(unlinked))
            .execute(conn)
    }

    /// Unlinks all packages with a given name except those matching pkg_id and version.
    pub fn unlink_others(
        conn: &mut SqliteConnection,
        pkg_name: &str,
        keep_pkg_id: &str,
        keep_version: &str,
    ) -> QueryResult<usize> {
        diesel::update(
            packages::table
                .filter(packages::pkg_name.eq(pkg_name))
                .filter(
                    packages::pkg_id
                        .ne(keep_pkg_id)
                        .or(packages::version.ne(keep_version)),
                ),
        )
        .set(packages::unlinked.eq(true))
        .execute(conn)
    }

    /// Updates the pkg_id for packages matching repo_name and old pkg_id.
    pub fn update_pkg_id(
        conn: &mut SqliteConnection,
        repo_name: &str,
        old_pkg_id: &str,
        new_pkg_id: &str,
    ) -> QueryResult<usize> {
        diesel::update(
            packages::table
                .filter(packages::repo_name.eq(repo_name))
                .filter(packages::pkg_id.eq(old_pkg_id)),
        )
        .set(packages::pkg_id.eq(new_pkg_id))
        .execute(conn)
    }

    /// Deletes an installed package by ID.
    pub fn delete(conn: &mut SqliteConnection, id: i32) -> QueryResult<usize> {
        diesel::delete(packages::table.filter(packages::id.eq(id))).execute(conn)
    }

    /// Checks if a pending install (is_installed=false) exists for a specific package version.
    /// Used to check if we can resume a partial install.
    pub fn has_pending_install(
        conn: &mut SqliteConnection,
        pkg_id: &str,
        pkg_name: &str,
        repo_name: &str,
        version: &str,
    ) -> QueryResult<bool> {
        let count: i64 = packages::table
            .filter(packages::pkg_id.eq(pkg_id))
            .filter(packages::pkg_name.eq(pkg_name))
            .filter(packages::repo_name.eq(repo_name))
            .filter(packages::version.eq(version))
            .filter(packages::is_installed.eq(false))
            .count()
            .get_result(conn)?;
        Ok(count > 0)
    }

    /// Deletes pending (is_installed=false) records for a package and returns their paths.
    /// Used to clean up orphaned partial installs before starting a new install.
    pub fn delete_pending_installs(
        conn: &mut SqliteConnection,
        pkg_id: &str,
        pkg_name: &str,
        repo_name: &str,
    ) -> QueryResult<Vec<String>> {
        let paths: Vec<String> = packages::table
            .filter(packages::pkg_id.eq(pkg_id))
            .filter(packages::pkg_name.eq(pkg_name))
            .filter(packages::repo_name.eq(repo_name))
            .filter(packages::is_installed.eq(false))
            .select(packages::installed_path)
            .load(conn)?;

        diesel::delete(
            packages::table
                .filter(packages::pkg_id.eq(pkg_id))
                .filter(packages::pkg_name.eq(pkg_name))
                .filter(packages::repo_name.eq(repo_name))
                .filter(packages::is_installed.eq(false)),
        )
        .execute(conn)?;

        Ok(paths)
    }

    /// Gets the portable package configuration for a package.
    pub fn get_portable(
        conn: &mut SqliteConnection,
        package_id: i32,
    ) -> QueryResult<Option<PortablePackage>> {
        portable_package::table
            .filter(portable_package::package_id.eq(package_id))
            .select(PortablePackage::as_select())
            .first(conn)
            .optional()
    }

    /// Inserts portable package configuration.
    pub fn insert_portable(
        conn: &mut SqliteConnection,
        portable: &NewPortablePackage,
    ) -> QueryResult<usize> {
        diesel::insert_into(portable_package::table)
            .values(portable)
            .execute(conn)
    }

    /// Updates or inserts portable package configuration.
    pub fn upsert_portable(
        conn: &mut SqliteConnection,
        package_id: i32,
        portable_path: Option<&str>,
        portable_home: Option<&str>,
        portable_config: Option<&str>,
        portable_share: Option<&str>,
        portable_cache: Option<&str>,
    ) -> QueryResult<usize> {
        let updated = diesel::update(
            portable_package::table.filter(portable_package::package_id.eq(package_id)),
        )
        .set((
            portable_package::portable_path.eq(portable_path),
            portable_package::portable_home.eq(portable_home),
            portable_package::portable_config.eq(portable_config),
            portable_package::portable_share.eq(portable_share),
            portable_package::portable_cache.eq(portable_cache),
        ))
        .execute(conn)?;

        if updated == 0 {
            diesel::insert_into(portable_package::table)
                .values(&NewPortablePackage {
                    package_id,
                    portable_path,
                    portable_home,
                    portable_config,
                    portable_share,
                    portable_cache,
                })
                .execute(conn)
        } else {
            Ok(updated)
        }
    }

    /// Deletes portable package configuration.
    pub fn delete_portable(conn: &mut SqliteConnection, package_id: i32) -> QueryResult<usize> {
        diesel::delete(portable_package::table.filter(portable_package::package_id.eq(package_id)))
            .execute(conn)
    }

    /// Gets old package versions (all except the newest one) for cleanup.
    /// Returns the installed paths of packages to remove.
    /// If `force` is true, includes pinned packages. Otherwise only unpinned packages.
    pub fn get_old_package_paths(
        conn: &mut SqliteConnection,
        pkg_id: &str,
        pkg_name: &str,
        repo_name: &str,
        force: bool,
    ) -> QueryResult<Vec<(i32, String)>> {
        let latest: Option<(i32, String)> = packages::table
            .filter(packages::pkg_id.eq(pkg_id))
            .filter(packages::pkg_name.eq(pkg_name))
            .filter(packages::repo_name.eq(repo_name))
            .order(packages::id.desc())
            .select((packages::id, packages::installed_path))
            .first(conn)
            .optional()?;

        let Some((latest_id, latest_path)) = latest else {
            return Ok(Vec::new());
        };

        let query = packages::table
            .filter(packages::pkg_id.eq(pkg_id))
            .filter(packages::pkg_name.eq(pkg_name))
            .filter(packages::repo_name.eq(repo_name))
            .filter(packages::id.ne(latest_id))
            .filter(packages::installed_path.ne(&latest_path))
            .into_boxed();

        let query = if force {
            query
        } else {
            query.filter(packages::pinned.eq(false))
        };

        query
            .select((packages::id, packages::installed_path))
            .load(conn)
    }

    /// Deletes old package versions (all except the newest one).
    /// If `force` is true, deletes pinned packages too. Otherwise only unpinned packages.
    pub fn delete_old_packages(
        conn: &mut SqliteConnection,
        pkg_id: &str,
        pkg_name: &str,
        repo_name: &str,
        force: bool,
    ) -> QueryResult<usize> {
        let latest_id: Option<i32> = packages::table
            .filter(packages::pkg_id.eq(pkg_id))
            .filter(packages::pkg_name.eq(pkg_name))
            .filter(packages::repo_name.eq(repo_name))
            .order(packages::id.desc())
            .select(packages::id)
            .first(conn)
            .optional()?;

        let Some(latest_id) = latest_id else {
            return Ok(0);
        };

        let pinned_filter: Box<dyn BoxableExpression<packages::table, Sqlite, SqlType = Bool>> =
            if force {
                Box::new(diesel::dsl::sql::<Bool>("TRUE"))
            } else {
                Box::new(packages::pinned.eq(false))
            };

        let query = packages::table
            .filter(packages::pkg_id.eq(pkg_id))
            .filter(packages::pkg_name.eq(pkg_name))
            .filter(packages::repo_name.eq(repo_name))
            .filter(packages::id.ne(latest_id))
            .filter(pinned_filter);

        diesel::delete(query).execute(conn)
    }

    /// Unlinks all packages with a given name except those matching pkg_id and checksum.
    /// Used when switching between alternate package versions.
    pub fn unlink_others_by_checksum(
        conn: &mut SqliteConnection,
        pkg_name: &str,
        keep_pkg_id: &str,
        keep_checksum: Option<&str>,
    ) -> QueryResult<usize> {
        if let Some(checksum) = keep_checksum {
            diesel::update(
                packages::table
                    .filter(packages::pkg_name.eq(pkg_name))
                    .filter(packages::pkg_id.ne(keep_pkg_id))
                    .filter(packages::checksum.ne(checksum)),
            )
            .set(packages::unlinked.eq(true))
            .execute(conn)
        } else {
            diesel::update(
                packages::table
                    .filter(packages::pkg_name.eq(pkg_name))
                    .filter(packages::pkg_id.ne(keep_pkg_id)),
            )
            .set(packages::unlinked.eq(true))
            .execute(conn)
        }
    }

    /// Links a package by pkg_name, pkg_id, and checksum.
    /// Used when switching to an alternate package version.
    pub fn link_by_checksum(
        conn: &mut SqliteConnection,
        pkg_name: &str,
        pkg_id: &str,
        checksum: Option<&str>,
    ) -> QueryResult<usize> {
        if let Some(checksum) = checksum {
            diesel::update(
                packages::table
                    .filter(packages::pkg_name.eq(pkg_name))
                    .filter(packages::pkg_id.eq(pkg_id))
                    .filter(packages::checksum.eq(checksum)),
            )
            .set(packages::unlinked.eq(false))
            .execute(conn)
        } else {
            diesel::update(
                packages::table
                    .filter(packages::pkg_name.eq(pkg_name))
                    .filter(packages::pkg_id.eq(pkg_id)),
            )
            .set(packages::unlinked.eq(false))
            .execute(conn)
        }
    }
}
