use soar_core::{
    database::packages::{get_installed_packages, get_packages, FilterOp, QueryOptions},
    package::{query::PackageQuery, remove::PackageRemover},
    SoarResult,
};
use tracing::{error, info, warn};

use crate::{state::AppState, utils::select_package_interactively};

pub async fn remove_packages(packages: &[String]) -> SoarResult<()> {
    let state = AppState::new().await?;
    let db = state.repo_db();

    for package in packages {
        let core_db = state.core_db().clone();

        let mut query = PackageQuery::try_from(package.as_str())?;
        let mut filters = query.create_filter();

        if let Some(ref pkg_id) = query.pkg_id {
            if pkg_id == "all" {
                let options = QueryOptions {
                    filters: filters.clone(),
                    ..Default::default()
                };
                let pkg = get_packages(db.clone(), options)?;
                if pkg.total == 0 {
                    error!("Package {} not found", query.name.unwrap());
                    continue;
                }
                let pkg = if pkg.total > 1 {
                    let pkgs = pkg.items.clone();
                    select_package_interactively(pkgs, &query.name.unwrap())?.unwrap()
                } else {
                    pkg.items.first().unwrap().clone()
                };
                query.pkg_id = Some(pkg.pkg_id.clone());
                query.name = None;

                filters.insert(
                    "pkg_id".to_string(),
                    (FilterOp::Eq, pkg.pkg_id.into()).into(),
                );
                filters.remove("pkg_name");
            }
        }

        let options = QueryOptions {
            filters,
            ..Default::default()
        };

        let installed_pkgs = get_installed_packages(core_db.clone(), options)?.items;

        if installed_pkgs.is_empty() {
            warn!("Package {} is not installed.", package);
            continue;
        }

        for installed_pkg in installed_pkgs {
            if !installed_pkg.is_installed {
                warn!("Package {} is not installed.", package);
                continue;
            }

            if query.name.is_none() && !installed_pkg.with_pkg_id {
                continue;
            }

            let remover = PackageRemover::new(installed_pkg.clone(), core_db.clone()).await;
            remover.remove().await?;

            info!("Removed {}", installed_pkg.pkg_name);
        }
    }

    Ok(())
}
