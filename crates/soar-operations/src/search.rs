use std::collections::{HashMap, HashSet};

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
use soar_utils::version::compare_versions;
use tracing::{debug, trace};

use crate::{
    utils::{is_installed, InstalledIndex, PackageKey},
    SearchEntry, SearchResult, SoarContext,
};

/// Search for packages across all repositories.
///
/// Uses fuzzy matching by default. Falls back to SQL LIKE for case-sensitive
/// searches.
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

    // One row per package: a result repeated once per published version says
    // nothing extra and pushes real matches off the list.
    let mut newest: HashMap<PackageKey, Package> = HashMap::new();
    let mut counts: HashMap<PackageKey, Vec<String>> = HashMap::new();
    let mut order: Vec<PackageKey> = Vec::new();
    for pkg in packages {
        let key = (
            pkg.repo_name.clone(),
            pkg.pkg_name.clone(),
            pkg.pkg_id.clone(),
            pkg.pkg_family.clone(),
        );
        counts
            .entry(key.clone())
            .or_default()
            .push(pkg.version.clone());
        match newest.get(&key) {
            Some(kept) if compare_versions(&kept.version, &pkg.version).is_ge() => {}
            _ => {
                if !newest.contains_key(&key) {
                    order.push(key.clone());
                }
                newest.insert(key, pkg);
            }
        }
    }
    // ranking order is the point of a search, so it is preserved
    let packages: Vec<Package> = order
        .into_iter()
        .filter_map(|k| newest.remove(&k))
        .collect();

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

    let total_count = packages.len();

    let entries: Vec<SearchEntry> = packages
        .into_iter()
        .take(search_limit)
        .map(|package| {
            let installed = is_installed(
                &installed_pkgs,
                &package.repo_name,
                &package.pkg_name,
                package.pkg_family.as_deref(),
            );
            let other_versions = counts
                .get(&(
                    package.repo_name.clone(),
                    package.pkg_name.clone(),
                    package.pkg_id.clone(),
                    package.pkg_family.clone(),
                ))
                .map(|all| {
                    let mut rest: Vec<String> = all
                        .iter()
                        .filter(|v| **v != package.version)
                        .cloned()
                        .collect();
                    rest.sort_by(|a, b| compare_versions(b, a));
                    rest
                })
                .unwrap_or_default();
            SearchEntry {
                package,
                installed,
                other_versions,
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

    // `scored` is sorted by score descending, so the first time we see a
    // package name is its best-scoring occurrence. Dedup by name to avoid
    // showing the same package once per repo that provides it.
    let mut seen = HashSet::new();
    let suggestions: Vec<String> = scored
        .into_iter()
        .filter_map(|(_, idx)| {
            let (_, candidate) = &candidates[idx];
            seen.insert(candidate.pkg_name.clone())
                .then(|| candidate.pkg_name.clone())
        })
        .take(max)
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

        // A package without an id simply has nothing extra to match on.
        let id_score = candidate.pkg_id.as_deref().and_then(|id| {
            let id_buf = Utf32String::from(id);
            pattern.score(id_buf.slice(..), &mut matcher)
        });

        let best_score = [name_score, id_score].into_iter().flatten().max();

        if let Some(score) = best_score {
            scored.push((score, idx));
        }
    }

    scored.sort_by_key(|s| std::cmp::Reverse(s.0));
    scored
}

/// Query detailed package information.
///
/// Accepts query strings in the format `family/name@version:repo`.
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
                    query.family.as_deref(),
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
                query.family.as_deref(),
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

    let mut packages: Vec<Package> = if let Some(ref version) = query.version {
        packages
            .into_iter()
            .filter(|p| p.has_version(version))
            .map(|p| p.resolve(query.version.as_deref()))
            .collect()
    } else {
        packages
    };

    // Maintainers live in their own table, so they take a second query.
    for package in &mut packages {
        let found = metadata_mgr.query_repo(&package.repo_name, |conn| {
            MetadataRepository::get_maintainers(conn, package.id as i32)
        });

        if let Ok(Some(maintainers)) = found {
            let named: Vec<_> = maintainers
                .into_iter()
                .map(|m| {
                    soar_core::database::models::Maintainer {
                        name: m.name,
                        contact: m.contact,
                    }
                })
                .collect();

            if !named.is_empty() {
                package.maintainers = Some(named);
            }
        }
    }

    // The query is ordered by name, which says nothing about several versions
    // of one package. Newest first, so the one that would be installed is on
    // top.
    packages.sort_by(|a, b| {
        a.pkg_name
            .cmp(&b.pkg_name)
            .then(a.repo_name.cmp(&b.repo_name))
            .then(compare_versions(&b.version, &a.version))
    });

    Ok(packages)
}
