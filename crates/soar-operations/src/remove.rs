use soar_core::{
    database::models::InstalledPackage,
    error::SoarError,
    package::{local::LocalPackage, query::PackageQuery, remove::PackageRemover, url::UrlPackage},
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
        // A package installed from a URL is named by that URL as readily as by
        // the name derived from it, and the URL is what the caller has.
        if UrlPackage::is_remote(package) || LocalPackage::is_local(package) {
            let mut installed: Vec<InstalledPackage> = diesel_db
                .with_conn(|conn| CoreRepository::find_by_download_url(conn, package))?
                .into_iter()
                .map(Into::into)
                .filter(|ip: &InstalledPackage| ip.is_installed)
                .collect();

            // An update replaces the recorded source with the one it fetched,
            // so the URL a package was installed from stops matching it. The
            // URL still names the package it produced, which is what the
            // caller means by it.
            if installed.is_empty() {
                if let Some((name, family)) = package_from_source(package) {
                    installed = diesel_db
                        .with_conn(|conn| {
                            CoreRepository::list_filtered(
                                conn,
                                Some("local"),
                                Some(&name),
                                None,
                                None,
                                Some(true),
                                None,
                                None,
                                Some(SortDirection::Asc),
                            )
                        })?
                        .into_iter()
                        .map(Into::into)
                        .filter(|ip: &InstalledPackage| {
                            ip.pkg_family.as_deref() == family.as_deref()
                        })
                        .collect();
                }
            }

            if installed.is_empty() {
                results.push(RemoveResolveResult::NotInstalled(package.clone()));
            } else {
                results.push(RemoveResolveResult::Resolved(installed));
            }
            continue;
        }

        let query = PackageQuery::try_from(package.as_str())?;

        // --all flag: remove all installed variants matching the name
        if let (true, None, Some(ref name)) = (all, query.pkg_id.as_deref(), &query.name) {
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

        // `#all` selected every variant, then removed only the one chosen, so
        // it did nothing a bare name did not. `--all` is what removes them all.
        if query.pkg_id.as_deref() == Some("all") {
            return Err(SoarError::InvalidPackageQuery(
                "'#all' is not supported when removing; use --all".into(),
            ));
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

/// The package name and family a URL or local path installs as.
fn package_from_source(source: &str) -> Option<(String, Option<String>)> {
    let package = if LocalPackage::is_local(source) {
        LocalPackage::from_path(source, None, None, None, None)
            .ok()?
            .to_package()
    } else {
        UrlPackage::from_remote(source, None, None, None, None)
            .ok()?
            .to_package()
    };
    Some((package.pkg_name, package.pkg_family))
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
                    stage: RemoveStage::Complete {
                        size_freed: None,
                    },
                });
                ctx.events().emit(SoarEvent::OperationComplete {
                    op_id,
                    pkg_name: pkg.pkg_name.clone(),
                });

                removed.push(RemovedInfo {
                    pkg_name: pkg.pkg_name,
                    repo_name: pkg.repo_name,
                    version: pkg.version,
                });
            }
            Err(err) => {
                ctx.events().emit(SoarEvent::OperationFailed {
                    op_id,
                    pkg_name: pkg.pkg_name.clone(),
                    error: err.to_string(),
                });

                failed.push(FailedInfo {
                    pkg_name: pkg.pkg_name,
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
