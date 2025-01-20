use std::collections::HashMap;

use soar_core::{
    config::get_config,
    database::packages::{get_installed_packages, get_packages, FilterOp, QueryOptions},
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
            let filters = query.create_filter();
            let options = QueryOptions {
                filters: filters.clone(),
                limit: 1,
                ..Default::default()
            };
            let installed_pkgs = get_installed_packages(core_db.clone(), options)?.items;

            for pkg in installed_pkgs {
                let mut filters = filters.clone();
                filters.insert(
                    "version".to_string(),
                    (FilterOp::Gt, pkg.version.clone().into()).into(),
                );
                let options = QueryOptions {
                    filters,
                    ..Default::default()
                };
                let updated = get_packages(repo_db.clone(), options)?.items;
                if updated.len() > 0 {
                    update_targets.push(InstallTarget {
                        package: updated.first().unwrap().clone(),
                        existing_install: Some(pkg),
                    })
                }
            }
        }
    } else {
        let mut filters = HashMap::new();
        filters.insert("pinned".to_string(), (FilterOp::Eq, false.into()).into());
        let options = QueryOptions {
            filters: filters.clone(),
            ..Default::default()
        };
        let installed_pkgs = get_installed_packages(core_db.clone(), options)?.items;
        for pkg in installed_pkgs {
            let mut filters = HashMap::new();

            filters.insert(
                "repo_name".to_string(),
                (FilterOp::Eq, pkg.repo_name.clone().into()).into(),
            );
            filters.insert(
                "pkg_name".to_string(),
                (FilterOp::Eq, pkg.pkg_name.clone().into()).into(),
            );
            filters.insert(
                "pkg_id".to_string(),
                (FilterOp::Eq, pkg.pkg_id.clone().into()).into(),
            );
            filters.insert(
                "version".to_string(),
                (FilterOp::Gt, pkg.version.clone().into()).into(),
            );
            let options = QueryOptions {
                filters,
                ..Default::default()
            };
            let updated = get_packages(repo_db.clone(), options)?.items;
            if updated.len() > 0 {
                update_targets.push(InstallTarget {
                    package: updated.first().unwrap().clone(),
                    existing_install: Some(pkg),
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
