use std::collections::HashSet;

use soar_config::packages::{PackagesConfig, ResolvedPackage};
use soar_core::{
    database::{
        connection::DieselDatabase,
        models::{InstalledPackage, Package},
    },
    package::{
        install::InstallTarget,
        query::PackageQuery,
        release_source::{run_version_command, ReleaseSource},
        update::remove_old_versions,
        url::UrlPackage,
    },
    utils::substitute_placeholders,
    SoarResult,
};
use soar_db::repository::{
    core::{CoreRepository, SortDirection},
    metadata::MetadataRepository,
};
use soar_events::{SoarEvent, UpdateCheckStatus, UpdateCleanupStage};
use tracing::{debug, warn};

use crate::{
    install::perform_installation, progress::next_op_id, InstallOptions, SoarContext, UpdateInfo,
    UpdateReport, UrlUpdateInfo,
};

/// Check for available updates.
///
/// If `packages` is `Some`, only checks the specified packages.
/// If `None`, checks all updatable packages.
pub async fn check_updates(
    ctx: &SoarContext,
    packages: Option<&[String]>,
) -> SoarResult<Vec<UpdateInfo>> {
    debug!("checking for updates");
    let metadata_mgr = ctx.metadata_manager().await?;
    let diesel_db = ctx.diesel_core_db()?.clone();

    let packages_config = PackagesConfig::load(None).ok();
    let resolved_packages = packages_config
        .as_ref()
        .map(|c| c.resolved_packages())
        .unwrap_or_default();

    let mut updates = Vec::new();

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
                        Some(true),
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
                    if let Some(update_info) = check_local_update(&pkg, &resolved_packages, ctx)? {
                        updates.push(update_info);
                    }
                    continue;
                }

                if let Some(update_info) = check_repo_update(&pkg, metadata_mgr, &diesel_db, ctx)? {
                    updates.push(update_info);
                }
            }
        }
    } else {
        // Check all updatable packages
        let installed_packages: Vec<InstalledPackage> = diesel_db
            .with_conn(CoreRepository::list_updatable)?
            .into_iter()
            .map(Into::into)
            .collect();

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

        for pkg in local_packages {
            if let Some(update_info) = check_local_update(&pkg, &resolved_packages, ctx)? {
                updates.push(update_info);
            }
        }

        for pkg in installed_packages {
            if pkg.repo_name == "local" {
                continue;
            }

            if let Some(update_info) = check_repo_update(&pkg, metadata_mgr, &diesel_db, ctx)? {
                updates.push(update_info);
            }
        }
    }

    Ok(updates)
}

fn check_repo_update(
    pkg: &InstalledPackage,
    metadata_mgr: &soar_core::database::connection::MetadataManager,
    diesel_db: &DieselDatabase,
    ctx: &SoarContext,
) -> SoarResult<Option<UpdateInfo>> {
    let new_pkg: Option<Package> = metadata_mgr
        .query_repo(&pkg.repo_name, |conn| {
            MetadataRepository::find_newer_version(conn, &pkg.pkg_name, &pkg.pkg_id, &pkg.version)
        })?
        .flatten()
        .map(|p| {
            let mut package: Package = p.into();
            package.repo_name = pkg.repo_name.clone();
            package
        });

    let Some(package) = new_pkg else {
        ctx.events().emit(SoarEvent::UpdateCheck {
            pkg_name: pkg.pkg_name.clone(),
            pkg_id: pkg.pkg_id.clone(),
            status: UpdateCheckStatus::UpToDate {
                version: pkg.version.clone(),
            },
        });
        return Ok(None);
    };

    // Check if the new version is already installed
    let new_version_installed = get_existing(&package, diesel_db)?;
    if let Some(ref installed) = new_version_installed {
        if installed.is_installed {
            return Ok(None);
        }
    }

    ctx.events().emit(SoarEvent::UpdateCheck {
        pkg_name: pkg.pkg_name.clone(),
        pkg_id: pkg.pkg_id.clone(),
        status: UpdateCheckStatus::Available {
            current_version: pkg.version.clone(),
            new_version: package.version.clone(),
        },
    });

    Ok(Some(UpdateInfo {
        pkg_name: pkg.pkg_name.clone(),
        pkg_id: pkg.pkg_id.clone(),
        repo_name: pkg.repo_name.clone(),
        current_version: pkg.version.clone(),
        new_version: package.version.clone(),
        target: InstallTarget {
            package,
            existing_install: Some(pkg.clone()),
            pinned: pkg.pinned,
            profile: Some(pkg.profile.clone()),
            portable: pkg.portable_path.clone(),
            portable_home: pkg.portable_home.clone(),
            portable_config: pkg.portable_config.clone(),
            portable_share: pkg.portable_share.clone(),
            portable_cache: pkg.portable_cache.clone(),
            ..Default::default()
        },
    }))
}

