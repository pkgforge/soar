use soar_core::{
    database::{
        models::InstalledPackage,
        packages::{get_installed_packages_with_filter, PackageFilter},
    },
    package::{query::PackageQuery, remove::PackageRemover},
    SoarResult,
};
use tracing::{info, warn};

use crate::state::AppState;

pub async fn remove_packages(packages: &[String], exact: bool) -> SoarResult<()> {
    let state = AppState::new().await?;
    let core_db = state.core_db().clone();

    for package in packages {
        let repo_db = state.repo_db().clone();

        let query = PackageQuery::try_from(package.as_str())?;
        let filter = PackageFilter::from_query(query);

        let installed_pkgs: Vec<InstalledPackage> =
            get_installed_packages_with_filter(core_db.clone(), 128, filter.clone())?
                .into_iter()
                .filter_map(Result::ok)
                .collect();

        let installed_pkg = installed_pkgs.first().unwrap();
        if !installed_pkg.is_installed {
            warn!("Package {} is not installed.", package);
            continue;
        }

        let remover = PackageRemover::new(installed_pkg.clone(), repo_db).await;
        remover.remove().await?;

        info!("Removed {}", installed_pkg.pkg_name);
    }

    Ok(())
}
