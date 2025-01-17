use soar_core::{
    config::get_config,
    database::{
        models::{InstalledPackage, Package},
        packages::{
            get_all_packages, get_installed_packages, get_installed_packages_with_filter,
            PackageFilter,
        },
    },
    package::{install::InstallTarget, query::PackageQuery},
    SoarResult,
};

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
            let filter = PackageFilter::from_query(query);
            let installed_pkgs = get_installed_packages_with_filter(core_db.clone(), 1024, filter)?;

            for pkg in installed_pkgs {
                if let Ok(pkg) = pkg {
                    let filter = PackageFilter {
                        pkg_name: Some(pkg.pkg_name.clone()),
                        repo_name: Some(pkg.repo_name.clone()),
                        ..Default::default()
                    };

                    if let Some(Ok(available_pkg)) = get_all_packages(repo_db.clone(), 1024)?
                        .into_iter()
                        .find(|p| matches_filter(p.as_ref().ok(), &filter))
                    {
                        if needs_update(&pkg, &available_pkg) {
                            update_targets.push(InstallTarget {
                                package: available_pkg,
                                existing_install: Some(pkg),
                            });
                        }
                    }
                }
            }
        }
    } else {
        let installed_pkgs = get_installed_packages(core_db.clone(), 1024)?;
        for pkg in installed_pkgs {
            if let Ok(pkg) = pkg {
                let filter = PackageFilter {
                    pkg_name: Some(pkg.pkg_name.clone()),
                    repo_name: Some(pkg.repo_name.clone()),
                    family: Some(pkg.pkg_id.clone()),
                    ..Default::default()
                };

                if let Some(Ok(available_pkg)) = get_all_packages(repo_db.clone(), 1024)?
                    .into_iter()
                    .find(|p| matches_filter(p.as_ref().ok(), &filter))
                {
                    if needs_update(&pkg, &available_pkg) {
                        update_targets.push(InstallTarget {
                            package: available_pkg,
                            existing_install: Some(pkg),
                        });
                    }
                }
            }
        }
    };

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

fn matches_filter(package: Option<&Package>, filter: &PackageFilter) -> bool {
    if let Some(pkg) = package {
        filter
            .pkg_name
            .as_ref()
            .map_or(true, |n| n == &pkg.pkg_name)
            && filter
                .repo_name
                .as_ref()
                .map_or(true, |r| r == &pkg.repo_name)
            && filter.family.as_ref().map_or(true, |f| f == &pkg.pkg_id)
    } else {
        false
    }
}

fn needs_update(installed: &InstalledPackage, available: &Package) -> bool {
    if installed.version != available.version {
        return available.version != installed.version;
    }
    installed.checksum != available.checksum
}
