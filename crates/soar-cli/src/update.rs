use std::sync::{atomic::Ordering, Arc};

use nu_ansi_term::Color::{Cyan, Green, Red};
use soar_core::{
    database::{
        connection::DieselDatabase,
        models::{InstalledPackage, Package},
    },
    error::SoarError,
    package::{install::InstallTarget, query::PackageQuery, update::remove_old_versions},
    SoarResult,
};
use soar_db::repository::{
    core::{CoreRepository, SortDirection},
    metadata::MetadataRepository,
};
use tabled::{
    builder::Builder,
    settings::{themes::BorderCorrection, Panel, Style},
};
use tracing::{error, info, warn};

use crate::{
    install::{create_install_context, install_single_package, InstallContext},
    logging::{clear_multi_progress, set_multi_progress},
    progress::{self, create_progress_bar},
    state::AppState,
    utils::{ask_target_action, display_settings, icon_or, Colored, Icons},
};

fn get_existing(
    package: &Package,
    diesel_db: &DieselDatabase,
) -> SoarResult<Option<InstalledPackage>> {
    let existing = diesel_db.with_conn(|conn| {
        CoreRepository::find_exact(
            conn,
            &package.repo_name,
            &package.pkg_name,
            &package.pkg_id,
            &package.version,
        )
    })?;

    Ok(existing.map(Into::into))
}

pub async fn update_packages(
    packages: Option<Vec<String>>,
    keep: bool,
    ask: bool,
    no_verify: bool,
) -> SoarResult<()> {
    let state = AppState::new();
    let metadata_mgr = state.metadata_manager().await?;
    let diesel_db = state.diesel_core_db()?.clone();
    let config = state.config();

    let mut update_targets = Vec::new();

    if let Some(packages) = packages {
        for package in packages {
            let query = PackageQuery::try_from(package.as_str())?;

            let installed_pkgs: Vec<InstalledPackage> = diesel_db
                .with_conn(|conn| {
                    CoreRepository::list_filtered(
                        conn,
                        query.repo_name.as_deref(),
                        query.name.as_deref(),
                        query.pkg_id.as_deref(),
                        query.version.as_deref(),
                        Some(true), // is_installed
                        None,
                        Some(1),
                        Some(SortDirection::Asc),
                    )
                })?
                .into_iter()
                .map(Into::into)
                .collect();

            for pkg in installed_pkgs {
                // Skip local packages (installed from URLs) - no version tracking
                if pkg.repo_name == "local" {
                    info!(
                        "Skipping {}#{} (local package - no version tracking)",
                        pkg.pkg_name, pkg.pkg_id
                    );
                    continue;
                }

                let new_pkg: Option<Package> = metadata_mgr
                    .query_repo(&pkg.repo_name, |conn| {
                        MetadataRepository::find_newer_version(
                            conn,
                            &pkg.pkg_name,
                            &pkg.pkg_id,
                            &pkg.version,
                        )
                    })?
                    .flatten()
                    .map(|p| {
                        let mut package: Package = p.into();
                        package.repo_name = pkg.repo_name.clone();
                        package
                    });

                if let Some(package) = new_pkg {
                    let with_pkg_id = pkg.with_pkg_id;

                    // Check if the new version is already installed (skip if so)
                    let new_version_installed = get_existing(&package, &diesel_db)?;
                    if let Some(ref installed) = new_version_installed {
                        if installed.is_installed {
                            continue;
                        }
                    }

                    update_targets.push(InstallTarget {
                        package,
                        existing_install: Some(pkg.clone()),
                        with_pkg_id,
                        pinned: pkg.pinned,
                        profile: Some(pkg.profile.clone()),
                        portable: pkg.portable_path.clone(),
                        portable_home: pkg.portable_home.clone(),
                        portable_config: pkg.portable_config.clone(),
                        portable_share: pkg.portable_share.clone(),
                        portable_cache: pkg.portable_cache.clone(),
                    })
                }
            }
        }
    } else {
        let installed_packages: Vec<InstalledPackage> = diesel_db
            .with_conn(CoreRepository::list_updatable)?
            .into_iter()
            .map(Into::into)
            .collect();

        for pkg in installed_packages {
            // Skip local packages (installed from URLs) - no version tracking
            if pkg.repo_name == "local" {
                continue;
            }

            let new_pkg: Option<Package> = metadata_mgr
                .query_repo(&pkg.repo_name, |conn| {
                    MetadataRepository::find_newer_version(
                        conn,
                        &pkg.pkg_name,
                        &pkg.pkg_id,
                        &pkg.version,
                    )
                })?
                .flatten()
                .map(|p| {
                    let mut package: Package = p.into();
                    package.repo_name = pkg.repo_name.clone();
                    package
                });

            if let Some(package) = new_pkg {
                let with_pkg_id = pkg.with_pkg_id;

                // Check if the new version is already installed (skip if so)
                let new_version_installed = get_existing(&package, &diesel_db)?;
                if let Some(ref installed) = new_version_installed {
                    if installed.is_installed {
                        continue;
                    }
                }

                // Keep existing_install to preserve portable settings.
                // Install always creates a new directory based on bsum.
                update_targets.push(InstallTarget {
                    package,
                    existing_install: Some(pkg.clone()),
                    with_pkg_id,
                    pinned: pkg.pinned,
                    profile: Some(pkg.profile.clone()),
                    portable: pkg.portable_path.clone(),
                    portable_home: pkg.portable_home.clone(),
                    portable_config: pkg.portable_config.clone(),
                    portable_share: pkg.portable_share.clone(),
                    portable_cache: pkg.portable_cache.clone(),
                })
            }
        }
    }

    if update_targets.is_empty() {
        info!("No packages to update.");
        return Ok(());
    }

    if ask {
        ask_target_action(&update_targets, "update")?;
    }

    let ctx = create_install_context(
        update_targets.len(),
        config.parallel_limit.unwrap_or(4),
        None,
        None,
        None,
        None,
        None,
        false,
        no_verify,
    );

    perform_update(ctx, update_targets, diesel_db, keep).await?;

    Ok(())
}

