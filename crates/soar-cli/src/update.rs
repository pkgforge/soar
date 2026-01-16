use std::sync::{atomic::Ordering, Arc};

use nu_ansi_term::Color::{Cyan, Green, Red};
use soar_config::packages::{PackagesConfig, ResolvedPackage, UpdateSource};
use soar_core::{
    database::{
        connection::DieselDatabase,
        models::{InstalledPackage, Package},
    },
    error::SoarError,
    package::{
        install::InstallTarget, query::PackageQuery, release_source::run_version_command,
        remote_update::check_for_update, update::remove_old_versions, url::UrlPackage,
    },
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

/// Tracks URL packages that need their packages.toml updated after successful update
#[derive(Clone)]
struct UrlUpdateInfo {
    pkg_name: String,
    new_version: String,
    new_url: Option<String>,
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

    // Load packages.toml to get update sources for local packages
    let packages_config = PackagesConfig::load(None).ok();
    let resolved_packages = packages_config
        .as_ref()
        .map(|c| c.resolved_packages())
        .unwrap_or_default();

    let mut update_targets = Vec::new();
    let mut url_updates: Vec<UrlUpdateInfo> = Vec::new();

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
                if pkg.repo_name == "local" {
                    if let Some((target, url_info)) =
                        check_local_package_update(&pkg, &resolved_packages)?
                    {
                        update_targets.push(target);
                        url_updates.push(url_info);
                    } else {
                        info!(
                            "Skipping {}#{} (no update source configured)",
                            pkg.pkg_name, pkg.pkg_id
                        );
                    }
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
                        pinned: pkg.pinned,
                        profile: Some(pkg.profile.clone()),
                        portable: pkg.portable_path.clone(),
                        portable_home: pkg.portable_home.clone(),
                        portable_config: pkg.portable_config.clone(),
                        portable_share: pkg.portable_share.clone(),
                        portable_cache: pkg.portable_cache.clone(),
                        entrypoint: None,
                        binaries: None,
                        nested_extract: None,
                        extract_root: None,
                        hooks: None,
                        build: None,
                        sandbox: None,
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

        // Get local packages for update checking
        let local_packages: Vec<InstalledPackage> = diesel_db
            .with_conn(|conn| {
                CoreRepository::list_filtered(
                    conn,
                    Some("local"),
                    None,
                    None,
                    None,
                    Some(true),
                    None,
                    None,
                    None,
                )
            })?
            .into_iter()
            .map(Into::into)
            .collect();

        // Check local packages for updates
        for pkg in local_packages {
            if let Some((target, url_info)) = check_local_package_update(&pkg, &resolved_packages)?
            {
                update_targets.push(target);
                url_updates.push(url_info);
            }
        }

        // Check repository packages for updates
        for pkg in installed_packages {
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
                    pinned: pkg.pinned,
                    profile: Some(pkg.profile.clone()),
                    portable: pkg.portable_path.clone(),
                    portable_home: pkg.portable_home.clone(),
                    portable_config: pkg.portable_config.clone(),
                    portable_share: pkg.portable_share.clone(),
                    portable_cache: pkg.portable_cache.clone(),
                    entrypoint: None,
                    binaries: None,
                    nested_extract: None,
                    extract_root: None,
                    hooks: None,
                    build: None,
                    sandbox: None,
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

    perform_update(ctx, update_targets, diesel_db.clone(), keep).await?;

    // Update URLs in packages.toml for successfully updated URL packages
    for url_info in url_updates {
        let is_installed = diesel_db
            .with_conn(|conn| {
                CoreRepository::list_filtered(
                    conn,
                    Some("local"),
                    Some(&url_info.pkg_name),
                    None,
                    Some(&url_info.new_version),
                    Some(true),
                    None,
                    Some(1),
                    None,
                )
            })
            .map(|pkgs| !pkgs.is_empty())
            .unwrap_or(false);

        if is_installed {
            if let Err(e) = PackagesConfig::update_package(
                &url_info.pkg_name,
                url_info.new_url.as_deref(),
                Some(&url_info.new_version),
                None,
            ) {
                warn!(
                    "Failed to update version for '{}' in packages.toml: {}",
                    url_info.pkg_name, e
                );
            }
        }
    }

    Ok(())
}

/// Derive an UpdateSource from a resolved package.
fn derive_update_source(resolved: &ResolvedPackage) -> Option<UpdateSource> {
    if let Some(ref update) = resolved.update {
        return Some(update.clone());
    }

    if let Some(ref repo) = resolved.github {
        return Some(UpdateSource::GitHub {
            repo: repo.clone(),
            asset_pattern: resolved.asset_pattern.clone(),
            include_prerelease: resolved.include_prerelease,
            tag_pattern: resolved.tag_pattern.clone(),
        });
    }

    if let Some(ref repo) = resolved.gitlab {
        return Some(UpdateSource::GitLab {
            repo: repo.clone(),
            asset_pattern: resolved.asset_pattern.clone(),
            include_prerelease: resolved.include_prerelease,
            tag_pattern: resolved.tag_pattern.clone(),
        });
    }

    None
}

/// Check if a local package has an update available via its update source
fn check_local_package_update(
    pkg: &InstalledPackage,
    resolved_packages: &[ResolvedPackage],
) -> SoarResult<Option<(InstallTarget, UrlUpdateInfo)>> {
    // Find resolved package that has an update source
    let resolved = resolved_packages
        .iter()
        .find(|r| r.name == pkg.pkg_name && derive_update_source(r).is_some());

    let Some(resolved) = resolved else {
        return Ok(None);
    };

    if resolved.pinned {
        info!("Skipping {}#{} (pinned)", pkg.pkg_name, pkg.pkg_id);
        return Ok(None);
    }

    let is_github_or_gitlab = resolved.github.is_some() || resolved.gitlab.is_some();
    let update_source = derive_update_source(resolved).unwrap();

    let (version, download_url, update_toml_url) = if let Some(ref cmd) = resolved.version_command {
        let v = match run_version_command(cmd) {
            Ok(v) => v.strip_prefix('v').unwrap_or(&v).to_string(),
            Err(e) => {
                warn!("Failed to run version_command for {}: {}", pkg.pkg_name, e);
                return Ok(None);
            }
        };

        let installed_version = pkg.version.strip_prefix('v').unwrap_or(&pkg.version);
        if v == installed_version {
            return Ok(None);
        }

        let update = match check_for_update(&update_source, &pkg.version) {
            Ok(Some(u)) => u,
            Ok(None) => {
                warn!("No release found for {}", pkg.pkg_name);
                return Ok(None);
            }
            Err(e) => {
                warn!("Failed to check for updates for {}: {}", pkg.pkg_name, e);
                return Ok(None);
            }
        };
        (v, update.download_url, None)
    } else {
        let update = match check_for_update(&update_source, &pkg.version) {
            Ok(Some(u)) => u,
            Ok(None) => return Ok(None),
            Err(e) => {
                warn!("Failed to check for updates for {}: {}", pkg.pkg_name, e);
                return Ok(None);
            }
        };
        let v = update
            .new_version
            .strip_prefix('v')
            .unwrap_or(&update.new_version)
            .to_string();
        let url = if is_github_or_gitlab {
            None
        } else {
            Some(update.download_url.clone())
        };
        (v, update.download_url, url)
    };

    let updated_url_pkg = UrlPackage::from_remote(
        &download_url,
        Some(&pkg.pkg_name),
        Some(&version),
        pkg.pkg_type.as_deref(),
        Some(&pkg.pkg_id),
    )?;

    let target = InstallTarget {
        package: updated_url_pkg.to_package(),
        existing_install: Some(pkg.clone()),
        pinned: resolved.pinned,
        profile: resolved.profile.clone(),
        portable: resolved.portable.as_ref().and_then(|p| p.path.clone()),
        portable_home: resolved.portable.as_ref().and_then(|p| p.home.clone()),
        portable_config: resolved.portable.as_ref().and_then(|p| p.config.clone()),
        portable_share: resolved.portable.as_ref().and_then(|p| p.share.clone()),
        portable_cache: resolved.portable.as_ref().and_then(|p| p.cache.clone()),
        entrypoint: resolved.entrypoint.clone(),
        binaries: resolved.binaries.clone(),
        nested_extract: resolved.nested_extract.clone(),
        extract_root: resolved.extract_root.clone(),
        hooks: resolved.hooks.clone(),
        build: resolved.build.clone(),
        sandbox: resolved.sandbox.clone(),
    };

    let url_info = UrlUpdateInfo {
        pkg_name: pkg.pkg_name.clone(),
        new_version: updated_url_pkg.version.clone(),
        new_url: update_toml_url,
    };

    Ok(Some((target, url_info)))
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
