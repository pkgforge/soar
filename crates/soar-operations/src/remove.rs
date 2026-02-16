use soar_core::{
    database::models::InstalledPackage,
    package::{query::PackageQuery, remove::PackageRemover},
    SoarResult,
};
use soar_db::repository::core::{CoreRepository, SortDirection};
use soar_events::{RemoveStage, SoarEvent};
use tracing::{debug, trace};

use crate::{
    progress::next_op_id, utils::get_package_hooks, FailedInfo, RemoveReport, RemoveResolveResult,
    RemovedInfo, SoarContext,
};

/// Resolve package queries into packages to remove.
///
/// For each query, returns a [`RemoveResolveResult`] indicating whether the
/// package was found, is ambiguous, or not installed.
pub fn resolve_removals(
    ctx: &SoarContext,
    packages: &[String],
    all: bool,
) -> SoarResult<Vec<RemoveResolveResult>> {
    debug!(
        count = packages.len(),
        all = all,
        "resolving packages for removal"
    );
    let diesel_db = ctx.diesel_core_db()?;

    let mut results = Vec::with_capacity(packages.len());

    for package in packages {
        let query = PackageQuery::try_from(package.as_str())?;

        // --all flag: remove all installed variants matching the name
        if let (true, None, Some(ref name)) = (all, &query.pkg_id, &query.name) {
            let installed: Vec<InstalledPackage> = diesel_db
                .with_conn(|conn| {
                    CoreRepository::list_filtered(
                        conn,
                        query.repo_name.as_deref(),
                        query.name.as_deref(),
                        None,
                        query.version.as_deref(),
                        None,
                        None,
                        None,
                        Some(SortDirection::Asc),
                    )
                })?
                .into_iter()
                .map(Into::into)
                .collect();

            if installed.is_empty() {
                results.push(RemoveResolveResult::NotInstalled(name.clone()));
            } else {
                results.push(RemoveResolveResult::Resolved(installed));
            }
            continue;
        }

        // Handle #all: remove all packages with the selected pkg_id
        if let Some(ref pkg_id) = query.pkg_id {
            if pkg_id == "all" {
                let installed: Vec<InstalledPackage> = diesel_db
                    .with_conn(|conn| {
                        CoreRepository::list_filtered(
                            conn,
                            query.repo_name.as_deref(),
                            query.name.as_deref(),
                            None,
                            None,
                            None,
                            None,
                            None,
                            Some(SortDirection::Asc),
                        )
                    })?
                    .into_iter()
                    .map(Into::into)
                    .collect();

                if installed.is_empty() {
                    results.push(RemoveResolveResult::NotInstalled(
                        query.name.clone().unwrap_or_default(),
                    ));
                } else if installed.len() > 1 {
                    // Multiple pkg_ids → ambiguous, caller picks which pkg_id
                    results.push(RemoveResolveResult::Ambiguous {
                        query: query.name.clone().unwrap_or_default(),
                        candidates: installed,
                    });
                } else {
                    let target_pkg_id = installed[0].pkg_id.clone();
                    // Find all packages with this pkg_id
                    let all_installed: Vec<InstalledPackage> = diesel_db
                        .with_conn(|conn| {
                            CoreRepository::list_filtered(
                                conn,
                                query.repo_name.as_deref(),
                                None,
                                Some(&target_pkg_id),
                                None,
                                None,
                                None,
                                None,
                                Some(SortDirection::Asc),
                            )
                        })?
                        .into_iter()
                        .map(Into::into)
                        .collect();

                    results.push(RemoveResolveResult::Resolved(all_installed));
                }
                continue;
            }
        }

        // Normal case: find matching installed packages
        let installed_pkgs: Vec<InstalledPackage> = diesel_db
            .with_conn(|conn| {
                CoreRepository::list_filtered(
                    conn,
                    query.repo_name.as_deref(),
                    query.name.as_deref(),
                    query.pkg_id.as_deref(),
                    query.version.as_deref(),
                    None,
                    None,
                    None,
                    Some(SortDirection::Asc),
                )
            })?
            .into_iter()
            .map(Into::into)
            .collect();

        if installed_pkgs.is_empty() {
            results.push(RemoveResolveResult::NotInstalled(package.clone()));
        } else if installed_pkgs.len() > 1 && query.pkg_id.is_none() {
            results.push(RemoveResolveResult::Ambiguous {
                query: query.name.clone().unwrap_or(package.clone()),
                candidates: installed_pkgs,
            });
        } else {
            results.push(RemoveResolveResult::Resolved(installed_pkgs));
        }
    }

    Ok(results)
}

/// Remove installed packages. Emits events through the context's event sink.
pub async fn perform_removal(
    ctx: &SoarContext,
    packages: Vec<InstalledPackage>,
) -> SoarResult<RemoveReport> {
    debug!(count = packages.len(), "performing removal");
    let diesel_db = ctx.diesel_core_db()?.clone();

    let mut removed = Vec::new();
    let mut failed = Vec::new();

    for pkg in packages {
        let op_id = next_op_id();

        ctx.events().emit(SoarEvent::Removing {
            op_id,
            pkg_name: pkg.pkg_name.clone(),
            pkg_id: pkg.pkg_id.clone(),
            stage: RemoveStage::RunningHook("pre_remove".into()),
        });

        trace!(
            pkg_name = pkg.pkg_name,
            pkg_id = pkg.pkg_id,
            "removing package"
        );

        let (hooks, sandbox) = get_package_hooks(&pkg.pkg_name);
        let remover = PackageRemover::new(pkg.clone(), diesel_db.clone(), ctx.config().clone())
            .await
            .with_hooks(hooks)
            .with_sandbox(sandbox);

        match remover.remove().await {
            Ok(()) => {
                ctx.events().emit(SoarEvent::Removing {
                    op_id,
                    pkg_name: pkg.pkg_name.clone(),
                    pkg_id: pkg.pkg_id.clone(),
                    stage: RemoveStage::Complete {
                        size_freed: None,
                    },
                });
                ctx.events().emit(SoarEvent::OperationComplete {
                    op_id,
                    pkg_name: pkg.pkg_name.clone(),
                    pkg_id: pkg.pkg_id.clone(),
                });

                removed.push(RemovedInfo {
                    pkg_name: pkg.pkg_name,
                    pkg_id: pkg.pkg_id,
                    repo_name: pkg.repo_name,
                    version: pkg.version,
                });
            }
            Err(err) => {
                ctx.events().emit(SoarEvent::OperationFailed {
                    op_id,
                    pkg_name: pkg.pkg_name.clone(),
                    pkg_id: pkg.pkg_id.clone(),
                    error: err.to_string(),
                });

                failed.push(FailedInfo {
                    pkg_name: pkg.pkg_name,
                    pkg_id: pkg.pkg_id,
                    error: err.to_string(),
                });
            }
        }
    }

    Ok(RemoveReport {
        removed,
        failed,
    })
}
