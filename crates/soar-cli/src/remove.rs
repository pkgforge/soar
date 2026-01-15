use soar_core::{
    database::models::InstalledPackage,
    package::{query::PackageQuery, remove::PackageRemover},
    SoarResult,
};
use soar_db::repository::core::{CoreRepository, SortDirection};
use tracing::{debug, error, info, trace, warn};

use crate::{
    state::AppState,
    utils::{confirm_action, get_package_hooks, select_package_interactively, Colored},
};

pub async fn remove_packages(packages: &[String], yes: bool, all: bool) -> SoarResult<()> {
    debug!(
        count = packages.len(),
        all = all,
        "starting package removal"
    );
    let state = AppState::new();
    let diesel_db = state.diesel_core_db()?.clone();

    for package in packages {
        trace!(package = package, "processing package for removal");
        let query = PackageQuery::try_from(package.as_str())?;

        // --all flag: remove all installed variants of the package
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
                error!("Package {} is not installed", name);
                continue;
            }

            for pkg in installed {
                debug!(
                    pkg_name = pkg.pkg_name,
                    pkg_id = pkg.pkg_id,
                    "removing package variant"
                );
                let (hooks, sandbox) = get_package_hooks(&pkg.pkg_name);
                let remover = PackageRemover::new(pkg.clone(), diesel_db.clone())
                    .await
                    .with_hooks(hooks)
                    .with_sandbox(sandbox);
                remover.remove().await?;

                info!(
                    "Removed {}#{}:{} ({})",
                    pkg.pkg_name, pkg.pkg_id, pkg.repo_name, pkg.version
                );
            }
            continue;
        }

        // Remove all installed packages with the pkg_id that provides the package
        if let Some(ref pkg_id) = query.pkg_id {
            if pkg_id == "all" {
                // Find all installed variants of this package
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
                    error!("Package {} is not installed", query.name.as_ref().unwrap());
                    continue;
                }

                // If multiple variants with different pkg_ids, show picker
                let selected_pkg = if installed.len() > 1 {
                    if yes {
                        installed.into_iter().next().unwrap()
                    } else {
                        select_package_interactively(installed, query.name.as_ref().unwrap())?
                            .unwrap()
                    }
                } else {
                    installed.into_iter().next().unwrap()
                };

                let target_pkg_id = selected_pkg.pkg_id.clone();

                // Find all installed packages with this pkg_id
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

                // Show confirmation for bulk remove
                if all_installed.len() > 1 && !yes {
                    use nu_ansi_term::Color::{Blue, Cyan, Green, LightRed};
                    info!(
                        "The following {} packages will be removed:",
                        Colored(Cyan, all_installed.len())
                    );
                    for pkg in &all_installed {
                        info!(
                            "  - {}#{}:{} ({})",
                            Colored(Blue, &pkg.pkg_name),
                            Colored(Cyan, &pkg.pkg_id),
                            Colored(Green, &pkg.repo_name),
                            Colored(LightRed, &pkg.version)
                        );
                    }
                    if !confirm_action("Proceed with removal?")? {
                        info!("Removal cancelled");
                        continue;
                    }
                }

                for pkg in all_installed {
                    debug!(
                        pkg_name = pkg.pkg_name,
                        pkg_id = pkg.pkg_id,
                        "removing package"
                    );
                    let (hooks, sandbox) = get_package_hooks(&pkg.pkg_name);
                    let remover = PackageRemover::new(pkg.clone(), diesel_db.clone())
                        .await
                        .with_hooks(hooks)
                        .with_sandbox(sandbox);
                    remover.remove().await?;

                    info!(
                        "Removed {}#{}:{} ({})",
                        pkg.pkg_name, pkg.pkg_id, pkg.repo_name, pkg.version
                    );
                }
                continue;
            }
        }

        // Normal case - find matching installed packages
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
            warn!("Package {} is not installed.", package);
            continue;
        }

        // If multiple packages match and user didn't specify pkg_id
        let pkgs_to_remove: Vec<InstalledPackage> =
            if installed_pkgs.len() > 1 && query.pkg_id.is_none() {
                if yes {
                    vec![installed_pkgs.into_iter().next().unwrap()]
                } else {
                    let pkg = select_package_interactively(
                        installed_pkgs,
                        query.name.as_ref().unwrap_or(package),
                    )?
                    .unwrap();
                    vec![pkg]
                }
            } else {
                installed_pkgs
                    .into_iter()
                    .filter(|pkg| query.name.is_some() || pkg.with_pkg_id)
                    .collect()
            };

        debug!(count = pkgs_to_remove.len(), "packages to remove");
        for installed_pkg in pkgs_to_remove {
            debug!(
                pkg_name = installed_pkg.pkg_name,
                pkg_id = installed_pkg.pkg_id,
                installed_path = installed_pkg.installed_path,
                "removing package"
            );
            let (hooks, sandbox) = get_package_hooks(&installed_pkg.pkg_name);
            let remover = PackageRemover::new(installed_pkg.clone(), diesel_db.clone())
                .await
                .with_hooks(hooks)
                .with_sandbox(sandbox);
            remover.remove().await?;

            info!(
                "Removed {}#{}:{} ({})",
                installed_pkg.pkg_name,
                installed_pkg.pkg_id,
                installed_pkg.repo_name,
                installed_pkg.version
            );
        }
    }

    debug!("package removal completed");
    Ok(())
}
