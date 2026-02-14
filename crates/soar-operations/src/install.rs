use std::{
    fs::{self, File},
    io::{BufReader, Read},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use minisign_verify::{PublicKey, Signature};
use soar_config::{config::get_config, utils::default_install_patterns};
use soar_core::{
    database::{
        connection::{DieselDatabase, MetadataManager},
        models::{InstalledPackage, Package},
    },
    error::{ErrorContext, SoarError},
    package::{
        install::{InstallMarker, InstallTarget, PackageInstaller},
        query::PackageQuery,
        update::remove_old_versions,
        url::UrlPackage,
    },
    SoarResult,
};
use soar_db::repository::{
    core::{CoreRepository, SortDirection},
    metadata::MetadataRepository,
};
use soar_events::{InstallStage, SoarEvent, VerifyStage};
use soar_package::integrate_package;
use soar_utils::{
    hash::{calculate_checksum, hash_string},
    lock::FileLock,
    pattern::apply_sig_variants,
};
use tokio::sync::Semaphore;
use tracing::{debug, trace, warn};

use crate::{
    progress::{create_progress_bridge, next_op_id},
    utils::{has_desktop_integration, mangle_package_symlinks},
    FailedInfo, InstallOptions, InstallReport, InstalledInfo, ResolveResult, SoarContext,
};

/// Resolve package queries into install targets or ambiguity results.
///
/// For each query string, returns a [`ResolveResult`] indicating whether the package
/// was resolved, is ambiguous (multiple candidates), not found, or already installed.
pub async fn resolve_packages(
    ctx: &SoarContext,
    packages: &[String],
    options: &InstallOptions,
) -> SoarResult<Vec<ResolveResult>> {
    debug!(count = packages.len(), "resolving packages for install");
    let metadata_mgr = ctx.metadata_manager().await?;
    let diesel_db = ctx.diesel_core_db()?;

    let mut results = Vec::with_capacity(packages.len());

    for package in packages {
        if UrlPackage::is_remote(package) {
            results.push(resolve_url_package(diesel_db, package, options)?);
            continue;
        }

        let query = PackageQuery::try_from(package.as_str())?;

        // Handle #all: install all packages with same pkg_id
        if let Some(ref pkg_id) = query.pkg_id {
            if pkg_id == "all" {
                results.push(resolve_all_variants(
                    metadata_mgr,
                    diesel_db,
                    &query,
                    options,
                )?);
                continue;
            }
        }

        // Handle pkg_id-only queries (no name)
        if query.name.is_none() && query.pkg_id.is_some() {
            results.push(resolve_by_pkg_id(metadata_mgr, diesel_db, &query, options)?);
            continue;
        }

        // Normal resolution
        results.push(resolve_normal(
            metadata_mgr,
            diesel_db,
            package,
            &query,
            options,
        )?);
    }

    Ok(results)
}

fn resolve_url_package(
    diesel_db: &DieselDatabase,
    package: &str,
    options: &InstallOptions,
) -> SoarResult<ResolveResult> {
    let url_pkg = UrlPackage::from_remote(
        package,
        options.name_override.as_deref(),
        options.version_override.as_deref(),
        options.pkg_type_override.as_deref(),
        options.pkg_id_override.as_deref(),
    )?;

    let installed_packages: Vec<InstalledPackage> = diesel_db
        .with_conn(|conn| {
            CoreRepository::list_filtered(
                conn,
                Some("local"),
                Some(&url_pkg.pkg_name),
                Some(&url_pkg.pkg_id),
                None,
                None,
                None,
                None,
                Some(SortDirection::Asc),
            )
        })?
        .into_iter()
        .map(Into::into)
        .collect();

    let installed_pkg = installed_packages.iter().find(|ip| ip.is_installed);

    if let Some(installed) = installed_pkg {
        if !options.force {
            return Ok(ResolveResult::AlreadyInstalled {
                pkg_name: installed.pkg_name.clone(),
                pkg_id: installed.pkg_id.clone(),
                repo_name: installed.repo_name.clone(),
                version: installed.version.clone(),
            });
        }
    }

    let existing_install = installed_pkg
        .cloned()
        .or_else(|| installed_packages.into_iter().next());

    Ok(ResolveResult::Resolved(vec![InstallTarget {
        package: url_pkg.to_package(),
        existing_install,
        pinned: false,
        profile: None,
        ..Default::default()
    }]))
}

fn resolve_all_variants(
    metadata_mgr: &MetadataManager,
    diesel_db: &DieselDatabase,
    query: &PackageQuery,
    options: &InstallOptions,
) -> SoarResult<ResolveResult> {
    let variants: Vec<Package> = if let Some(ref repo_name) = query.repo_name {
        metadata_mgr
            .query_repo(repo_name, |conn| {
                MetadataRepository::find_filtered(
                    conn,
                    query.name.as_deref(),
                    None,
                    None,
                    None,
                    Some(SortDirection::Asc),
                )
            })?
            .unwrap_or_default()
            .into_iter()
            .map(|p| {
                let mut pkg: Package = p.into();
                pkg.repo_name = repo_name.clone();
                pkg
            })
            .collect()
    } else {
        metadata_mgr.query_all_flat(|repo_name, conn| {
            let pkgs = MetadataRepository::find_filtered(
                conn,
                query.name.as_deref(),
                None,
                None,
                None,
                Some(SortDirection::Asc),
            )?;
            Ok(pkgs
                .into_iter()
                .map(|p| {
                    let mut pkg: Package = p.into();
                    pkg.repo_name = repo_name.to_string();
                    pkg
                })
                .collect())
        })?
    };

    if variants.is_empty() {
        return Ok(ResolveResult::NotFound(
            query.name.clone().unwrap_or_default(),
        ));
    }

    // Multiple distinct pkg_ids -> ambiguous, caller must pick
    if variants.len() > 1 {
        let first_pkg_id = &variants[0].pkg_id;
        let all_same_pkg_id = variants.iter().all(|v| v.pkg_id == *first_pkg_id);
        if !all_same_pkg_id {
            return Ok(ResolveResult::Ambiguous(crate::AmbiguousPackage {
                query: query.name.clone().unwrap_or_default(),
                candidates: variants,
            }));
        }
    }

    let target_pkg_id = variants[0].pkg_id.clone();

    // Find all packages with this pkg_id
    let all_pkgs: Vec<Package> = if let Some(ref repo_name) = query.repo_name {
        metadata_mgr
            .query_repo(repo_name, |conn| {
                MetadataRepository::find_filtered(
                    conn,
                    None,
                    Some(&target_pkg_id),
                    None,
                    None,
                    Some(SortDirection::Asc),
                )
            })?
            .unwrap_or_default()
            .into_iter()
            .map(|p| {
                let mut pkg: Package = p.into();
                pkg.repo_name = repo_name.clone();
                pkg
            })
            .collect()
    } else {
        metadata_mgr.query_all_flat(|repo_name, conn| {
            let pkgs = MetadataRepository::find_filtered(
                conn,
                None,
                Some(&target_pkg_id),
                None,
                None,
                Some(SortDirection::Asc),
            )?;
            Ok(pkgs
                .into_iter()
                .map(|p| {
                    let mut pkg: Package = p.into();
                    pkg.repo_name = repo_name.to_string();
                    pkg
                })
                .collect())
        })?
    };

    let installed_packages: Vec<InstalledPackage> = diesel_db
        .with_conn(|conn| {
            CoreRepository::list_filtered(
                conn,
                query.repo_name.as_deref(),
                None,
                Some(&target_pkg_id),
                None,
                None,
                None,
                None,
                Some(SortDirection::Asc),
            )
        })?
        .into_iter()
        .map(Into::into)
        .collect();

    let mut targets = Vec::new();
    for pkg in all_pkgs {
        let existing_install = installed_packages
            .iter()
            .find(|ip| ip.pkg_name == pkg.pkg_name)
            .cloned();

        if let Some(ref existing) = existing_install {
            if existing.is_installed && !options.force {
                continue;
            }
        }

        let pkg = pkg.resolve(query.version.as_deref());

        targets.push(InstallTarget {
            package: pkg,
            existing_install,
            pinned: query.version.is_some(),
            profile: None,
            ..Default::default()
        });
    }

    Ok(ResolveResult::Resolved(targets))
}

fn resolve_by_pkg_id(
    metadata_mgr: &MetadataManager,
    diesel_db: &DieselDatabase,
    query: &PackageQuery,
    options: &InstallOptions,
) -> SoarResult<ResolveResult> {
    let installed_packages: Vec<InstalledPackage> = diesel_db
        .with_conn(|conn| {
            CoreRepository::list_filtered(
                conn,
                query.repo_name.as_deref(),
                query.name.as_deref(),
                query.pkg_id.as_deref(),
                None,
                None,
                None,
                None,
                Some(SortDirection::Asc),
            )
        })?
        .into_iter()
        .map(Into::into)
        .collect();

    let repo_pkgs: Vec<Package> = if let Some(ref repo_name) = query.repo_name {
        metadata_mgr
            .query_repo(repo_name, |conn| {
                MetadataRepository::find_filtered(
                    conn,
                    None,
                    query.pkg_id.as_deref(),
                    None,
                    None,
                    None,
                )
            })?
            .unwrap_or_default()
            .into_iter()
            .map(|p| {
                let mut pkg: Package = p.into();
                pkg.repo_name = repo_name.clone();
                pkg
            })
            .collect()
    } else {
        metadata_mgr.query_all_flat(|repo_name, conn| {
            let pkgs = MetadataRepository::find_filtered(
                conn,
                None,
                query.pkg_id.as_deref(),
                None,
                None,
                None,
            )?;
            Ok(pkgs
                .into_iter()
                .map(|p| {
                    let mut pkg: Package = p.into();
                    pkg.repo_name = repo_name.to_string();
                    pkg
                })
                .collect())
        })?
    };

    let repo_pkgs: Vec<Package> = if let Some(ref version) = query.version {
        repo_pkgs
            .into_iter()
            .filter(|p| p.has_version(version))
            .collect()
    } else {
        repo_pkgs
    };

    let mut targets = Vec::new();
    for pkg in repo_pkgs {
        let pkg = pkg.resolve(query.version.as_deref());

        let existing_install = installed_packages
            .iter()
            .find(|ip| ip.pkg_name == pkg.pkg_name)
            .cloned();

        if let Some(ref existing) = existing_install {
            if existing.is_installed && !options.force {
                continue;
            }
        }

        targets.push(InstallTarget {
            package: pkg,
            existing_install,
            pinned: query.version.is_some(),
            profile: None,
            ..Default::default()
        });
    }

    Ok(ResolveResult::Resolved(targets))
}

fn resolve_normal(
    metadata_mgr: &MetadataManager,
    diesel_db: &DieselDatabase,
    package_name: &str,
    query: &PackageQuery,
    options: &InstallOptions,
) -> SoarResult<ResolveResult> {
    let installed_packages: Vec<InstalledPackage> = diesel_db
        .with_conn(|conn| {
            CoreRepository::list_filtered(
                conn,
                query.repo_name.as_deref(),
                query.name.as_deref(),
                query.pkg_id.as_deref(),
                None,
                None,
                None,
                None,
                Some(SortDirection::Asc),
            )
        })?
        .into_iter()
        .map(Into::into)
        .collect();

    let maybe_existing = installed_packages.first().cloned();

    let packages: Vec<Package> = find_packages(metadata_mgr, query, &maybe_existing)?;

    let packages: Vec<Package> = if let Some(ref version) = query.version {
        packages
            .into_iter()
            .filter(|p| p.has_version(version))
            .collect()
    } else {
        packages
    };

    match packages.len() {
        0 => Ok(ResolveResult::NotFound(package_name.to_string())),
        1 => {
            let pkg = packages.into_iter().next().unwrap();
            let installed_pkg = installed_packages.iter().find(|ip| ip.is_installed);

            if let Some(installed) = installed_pkg {
                if !options.force {
                    return Ok(ResolveResult::AlreadyInstalled {
                        pkg_name: installed.pkg_name.clone(),
                        pkg_id: installed.pkg_id.clone(),
                        repo_name: installed.repo_name.clone(),
                        version: installed.version.clone(),
                    });
                }
            }

            let existing_install = installed_packages
                .iter()
                .find(|ip| ip.version == pkg.version)
                .cloned();

            let pkg = pkg.resolve(query.version.as_deref());

            Ok(ResolveResult::Resolved(vec![InstallTarget {
                package: pkg,
                existing_install,
                pinned: query.version.is_some(),
                profile: None,
                ..Default::default()
            }]))
        }
        _ => {
            Ok(ResolveResult::Ambiguous(crate::AmbiguousPackage {
                query: package_name.to_string(),
                candidates: packages,
            }))
        }
    }
}

fn find_packages(
    metadata_mgr: &MetadataManager,
    query: &PackageQuery,
    existing_install: &Option<InstalledPackage>,
) -> SoarResult<Vec<Package>> {
    // If we have an existing install, try to find it in its original repo first
    if let Some(existing) = existing_install {
        let existing_pkgs: Vec<Package> = metadata_mgr
            .query_repo(&existing.repo_name, |conn| {
                MetadataRepository::find_filtered(
                    conn,
                    Some(&existing.pkg_name),
                    Some(&existing.pkg_id),
                    None,
                    None,
                    None,
                )
            })?
            .unwrap_or_default()
            .into_iter()
            .map(|p| {
                let mut pkg: Package = p.into();
                pkg.repo_name = existing.repo_name.clone();
                pkg
            })
            .collect();

        if !existing_pkgs.is_empty() {
            return Ok(existing_pkgs);
        }
    }

    if let Some(ref repo_name) = query.repo_name {
        Ok(metadata_mgr
            .query_repo(repo_name, |conn| {
                MetadataRepository::find_filtered(
                    conn,
                    query.name.as_deref(),
                    query.pkg_id.as_deref(),
                    None,
                    None,
                    None,
                )
            })?
            .unwrap_or_default()
            .into_iter()
            .map(|p| {
                let mut pkg: Package = p.into();
                pkg.repo_name = repo_name.clone();
                pkg
            })
            .collect())
    } else {
        metadata_mgr.query_all_flat(|repo_name, conn| {
            let pkgs = MetadataRepository::find_filtered(
                conn,
                query.name.as_deref(),
                query.pkg_id.as_deref(),
                None,
                None,
                None,
            )?;
            Ok(pkgs
                .into_iter()
                .map(|p| {
                    let mut pkg: Package = p.into();
                    pkg.repo_name = repo_name.to_string();
                    pkg
                })
                .collect())
        })
    }
}

/// Install resolved targets. Emits events through the context's event sink.
///
/// Handles concurrency control, download, verification, symlink creation,
/// desktop integration, and database recording.
pub async fn perform_installation(
    ctx: &SoarContext,
    targets: Vec<InstallTarget>,
    options: &InstallOptions,
) -> SoarResult<InstallReport> {
    debug!(count = targets.len(), "performing installation");
    let diesel_db = ctx.diesel_core_db()?.clone();
    let parallel_limit = ctx.config().parallel_limit.unwrap_or(4);
    let semaphore = Arc::new(Semaphore::new(parallel_limit as usize));

    let installed = Arc::new(Mutex::new(Vec::new()));
    let failed = Arc::new(Mutex::new(Vec::new()));
    let warnings = Arc::new(Mutex::new(Vec::new()));

    let total = targets.len() as u32;
    let completed = Arc::new(AtomicU32::new(0));
    let failed_count = Arc::new(AtomicU32::new(0));

    let mut handles = Vec::new();

    for target in targets {
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let ctx = ctx.clone();
        let db = diesel_db.clone();
        let installed = installed.clone();
        let failed = failed.clone();
        let warnings = warnings.clone();
        let completed = completed.clone();
        let failed_count = failed_count.clone();
        let binary_only = options.binary_only;
        let no_verify = options.no_verify;
        let portable = options.portable.clone();
        let portable_home = options.portable_home.clone();
        let portable_config = options.portable_config.clone();
        let portable_share = options.portable_share.clone();
        let portable_cache = options.portable_cache.clone();

        let handle = tokio::spawn(async move {
            let result = install_single_package(
                &ctx,
                &target,
                db.clone(),
                binary_only,
                no_verify,
                portable.as_deref(),
                portable_home.as_deref(),
                portable_config.as_deref(),
                portable_share.as_deref(),
                portable_cache.as_deref(),
            )
            .await;

            match result {
                Ok((install_dir, symlinks)) => {
                    if !install_dir.as_os_str().is_empty() {
                        installed.lock().unwrap().push(InstalledInfo {
                            pkg_name: target.package.pkg_name.clone(),
                            pkg_id: target.package.pkg_id.clone(),
                            repo_name: target.package.repo_name.clone(),
                            version: target.package.version.clone(),
                            install_dir,
                            symlinks,
                            notes: target.package.notes.clone(),
                        });
                    }
                    let _ = remove_old_versions(&target.package, &db, false);
                }
                Err(err) => {
                    match err {
                        SoarError::Warning(msg) => {
                            warnings.lock().unwrap().push(msg);
                            let _ = remove_old_versions(&target.package, &db, false);
                        }
                        _ => {
                            let op_id = next_op_id();
                            ctx.events().emit(SoarEvent::OperationFailed {
                                op_id,
                                pkg_name: target.package.pkg_name.clone(),
                                pkg_id: target.package.pkg_id.clone(),
                                error: err.to_string(),
                            });
                            failed.lock().unwrap().push(FailedInfo {
                                pkg_name: target.package.pkg_name.clone(),
                                pkg_id: target.package.pkg_id.clone(),
                                error: err.to_string(),
                            });
                            failed_count.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
            }

            let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
            ctx.events().emit(SoarEvent::BatchProgress {
                completed: done,
                total,
                failed: failed_count.load(Ordering::Relaxed),
            });

            drop(permit);
        });
        handles.push(handle);
    }

    for handle in handles {
        handle
            .await
            .map_err(|err| SoarError::Custom(format!("Join handle error: {err}")))?;
    }

    let installed = Arc::try_unwrap(installed).unwrap().into_inner().unwrap();
    let failed = Arc::try_unwrap(failed).unwrap().into_inner().unwrap();
    let warnings = Arc::try_unwrap(warnings).unwrap().into_inner().unwrap();

    Ok(InstallReport {
        installed,
        failed,
        warnings,
    })
}

#[allow(clippy::too_many_arguments)]
async fn install_single_package(
    ctx: &SoarContext,
    target: &InstallTarget,
    core_db: DieselDatabase,
    binary_only: bool,
    no_verify: bool,
    portable: Option<&str>,
    portable_home: Option<&str>,
    portable_config: Option<&str>,
    portable_share: Option<&str>,
    portable_cache: Option<&str>,
) -> SoarResult<(PathBuf, Vec<(PathBuf, PathBuf)>)> {
    let op_id = next_op_id();
    let events = ctx.events().clone();
    let pkg = &target.package;

    debug!(
        pkg_name = pkg.pkg_name,
        pkg_id = pkg.pkg_id,
        version = pkg.version,
        "installing package"
    );

    // Acquire lock
    let mut lock_attempts = 0;
    let _package_lock = loop {
        match FileLock::try_acquire(&pkg.pkg_name) {
            Ok(Some(lock)) => break Ok(lock),
            Ok(None) => {
                lock_attempts += 1;
                if lock_attempts == 1 {
                    debug!("waiting for lock on '{}'", pkg.pkg_name);
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            Err(err) => break Err(err),
        }
    }
    .map_err(|e| SoarError::Custom(format!("Failed to acquire package lock: {}", e)))?;

    // Re-check if package is already installed after acquiring lock
    let freshly_installed = core_db
        .with_conn(|conn| {
            CoreRepository::list_filtered(
                conn,
                Some(&pkg.repo_name),
                Some(&pkg.pkg_name),
                Some(&pkg.pkg_id),
                Some(&pkg.version),
                Some(true),
                None,
                None,
                Some(SortDirection::Asc),
            )
        })?
        .into_iter()
        .find(|ip| ip.is_installed);

    if freshly_installed.is_some() {
        return Ok((PathBuf::new(), Vec::new()));
    }

    let config = get_config();
    let bin_dir = config.get_bin_path()?;

    let dir_suffix: String = pkg
        .bsum
        .as_ref()
        .filter(|s| s.len() >= 12)
        .map(|s| s[..12].to_string())
        .unwrap_or_else(|| {
            let input = format!("{}:{}:{}", pkg.pkg_id, pkg.pkg_name, pkg.version);
            hash_string(&input)[..12].to_string()
        });

    let install_dir = config
        .get_packages_path(target.profile.clone())
        .unwrap()
        .join(format!("{}-{}-{}", pkg.pkg_name, pkg.pkg_id, dir_suffix));
    let real_bin = install_dir.join(&pkg.pkg_name);

    let (
        unlinked,
        eff_portable,
        eff_portable_home,
        eff_portable_config,
        eff_portable_share,
        eff_portable_cache,
        excludes,
    ) = if let Some(ref existing) = target.existing_install {
        (
            existing.unlinked,
            existing.portable_path.as_deref(),
            existing.portable_home.as_deref(),
            existing.portable_config.as_deref(),
            existing.portable_share.as_deref(),
            existing.portable_cache.as_deref(),
            existing.install_patterns.as_deref(),
        )
    } else {
        (
            false,
            portable,
            portable_home,
            portable_config,
            portable_share,
            portable_cache,
            None,
        )
    };

    let should_cleanup = if let Some(ref existing) = target.existing_install {
        if existing.is_installed {
            true
        } else {
            match InstallMarker::read_from_dir(&install_dir) {
                Some(marker) => !marker.matches_package(pkg),
                None => true,
            }
        }
    } else {
        false
    };

    if should_cleanup && install_dir.exists() {
        debug!(path = %install_dir.display(), "cleaning up existing installation directory");
        fs::remove_dir_all(&install_dir).map_err(|err| {
            SoarError::Custom(format!(
                "Failed to clean up install directory {}: {}",
                install_dir.display(),
                err
            ))
        })?;
    }

    let install_patterns = excludes.map(|e| e.to_vec()).unwrap_or_else(|| {
        if binary_only {
            let mut patterns = default_install_patterns();
            patterns.extend(
                ["!*.png", "!*.svg", "!*.desktop", "!LICENSE", "!CHECKSUM"]
                    .iter()
                    .map(ToString::to_string),
            );
            patterns
        } else {
            config.install_patterns.clone().unwrap_or_default()
        }
    });
    let install_patterns = apply_sig_variants(install_patterns);

    // Create progress bridge for download events
    let progress_callback = create_progress_bridge(
        events.clone(),
        op_id,
        pkg.pkg_name.clone(),
        pkg.pkg_id.clone(),
    );

    trace!(install_dir = %install_dir.display(), "creating package installer");
    let installer = PackageInstaller::new(
        target,
        &install_dir,
        Some(progress_callback),
        core_db.clone(),
        install_patterns.to_vec(),
    )
    .await?;

    // Download
    let downloaded_checksum = installer.download_package().await?;

    // Signature verification
    if let Some(repository) = config.get_repository(&pkg.repo_name) {
        if repository.signature_verification() {
            events.emit(SoarEvent::Verifying {
                op_id,
                pkg_name: pkg.pkg_name.clone(),
                pkg_id: pkg.pkg_id.clone(),
                stage: VerifyStage::Signature,
            });

            let repository_path = repository.get_path()?;
            let pubkey_file = repository_path.join("minisign.pub");
            if pubkey_file.exists() {
                verify_signatures(&pubkey_file, &install_dir)?;
            } else {
                warn!(
                    "{}#{} - Signature verification skipped as no pubkey was found.",
                    pkg.pkg_name, pkg.pkg_id
                );
            }
        }
    } else {
        // Clean up .sig files for packages without signature verification
        cleanup_sig_files(&install_dir);
    }

    // Checksum verification
    if pkg.provides.is_some() && !no_verify {
        events.emit(SoarEvent::Verifying {
            op_id,
            pkg_name: pkg.pkg_name.clone(),
            pkg_id: pkg.pkg_id.clone(),
            stage: VerifyStage::Checksum,
        });

        let final_checksum = if pkg.ghcr_pkg.is_some() {
            if real_bin.exists() {
                Some(calculate_checksum(&real_bin)?)
            } else {
                None
            }
        } else {
            downloaded_checksum
        };

        match (final_checksum, pkg.bsum.as_ref()) {
            (Some(calculated), Some(expected)) if calculated != *expected => {
                events.emit(SoarEvent::Verifying {
                    op_id,
                    pkg_name: pkg.pkg_name.clone(),
                    pkg_id: pkg.pkg_id.clone(),
                    stage: VerifyStage::Failed("checksum mismatch".into()),
                });
                return Err(SoarError::Custom(format!(
                    "{}#{} - Invalid checksum, skipped installation.",
                    pkg.pkg_name, pkg.pkg_id
                )));
            }
            (Some(ref calculated), Some(expected)) if calculated == expected => {
                events.emit(SoarEvent::Verifying {
                    op_id,
                    pkg_name: pkg.pkg_name.clone(),
                    pkg_id: pkg.pkg_id.clone(),
                    stage: VerifyStage::Passed,
                });
            }
            _ => {}
        }
    }

    // Create symlinks
    events.emit(SoarEvent::Installing {
        op_id,
        pkg_name: pkg.pkg_name.clone(),
        pkg_id: pkg.pkg_id.clone(),
        stage: InstallStage::LinkingBinaries,
    });

    let symlinks = mangle_package_symlinks(
        &install_dir,
        &bin_dir,
        pkg.provides.as_deref(),
        &pkg.pkg_name,
        &pkg.version,
        target.entrypoint.as_deref(),
        target.binaries.as_deref(),
    )
    .await?;

    // Desktop integration
    if !unlinked || has_desktop_integration(pkg, ctx.config()) {
        events.emit(SoarEvent::Installing {
            op_id,
            pkg_name: pkg.pkg_name.clone(),
            pkg_id: pkg.pkg_id.clone(),
            stage: InstallStage::DesktopIntegration,
        });

        let actual_bin = symlinks.first().map(|(src, _)| src.as_path());
        integrate_package(
            &install_dir,
            pkg,
            actual_bin,
            eff_portable,
            eff_portable_home,
            eff_portable_config,
            eff_portable_share,
            eff_portable_cache,
        )
        .await?;
    }

    // Record to database
    events.emit(SoarEvent::Installing {
        op_id,
        pkg_name: pkg.pkg_name.clone(),
        pkg_id: pkg.pkg_id.clone(),
        stage: InstallStage::RecordingDatabase,
    });

    installer
        .record(
            unlinked,
            eff_portable,
            eff_portable_home,
            eff_portable_config,
            eff_portable_share,
            eff_portable_cache,
        )
        .await?;

    installer.run_post_install_hook()?;

    events.emit(SoarEvent::OperationComplete {
        op_id,
        pkg_name: pkg.pkg_name.clone(),
        pkg_id: pkg.pkg_id.clone(),
    });

    debug!(
        pkg_name = pkg.pkg_name,
        pkg_id = pkg.pkg_id,
        version = pkg.version,
        "installation complete"
    );
    Ok((install_dir, symlinks))
}

fn verify_signatures(pubkey_file: &Path, install_dir: &Path) -> SoarResult<()> {
    let pubkey = PublicKey::from_base64(
        fs::read_to_string(pubkey_file)
            .with_context(|| format!("reading minisign key from {}", pubkey_file.display()))?
            .trim(),
    )
    .map_err(|err| {
        SoarError::Custom(format!(
            "Failed to load public key from {}: {}",
            pubkey_file.display(),
            err
        ))
    })?;

    let entries = fs::read_dir(install_dir)
        .with_context(|| format!("reading package directory {}", install_dir.display()))?;

    for entry in entries {
        let path = entry
            .with_context(|| format!("reading entry from directory {}", install_dir.display()))?
            .path();
        let is_signature_file = path.extension().is_some_and(|ext| ext == "sig");
        let original_file = path.with_extension("");
        if is_signature_file && path.is_file() && original_file.is_file() {
            let signature = Signature::from_file(&path).map_err(|err| {
                SoarError::Custom(format!(
                    "Failed to load signature file from {}: {}",
                    path.display(),
                    err
                ))
            })?;
            let mut stream_verifier = pubkey.verify_stream(&signature).map_err(|err| {
                SoarError::Custom(format!("Failed to setup stream verifier: {err}"))
            })?;

            let file = File::open(&original_file).with_context(|| {
                format!(
                    "opening file {} for signature verification",
                    original_file.display()
                )
            })?;
            let mut buf_reader = BufReader::new(file);

            let mut buffer = [0u8; 8192];
            loop {
                match buf_reader.read(&mut buffer).with_context(|| {
                    format!("reading to buffer from {}", original_file.display())
                })? {
                    0 => break,
                    n => {
                        stream_verifier.update(&buffer[..n]);
                    }
                }
            }

            stream_verifier.finalize().map_err(|_| {
                SoarError::Custom(format!(
                    "Signature verification failed for {}",
                    original_file.display()
                ))
            })?;

            fs::remove_file(&path)
                .with_context(|| format!("removing minisign file {}", path.display()))?;
        }
    }

    Ok(())
}

fn cleanup_sig_files(install_dir: &Path) {
    if let Ok(entries) = fs::read_dir(install_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "sig") && path.is_file() {
                fs::remove_file(&path).ok();
            }
        }
    }
}