fn check_local_update(
    pkg: &InstalledPackage,
    resolved_packages: &[ResolvedPackage],
    ctx: &SoarContext,
) -> SoarResult<Option<UpdateInfo>> {
    let resolved = resolved_packages
        .iter()
        .find(|r| r.name == pkg.pkg_name && has_update_source(r));

    let Some(resolved) = resolved else {
        ctx.events().emit(SoarEvent::UpdateCheck {
            pkg_name: pkg.pkg_name.clone(),
            pkg_id: pkg.pkg_id.clone(),
            status: UpdateCheckStatus::Skipped {
                reason: "no update source configured".into(),
            },
        });
        return Ok(None);
    };

    if resolved.pinned {
        ctx.events().emit(SoarEvent::UpdateCheck {
            pkg_name: pkg.pkg_name.clone(),
            pkg_id: pkg.pkg_id.clone(),
            status: UpdateCheckStatus::Skipped {
                reason: "pinned".into(),
            },
        });
        return Ok(None);
    }

    let is_github_or_gitlab = resolved.github.is_some() || resolved.gitlab.is_some();

    let (version, download_url, size, _update_toml_url) =
        if let Some(ref cmd) = resolved.version_command {
            let result = match run_version_command(cmd) {
                Ok(r) => r,
                Err(e) => {
                    warn!("Failed to run version_command for {}: {}", pkg.pkg_name, e);
                    return Ok(None);
                }
            };

            let v = result
                .version
                .strip_prefix('v')
                .unwrap_or(&result.version)
                .to_string();

            let installed_version = pkg.version.strip_prefix('v').unwrap_or(&pkg.version);
            if v == installed_version {
                ctx.events().emit(SoarEvent::UpdateCheck {
                    pkg_name: pkg.pkg_name.clone(),
                    pkg_id: pkg.pkg_id.clone(),
                    status: UpdateCheckStatus::UpToDate {
                        version: pkg.version.clone(),
                    },
                });
                return Ok(None);
            }

            let (url, should_update_toml_url) = match result.download_url {
                Some(url) => (url, true),
                None => {
                    match &resolved.url {
                        Some(url) => (substitute_placeholders(url, Some(&v)), false),
                        None => {
                            warn!(
                            "version_command returned no URL and no url field configured for {}",
                            pkg.pkg_name
                        );
                            return Ok(None);
                        }
                    }
                }
            };

            let toml_url = if is_github_or_gitlab || !should_update_toml_url {
                None
            } else {
                Some(url.clone())
            };
            (v, url, result.size, toml_url)
        } else {
            let release_source = match ReleaseSource::from_resolved(resolved) {
                Some(s) => s,
                None => {
                    warn!("No release source configured for {}", pkg.pkg_name);
                    return Ok(None);
                }
            };
            let release = match release_source.resolve() {
                Ok(r) => r,
                Err(e) => {
                    warn!("Failed to check for updates for {}: {}", pkg.pkg_name, e);
                    return Ok(None);
                }
            };

            let v = release
                .version
                .strip_prefix('v')
                .unwrap_or(&release.version)
                .to_string();

            let installed_version = pkg.version.strip_prefix('v').unwrap_or(&pkg.version);
            if v == installed_version {
                ctx.events().emit(SoarEvent::UpdateCheck {
                    pkg_name: pkg.pkg_name.clone(),
                    pkg_id: pkg.pkg_id.clone(),
                    status: UpdateCheckStatus::UpToDate {
                        version: pkg.version.clone(),
                    },
                });
                return Ok(None);
            }

            let url = if is_github_or_gitlab {
                None
            } else {
                Some(release.download_url.clone())
            };
            (v, release.download_url, release.size, url)
        };

    let mut updated_url_pkg = UrlPackage::from_remote(
        &download_url,
        Some(&pkg.pkg_name),
        Some(&version),
        pkg.pkg_type.as_deref(),
        Some(&pkg.pkg_id),
    )?;
    updated_url_pkg.size = size;

    ctx.events().emit(SoarEvent::UpdateCheck {
        pkg_name: pkg.pkg_name.clone(),
        pkg_id: pkg.pkg_id.clone(),
        status: UpdateCheckStatus::Available {
            current_version: pkg.version.clone(),
            new_version: version.clone(),
        },
    });

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

    Ok(Some(UpdateInfo {
        pkg_name: pkg.pkg_name.clone(),
        pkg_id: pkg.pkg_id.clone(),
        repo_name: pkg.repo_name.clone(),
        current_version: pkg.version.clone(),
        new_version: version,
        target,
    }))
}

