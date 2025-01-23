use soar_core::{
    config::get_config,
    database::packages::{FilterCondition, PackageQueryBuilder},
    package::{install::InstallTarget, query::PackageQuery},
    SoarResult,
};
use tracing::info;

use crate::{
    install::{create_install_context, perform_installation},
    state::AppState,
};

pub async fn update_packages(packages: Option<Vec<String>>) -> SoarResult<()> {
    let state = AppState::new().await?;
    let core_db = state.core_db();
    let repo_db = state.repo_db();

    let mut update_targets = Vec::new();

    if let Some(packages) = packages {
        for package in packages {
            let query = PackageQuery::try_from(package.as_str())?;
            let builder = PackageQueryBuilder::new(core_db.clone());
            let builder = query.apply_filters(builder.clone()).limit(1);
            let installed_pkgs = builder.load_installed()?.items;

            for pkg in installed_pkgs {
                let updated = builder
                    .clone()
                    .database(repo_db.clone())
                    .where_and("version", FilterCondition::Gt(pkg.version.clone()))
                    .load()?
                    .items;

                if updated.len() > 0 {
                    let with_pkg_id = pkg.with_pkg_id;
                    update_targets.push(InstallTarget {
                        package: updated.first().unwrap().clone(),
                        existing_install: Some(pkg),
                        with_pkg_id,
                    })
                }
            }
        }
    } else {
        let installed_packages = PackageQueryBuilder::new(core_db.clone())
            .where_and("pinned", FilterCondition::Eq(false.to_string()))
            .load_installed()?
            .items;

        for pkg in installed_packages {
            let updated = PackageQueryBuilder::new(repo_db.clone())
                .where_and("repo_name", FilterCondition::Eq(pkg.repo_name.clone()))
                .where_and("pkg_name", FilterCondition::Eq(pkg.pkg_name.clone()))
                .where_and("pkg_id", FilterCondition::Eq(pkg.pkg_id.clone()))
                .where_and("version", FilterCondition::Gt(pkg.version.clone()))
                .load()?
                .items;

            if updated.len() > 0 {
                let with_pkg_id = pkg.with_pkg_id;
                update_targets.push(InstallTarget {
                    package: updated.first().unwrap().clone(),
                    existing_install: Some(pkg),
                    with_pkg_id,
                })
            }
        }
    }

    if update_targets.is_empty() {
        info!("No packages to update.");
        return Ok(());
    }

    let ctx = create_install_context(
        update_targets.len(),
        get_config().parallel_limit.unwrap_or(1) as usize,
        None,
        None,
        None,
    );

    perform_installation(ctx, update_targets, core_db.clone()).await?;

    Ok(())
}