pub async fn perform_update(
    ctx: InstallContext,
    targets: Vec<InstallTarget>,
    diesel_db: DieselDatabase,
    keep: bool,
) -> SoarResult<()> {
    set_multi_progress(&ctx.multi_progress);
    let mut handles = Vec::new();
    let fixed_width = 40;

    for (idx, target) in targets.iter().enumerate() {
        let handle = spawn_update_task(
            &ctx,
            target.clone(),
            diesel_db.clone(),
            idx,
            fixed_width,
            keep,
        )
        .await;
        handles.push(handle);
    }

    for handle in handles {
        handle
            .await
            .map_err(|err| SoarError::Custom(format!("Join handle error: {err}")))?;
    }

    ctx.total_progress_bar.finish_and_clear();
    clear_multi_progress();

    for warn in ctx.warnings.lock().unwrap().iter() {
        warn!("{warn}");
    }

    for error in ctx.errors.lock().unwrap().iter() {
        error!("{error}");
    }

    let updated_count = ctx.installed_count.load(Ordering::Relaxed);
    let failed_count = ctx.failed.load(Ordering::Relaxed);
    let settings = display_settings();

    if settings.icons() {
        let mut builder = Builder::new();

        if updated_count > 0 {
            builder.push_record([
                format!("{} Updated", icon_or(Icons::CHECK, "+")),
                format!(
                    "{}/{}",
                    Colored(Green, updated_count),
                    Colored(Cyan, ctx.total_packages)
                ),
            ]);
        }
        if failed_count > 0 {
            builder.push_record([
                format!("{} Failed", icon_or(Icons::CROSS, "!")),
                format!("{}", Colored(Red, failed_count)),
            ]);
        }
        if updated_count == 0 && failed_count == 0 {
            builder.push_record([
                format!("{} Status", icon_or(Icons::WARNING, "!")),
                "No packages updated".to_string(),
            ]);
        }

        let table = builder
            .build()
            .with(Panel::header("Update Summary"))
            .with(Style::rounded())
            .with(BorderCorrection {})
            .to_string();

        info!("\n{table}");
    } else {
        info!(
            "Updated {}/{} packages{}",
            updated_count,
            ctx.total_packages,
            if failed_count > 0 {
                format!(", {} failed", failed_count)
            } else {
                String::new()
            }
        );
    }

    Ok(())
}

async fn spawn_update_task(
    ctx: &InstallContext,
    target: InstallTarget,
    diesel_db: DieselDatabase,
    idx: usize,
    fixed_width: usize,
    keep: bool,
) -> tokio::task::JoinHandle<()> {
    let permit = ctx.semaphore.clone().acquire_owned().await.unwrap();
    let progress_bar = ctx
        .multi_progress
        .insert_from_back(1, create_progress_bar());

    let message = format!(
        "[{}/{}] {}#{}",
        idx + 1,
        ctx.total_packages,
        target.package.pkg_name,
        target.package.pkg_id
    );
    let message = if message.len() > fixed_width {
        format!("{message:.fixed_width$}")
    } else {
        format!("{message:<fixed_width$}")
    };
    progress_bar.set_prefix(message);

    let progress_callback = Arc::new(move |state| {
        progress::handle_progress(state, &progress_bar);
    });

    let total_pb = ctx.total_progress_bar.clone();
    let installed_count = ctx.installed_count.clone();
    let mut ctx = ctx.clone();
    ctx.portable = target.portable.clone();
    ctx.portable_home = target.portable_home.clone();
    ctx.portable_config = target.portable_config.clone();
    ctx.portable_share = target.portable_share.clone();

    tokio::spawn(async move {
        let result =
            install_single_package(&ctx, &target, progress_callback, diesel_db.clone()).await;

        if let Err(err) = result {
            match err {
                SoarError::Warning(err) => {
                    let mut warnings = ctx.warnings.lock().unwrap();
                    warnings.push(err);

                    if !keep {
                        let _ = remove_old_versions(&target.package, &diesel_db, false);
                    }
                }
                _ => {
                    let mut errors = ctx.errors.lock().unwrap();
                    errors.push(err.to_string());
                }
            }
        } else {
            installed_count.fetch_add(1, Ordering::Relaxed);
            total_pb.inc(1);

            if !keep {
                let _ = remove_old_versions(&target.package, &diesel_db, false);
            }
        }

        drop(permit);
    })
}
