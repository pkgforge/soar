use soar_core::{
    database::packages::{get_installed_packages, QueryOptions},
    package::{query::PackageQuery, remove::PackageRemover},
    SoarResult,
};
use tracing::{info, warn};

use crate::state::AppState;

pub async fn remove_packages(packages: &[String]) -> SoarResult<()> {
    let state = AppState::new().await?;

    for package in packages {
        let core_db = state.core_db().clone();

        let query = PackageQuery::try_from(package.as_str())?;
        let filters = query.create_filter();
        let options = QueryOptions {
            filters,
            ..Default::default()
        };

        let installed_pkgs = get_installed_packages(core_db.clone(), options)?.items;

        if installed_pkgs.is_empty() {
            warn!("Package {} is not installed.", package);
            continue;
        }

        let installed_pkg = installed_pkgs.first().unwrap();
        if !installed_pkg.is_installed {
            warn!("Package {} is not installed.", package);
            continue;
        }

        let remover = PackageRemover::new(installed_pkg.clone(), core_db).await;
        remover.remove().await?;

        info!("Removed {}", installed_pkg.pkg_name);
    }

    Ok(())
}
