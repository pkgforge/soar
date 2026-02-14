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
use soar_utils::fs::dir_size;
use tracing::{debug, trace};

use crate::{
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

    let installed_pkgs: HashMap<(String, String, String), bool> = diesel_db
        .with_conn(|conn| {
            CoreRepository::list_filtered(conn, None, None, None, None, None, None, None, None)
        })?
        .into_par_iter()
        .map(|pkg| ((pkg.repo_name, pkg.pkg_id, pkg.pkg_name), pkg.is_installed))
        .collect();

    let total = packages.len();

    let entries: Vec<PackageListEntry> = packages
        .into_iter()
        .map(|entry| {
            let key = (
                entry.repo_name.clone(),
                entry.pkg.pkg_id.clone(),
                entry.pkg.pkg_name.clone(),
            );
            let installed = installed_pkgs.get(&key).copied().unwrap_or(false);

            // Build a minimal Package for the entry
            let package = Package {
                repo_name: entry.repo_name,
                pkg_id: entry.pkg.pkg_id,
                pkg_name: entry.pkg.pkg_name,
                pkg_type: entry.pkg.pkg_type,
                version: entry.pkg.version,
                ..Default::default()
            };

            PackageListEntry {
                package,
                installed,
            }
        })
        .collect();

    Ok(PackageListResult {
        packages: entries,
        total,
    })
}

/// List installed packages, optionally filtered by repository.
pub async fn list_installed(
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
