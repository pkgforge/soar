use soar_core::{
    database::models::InstalledPackage,
    package::{query::PackageQuery, remove::PackageRemover},
    SoarResult,
};
use soar_db::repository::core::{CoreRepository, SortDirection};
use tracing::{error, info, warn};

use crate::{state::AppState, utils::select_package_interactively};

pub async fn remove_packages(packages: &[String]) -> SoarResult<()> {
    let state = AppState::new();
    let diesel_db = state.diesel_core_db()?.clone();

    for package in packages {
        let mut query = PackageQuery::try_from(package.as_str())?;

        if let Some(ref pkg_id) = query.pkg_id {
            if pkg_id == "all" {
                let installed: Vec<InstalledPackage> = diesel_db
                    .with_conn(|conn| {
                        CoreRepository::list_filtered(
                            conn,
                            query.repo_name.as_deref(),
                            query.name.as_deref(),
                            None, // no pkg_id filter for "all"
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
                    error!("Package {} is not installed", query.name.as_ref().unwrap());
                    continue;
                }

                let pkg = if installed.len() > 1 {
                    select_package_interactively(installed, query.name.as_ref().unwrap())?.unwrap()
                } else {
                    installed.into_iter().next().unwrap()
                };
                query.pkg_id = Some(pkg.pkg_id.clone());
                query.name = None;
            }
        }

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

        for installed_pkg in installed_pkgs {
            if query.name.is_none() && !installed_pkg.with_pkg_id {
                continue;
            }

            let remover = PackageRemover::new(installed_pkg.clone(), diesel_db.clone()).await;
            remover.remove().await?;

            info!(
                "Removed {}#{}",
                installed_pkg.pkg_name, installed_pkg.pkg_id
            );
        }
    }

    Ok(())
}