fn has_update_source(resolved: &ResolvedPackage) -> bool {
    resolved.version_command.is_some() || resolved.github.is_some() || resolved.gitlab.is_some()
}

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

/// Perform updates for the given update targets.
///
/// Each update is essentially an install of the new version followed by
/// cleanup of old versions (unless `keep_old` is true).
pub async fn perform_update(
    ctx: &SoarContext,
    updates: Vec<UpdateInfo>,
    keep_old: bool,
) -> SoarResult<UpdateReport> {
    debug!(
        count = updates.len(),
        keep_old = keep_old,
        "performing updates"
    );

    let packages_config = PackagesConfig::load(None).ok();
    let resolved_packages = packages_config
        .as_ref()
        .map(|c| c.resolved_packages())
        .unwrap_or_default();

    // Collect URL update tracking info before we consume the updates
    let url_tracking: Vec<(String, String, Option<String>)> = updates
        .iter()
        .filter(|u| u.repo_name == "local")
        .filter_map(|u| {
            resolved_packages
                .iter()
                .find(|r| r.name == u.pkg_name && has_update_source(r))
                .map(|_| {
                    (
                        u.pkg_name.clone(),
                        u.new_version.clone(),
                        None, // URL tracking is handled in check_local_update
                    )
                })
        })
        .collect();

    let targets: Vec<InstallTarget> = updates.into_iter().map(|u| u.target).collect();

    let options = InstallOptions {
        no_verify: false,
        ..Default::default()
    };

    let install_report = perform_installation(ctx, targets.clone(), &options).await?;

    // Clean up old versions only for successfully updated packages
    if !keep_old {
        let diesel_db = ctx.diesel_core_db()?.clone();
        let succeeded: HashSet<(&str, &str)> = install_report
            .installed
            .iter()
            .map(|i| (i.pkg_name.as_str(), i.pkg_id.as_str()))
            .collect();

        for target in &targets {
            let pkg = &target.package;
            if !succeeded.contains(&(pkg.pkg_name.as_str(), pkg.pkg_id.as_str())) {
                continue;
            }

            let op_id = next_op_id();
            ctx.events().emit(SoarEvent::UpdateCleanup {
                op_id,
                pkg_name: pkg.pkg_name.clone(),
                pkg_id: pkg.pkg_id.clone(),
                old_version: target
                    .existing_install
                    .as_ref()
                    .map(|e| e.version.clone())
                    .unwrap_or_default(),
                stage: UpdateCleanupStage::Removing,
            });

            let _ = remove_old_versions(pkg, &diesel_db, false);

            ctx.events().emit(SoarEvent::UpdateCleanup {
                op_id,
                pkg_name: pkg.pkg_name.clone(),
                pkg_id: pkg.pkg_id.clone(),
                old_version: target
                    .existing_install
                    .as_ref()
                    .map(|e| e.version.clone())
                    .unwrap_or_default(),
                stage: UpdateCleanupStage::Complete {
                    size_freed: None,
                },
            });
        }
    }

    // Update packages.toml for URL packages
    let mut url_updates = Vec::new();
    let diesel_db = ctx.diesel_core_db()?;
    for (pkg_name, new_version, new_url) in url_tracking {
        let is_installed = diesel_db
            .with_conn(|conn| {
                CoreRepository::list_filtered(
                    conn,
                    Some("local"),
                    Some(&pkg_name),
                    None,
                    Some(&new_version),
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
                &pkg_name,
                new_url.as_deref(),
                Some(&new_version),
                None,
            ) {
                warn!(
                    "Failed to update version for '{}' in packages.toml: {}",
                    pkg_name, e
                );
            }

            url_updates.push(UrlUpdateInfo {
                pkg_name,
                new_version,
                new_url,
            });
        }
    }

    Ok(UpdateReport {
        updated: install_report.installed,
        failed: install_report.failed,
        url_updates,
    })
}
