use std::collections::HashMap;

use rayon::iter::{IntoParallelIterator, ParallelIterator};
use soar_config::config::get_config;
use soar_core::{database::models::Package, package::query::PackageQuery, SoarResult};
use soar_db::repository::{
    core::{CoreRepository, SortDirection},
    metadata::MetadataRepository,
};
use tracing::{debug, trace};

use crate::{SearchEntry, SearchResult, SoarContext};

/// Search for packages across all repositories.
pub async fn search_packages(
    ctx: &SoarContext,
    query: &str,
    case_sensitive: bool,
    limit: Option<usize>,
) -> SoarResult<SearchResult> {
    debug!(
        query = query,
        case_sensitive = case_sensitive,
        limit = ?limit,
        "searching packages"
    );
    let metadata_mgr = ctx.metadata_manager().await?;
    let diesel_db = ctx.diesel_core_db()?;

    let search_limit = limit.or(get_config().search_limit).unwrap_or(20) as i64;
    trace!(search_limit = search_limit, "using search limit");

    let packages: Vec<Package> = metadata_mgr.query_all_flat(|repo_name, conn| {
        let pkgs = if case_sensitive {
            MetadataRepository::search_case_sensitive(conn, query, Some(search_limit))?
        } else {
            MetadataRepository::search(conn, query, Some(search_limit))?
        };
        Ok(pkgs
            .into_iter()
            .map(|p| {
                let mut pkg: Package = p.into();
                pkg.repo_name = repo_name.to_string();
                pkg
            })
            .collect())
    })?;

    let installed_pkgs: HashMap<(String, String, String), bool> = diesel_db
        .with_conn(|conn| {
            CoreRepository::list_filtered(conn, None, None, None, None, None, None, None, None)
        })?
        .into_par_iter()
        .map(|pkg| ((pkg.repo_name, pkg.pkg_id, pkg.pkg_name), pkg.is_installed))
        .collect();

    let total_count = packages.len();

    let entries: Vec<SearchEntry> = packages
        .into_iter()
        .take(search_limit as usize)
        .map(|package| {
            let key = (
                package.repo_name.clone(),
                package.pkg_id.clone(),
                package.pkg_name.clone(),
            );
            let installed = installed_pkgs.get(&key).copied().unwrap_or(false);

            SearchEntry {
                package,
                installed,
            }
        })
        .collect();

    Ok(SearchResult {
        packages: entries,
        total_count,
    })
}

/// Query detailed package information.
///
/// Accepts query strings in the format `name#pkg_id@version:repo`.
/// Returns all matching packages with full metadata.
pub async fn query_package(ctx: &SoarContext, query_str: &str) -> SoarResult<Vec<Package>> {
    debug!(query = query_str, "querying package info");
    let metadata_mgr = ctx.metadata_manager().await?;

    let query = PackageQuery::try_from(query_str)?;
    trace!(
        name = ?query.name,
        pkg_id = ?query.pkg_id,
        version = ?query.version,
        repo = ?query.repo_name,
        "parsed query"
    );

    let packages: Vec<Package> = if let Some(ref repo_name) = query.repo_name {
        metadata_mgr
            .query_repo(repo_name, |conn| {
                MetadataRepository::find_filtered(
                    conn,
                    query.name.as_deref(),
                    query.pkg_id.as_deref(),
                    None,
                    None,
                    Some(SortDirection::Asc),
                )
            })?
            .unwrap_or_default()
            .into_iter()
            .map(|p| {
                let mut pkg: Package = p.into();
                pkg.repo_name = repo_name.clone();
                pkg
            })
            .collect()
    } else {
        metadata_mgr.query_all_flat(|repo_name, conn| {
            let pkgs = MetadataRepository::find_filtered(
                conn,
                query.name.as_deref(),
                query.pkg_id.as_deref(),
                None,
                None,
                Some(SortDirection::Asc),
            )?;
            Ok(pkgs
                .into_iter()
                .map(|p| {
                    let mut pkg: Package = p.into();
                    pkg.repo_name = repo_name.to_string();
                    pkg
                })
                .collect())
        })?
    };

    let packages: Vec<Package> = if let Some(ref version) = query.version {
        packages
            .into_iter()
            .filter(|p| p.has_version(version))
            .map(|p| p.resolve(query.version.as_deref()))
            .collect()
    } else {
        packages
    };

    Ok(packages)
}
