use std::{collections::HashMap, path::PathBuf};

use rayon::iter::{IntoParallelIterator, ParallelIterator};
use soar_core::{
    database::models::{InstalledPackage, Package},
    SoarResult,
};
use soar_db::{
    models::metadata::PackageListing,
    repository::{core::CoreRepository, metadata::MetadataRepository},
};
use soar_utils::{fs::dir_size, version::compare_versions};
use tracing::{debug, trace};

use crate::{
    utils::{is_installed, InstalledIndex, NameCounts, PackageKey},
    InstalledEntry, InstalledListResult, PackageListEntry, PackageListResult, SoarContext,
};

/// List all available packages, optionally filtered by repository.
pub async fn list_packages(
    ctx: &SoarContext,
    repo_name: Option<&str>,
) -> SoarResult<PackageListResult> {
    debug!(repo = ?repo_name, "listing packages");
    let metadata_mgr = ctx.metadata_manager().await?;
    let diesel_db = ctx.diesel_core_db()?;

    struct ListingWithRepo {
        repo_name: String,
        pkg: PackageListing,
    }

    let packages: Vec<ListingWithRepo> = if let Some(repo_name) = repo_name {
        metadata_mgr
            .query_repo(repo_name, MetadataRepository::list_all_minimal)?
            .unwrap_or_default()
            .into_iter()
            .map(|pkg| {
                ListingWithRepo {
                    repo_name: repo_name.to_string(),
                    pkg,
                }
            })
            .collect()
    } else {
        metadata_mgr.query_all_flat(|repo_name, conn| {
            let pkgs = MetadataRepository::list_all_minimal(conn)?;
            Ok(pkgs
                .into_iter()
                .map(|pkg| {
                    ListingWithRepo {
                        repo_name: repo_name.to_string(),
                        pkg,
                    }
                })
                .collect())
        })?
    };

    // One row per package, not per version. A repository publishes every
    // version it knows, and listing them all buries the packages themselves.
    let mut newest: HashMap<PackageKey, ListingWithRepo> = HashMap::new();
    let mut counts: HashMap<PackageKey, Vec<String>> = HashMap::new();
    for entry in packages {
        // family included: two packages sharing a name are different
        // packages, not two versions of one
        let key = (
            entry.repo_name.clone(),
            entry.pkg.pkg_name.clone(),
            entry.pkg.pkg_id.clone(),
            entry.pkg.pkg_family.clone(),
        );
        counts
            .entry(key.clone())
            .or_default()
            .push(entry.pkg.version.clone());
        match newest.get(&key) {
            Some(kept) if compare_versions(&kept.pkg.version, &entry.pkg.version).is_ge() => {}
            _ => {
                newest.insert(key, entry);
            }
        }
    }
    let mut packages: Vec<ListingWithRepo> = newest.into_values().collect();
    packages.sort_by(|a, b| {
        a.pkg
            .pkg_name
            .cmp(&b.pkg.pkg_name)
            .then(a.repo_name.cmp(&b.repo_name))
    });

    let installed_pkgs: InstalledIndex = diesel_db
        .with_conn(|conn| {
            CoreRepository::list_filtered(conn, None, None, None, None, None, None, None, None)
        })?
        .into_par_iter()
        // Keyed by name, not id: a package installed before ids became
        // optional still carries one, while its metadata no longer does, and
        // keying on both would stop matching the two. Rows sharing a key are
        // merged rather than overwritten, so one uninstalled version cannot
        // mask an installed one.
        // Keyed by family too, or a package merely sharing a name would
        // inherit the marker. The family is recorded at install time, so a
        // package without one matches only entries without one.
        .map(|pkg| {
            (
                (pkg.repo_name, pkg.pkg_name),
                (pkg.pkg_family, pkg.is_installed),
            )
        })
        .fold(HashMap::new, |mut acc: HashMap<_, Vec<_>>, (key, value)| {
            acc.entry(key).or_default().push(value);
            acc
        })
        .reduce(HashMap::new, |mut acc: HashMap<_, Vec<_>>, part| {
            for (key, values) in part {
                acc.entry(key).or_default().extend(values);
            }
            acc
        });

    let offered: NameCounts = metadata_mgr
        .query_all(|_repo_name, conn| MetadataRepository::count_names(conn))?
        .into_iter()
        .flat_map(|(repo_name, rows)| {
            rows.into_iter().map(move |(pkg_name, offered)| {
                ((repo_name.clone(), pkg_name), offered.max(0) as usize)
            })
        })
        .collect();

    let total = packages.len();

    let entries: Vec<PackageListEntry> = packages
        .into_iter()
        .map(|entry| {
            let installed = is_installed(
                &installed_pkgs,
                &offered,
                &entry.repo_name,
                &entry.pkg.pkg_name,
                entry.pkg.pkg_family.as_deref(),
            );
            let other_versions = counts
                .get(&(
                    entry.repo_name.clone(),
                    entry.pkg.pkg_name.clone(),
                    entry.pkg.pkg_id.clone(),
                    entry.pkg.pkg_family.clone(),
                ))
                .map(|all| {
                    let mut rest: Vec<String> = all
                        .iter()
                        .filter(|v| **v != entry.pkg.version)
                        .cloned()
                        .collect();
                    rest.sort_by(|a, b| compare_versions(b, a));
                    rest
                })
                .unwrap_or_default();

            // Build a minimal Package for the entry
            let package = Package {
                repo_name: entry.repo_name,
                pkg_name: entry.pkg.pkg_name,
                pkg_family: entry.pkg.pkg_family,
                pkg_type: entry.pkg.pkg_type,
                version: entry.pkg.version,
                ..Default::default()
            };

            PackageListEntry {
                package,
                installed,
                other_versions,
            }
        })
        .collect();

    Ok(PackageListResult {
        packages: entries,
        total,
    })
}

/// List installed packages, optionally filtered by repository.
pub fn list_installed(
    ctx: &SoarContext,
    repo_name: Option<&str>,
) -> SoarResult<InstalledListResult> {
    debug!(repo = ?repo_name, "listing installed packages");
    let diesel_db = ctx.diesel_core_db()?;

    let packages: Vec<InstalledPackage> = diesel_db
        .with_conn(|conn| {
            CoreRepository::list_filtered(conn, repo_name, None, None, None, None, None, None, None)
        })?
        .into_iter()
        .map(Into::into)
        .collect();
    trace!(count = packages.len(), "fetched installed packages");

    let mut total_size = 0u64;
    let total_count = packages.len();

    let entries: Vec<InstalledEntry> = packages
        .into_iter()
        .map(|package| {
            let installed_path = PathBuf::from(&package.installed_path);
            let disk_size = dir_size(&installed_path).unwrap_or(0);
            let is_healthy = package.is_installed && installed_path.exists();
            total_size += disk_size;

            InstalledEntry {
                package,
                disk_size,
                is_healthy,
            }
        })
        .collect();

    Ok(InstalledListResult {
        packages: entries,
        total_count,
        total_size,
    })
}

/// Count distinct installed packages.
pub fn count_installed(ctx: &SoarContext, repo_name: Option<&str>) -> SoarResult<i64> {
    let diesel_db = ctx.diesel_core_db()?;
    diesel_db.with_conn(|conn| CoreRepository::count_distinct_installed(conn, repo_name))
}
