use std::collections::HashMap;

use nucleo_matcher::{
    pattern::{CaseMatching, Normalization, Pattern},
    Config, Matcher, Utf32String,
};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use soar_config::config::get_config;
use soar_core::{database::models::Package, package::query::PackageQuery, SoarResult};
use soar_db::{
    models::metadata::FuzzyCandidate,
    repository::{
        core::{CoreRepository, SortDirection},
        metadata::MetadataRepository,
    },
};
use tracing::{debug, trace};

use crate::{SearchEntry, SearchResult, SoarContext};

/// Search for packages across all repositories.
///
/// Uses fuzzy matching by default. Falls back to SQL LIKE for case-sensitive searches.
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
    let search_limit = limit.or(get_config().search_limit).unwrap_or(20);

    let packages: Vec<Package> = if case_sensitive {
        let sql_limit = search_limit as i64;
        metadata_mgr.query_all_flat(|repo_name, conn| {
            let pkgs = MetadataRepository::search_case_sensitive(conn, query, Some(sql_limit))?;
            Ok(pkgs
                .into_iter()
                .map(|p| {
                    let mut pkg: Package = p.into();
                    pkg.repo_name = repo_name.to_string();
                    pkg
                })
                .collect())
        })?
    } else {
        fuzzy_search(ctx, query, search_limit).await?
    };

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
        .take(search_limit)
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

/// Returns top fuzzy-matched packages across all repositories.
async fn fuzzy_search(ctx: &SoarContext, query: &str, limit: usize) -> SoarResult<Vec<Package>> {
    let metadata_mgr = ctx.metadata_manager().await?;

    let candidates: Vec<(String, FuzzyCandidate)> =
        metadata_mgr.query_all_flat(|repo_name, conn| {
            let items = MetadataRepository::load_fuzzy_candidates(conn)?;
            Ok(items
                .into_iter()
                .map(|c| (repo_name.to_string(), c))
                .collect())
        })?;

    let scored = score_candidates(query, &candidates);
    let top: Vec<_> = scored.into_iter().take(limit).collect();

    let mut repo_ids: HashMap<&str, Vec<i32>> = HashMap::new();
    for &(_, idx) in &top {
        let (repo_name, candidate) = &candidates[idx];
        repo_ids
            .entry(repo_name.as_str())
            .or_default()
            .push(candidate.id);
    }

    let mut full_packages: HashMap<(String, i32), Package> = HashMap::new();
    for (repo_name, ids) in &repo_ids {
        if let Some(pkgs) =
            metadata_mgr.query_repo(repo_name, |conn| MetadataRepository::find_by_ids(conn, ids))?
        {
            for p in pkgs {
                let db_id = p.id;
                let mut pkg: Package = p.into();
                pkg.repo_name = repo_name.to_string();
                full_packages.insert((repo_name.to_string(), db_id), pkg);
            }
        }
    }

    let packages: Vec<Package> = top
        .into_iter()
        .filter_map(|(_, idx)| {
            let (repo_name, candidate) = &candidates[idx];
            full_packages.remove(&(repo_name.clone(), candidate.id))
        })
        .collect();

    Ok(packages)
}

/// Suggest similar package names for "did you mean?" messages.
pub async fn suggest_similar(
    ctx: &SoarContext,
    query: &str,
    max: usize,
) -> SoarResult<Vec<String>> {
    let metadata_mgr = ctx.metadata_manager().await?;

    let candidates: Vec<(String, FuzzyCandidate)> =
        metadata_mgr.query_all_flat(|repo_name, conn| {
            let items = MetadataRepository::load_fuzzy_candidates(conn)?;
            Ok(items
                .into_iter()
                .map(|c| (repo_name.to_string(), c))
                .collect())
        })?;

    let scored = score_candidates(query, &candidates);

    let suggestions: Vec<String> = scored
        .into_iter()
        .take(max)
        .map(|(_, idx)| {
            let (_, candidate) = &candidates[idx];
            candidate.pkg_name.clone()
        })
        .collect();

    Ok(suggestions)
}

fn score_candidates(query: &str, candidates: &[(String, FuzzyCandidate)]) -> Vec<(u32, usize)> {
    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);

    let mut scored: Vec<(u32, usize)> = Vec::new();

    for (idx, (_repo_name, candidate)) in candidates.iter().enumerate() {
        let name_buf = Utf32String::from(candidate.pkg_name.as_str());
        let name_score = pattern.score(name_buf.slice(..), &mut matcher);

        let id_buf = Utf32String::from(candidate.pkg_id.as_str());
        let id_score = pattern.score(id_buf.slice(..), &mut matcher);

        let best_score = [name_score, id_score].into_iter().flatten().max();

        if let Some(score) = best_score {
            scored.push((score, idx));
        }
    }

    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored
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
