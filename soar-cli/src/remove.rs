use soar_core::{
    database::packages::PackageQueryBuilder,
    package::{query::PackageQuery, remove::PackageRemover},
    SoarResult,
};
use tracing::{error, info, warn};

use crate::{state::AppState, utils::select_package_interactively};

pub async fn remove_packages(packages: &[String]) -> SoarResult<()> {
    let state = AppState::new();

    for package in packages {
        let core_db = state.core_db()?;

        let mut query = PackageQuery::try_from(package.as_str())?;
        let builder = PackageQueryBuilder::new(core_db.clone());

        if let Some(ref pkg_id) = query.pkg_id {
            if pkg_id == "all" {
                let builder = query.apply_filters(builder.clone());
                let packages = builder.load_installed()?;

                if packages.total == 0 {
                    error!("Package {} is not installed", query.name.unwrap());
                    continue;
                }
                let pkg = if packages.total > 1 {
                    let pkgs = packages.items.clone();
                    select_package_interactively(pkgs, &query.name.unwrap())?.unwrap()
                } else {
                    packages.items.first().unwrap().clone()
                };
                query.pkg_id = Some(pkg.pkg_id.clone());
                query.name = None;
            }
        }

        let builder = query.apply_filters(builder);
        let installed_pkgs = builder
            .clone()
            .database(core_db.clone())
            .load_installed()?
            .items;

        if installed_pkgs.is_empty() {
            warn!("Package {} is not installed.", package);
            continue;
        }

        for installed_pkg in installed_pkgs {
            if query.name.is_none() && !installed_pkg.with_pkg_id {
                continue;
            }

            let remover = PackageRemover::new(installed_pkg.clone(), core_db.clone()).await;
            remover.remove().await?;

            info!(
                "Removed {}#{}",
                installed_pkg.pkg_name, installed_pkg.pkg_id
            );
        }
    }

    Ok(())
}
