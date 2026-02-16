use std::collections::HashSet;

use soar_config::packages::{PackagesConfig, ResolvedPackage};
use soar_core::{
    database::{
        connection::DieselDatabase,
        models::{InstalledPackage, Package},
    },
    package::{
        install::InstallTarget,
        release_source::{run_version_command, ReleaseSource},
        remove::PackageRemover,
        url::UrlPackage,
    },
    utils::substitute_placeholders,
    SoarResult,
};
use soar_db::repository::{
    core::{CoreRepository, SortDirection},
    metadata::MetadataRepository,
};
use soar_events::{RemoveStage, SoarEvent};
use tracing::{debug, warn};

use crate::{
    install::perform_installation, progress::next_op_id, utils::get_package_hooks, ApplyDiff,
    ApplyReport, InstallOptions, SoarContext,
};

/// Status of a URL package compared against installed packages.
enum UrlPackageStatus {
    ToInstall(InstallTarget),
    ToUpdate(InstallTarget),
    InSync(String),
}

/// Compute the difference between declared packages (from packages.toml) and
/// installed packages.
///
/// If `prune` is true, packages installed but not declared will be listed for removal.
pub async fn compute_diff(
    ctx: &SoarContext,
    resolved: &[ResolvedPackage],
    prune: bool,
) -> SoarResult<ApplyDiff> {
    debug!(
        count = resolved.len(),
        prune = prune,
        "computing apply diff"
    );
    let metadata_mgr = ctx.metadata_manager().await?;
    let diesel_db = ctx.diesel_core_db()?.clone();

    let mut diff = ApplyDiff::default();
    let mut declared_keys: HashSet<(String, Option<String>, Option<String>)> = HashSet::new();

    for pkg in resolved {
        declared_keys.insert((pkg.name.clone(), pkg.pkg_id.clone(), pkg.repo.clone()));

        let is_github_or_gitlab = pkg.github.is_some() || pkg.gitlab.is_some();
        if is_github_or_gitlab || pkg.url.is_some() {
            handle_local_package(pkg, is_github_or_gitlab, &diesel_db, &mut diff)?;
            continue;
        }

        // Find package in metadata
        let found_packages: Vec<Package> = if let Some(ref repo_name) = pkg.repo {
            metadata_mgr
                .query_repo(repo_name, |conn| {
                    MetadataRepository::find_filtered(
                        conn,
                        Some(&pkg.name),
                        pkg.pkg_id.as_deref(),
                        pkg.version.as_deref(),
                        None,
                        Some(SortDirection::Asc),
                    )
                })?
                .unwrap_or_default()
                .into_iter()
                .map(|p| {
                    let mut package: Package = p.into();
                    package.repo_name = repo_name.clone();
                    package
                })
                .collect()
        } else {
            metadata_mgr.query_all_flat(|repo_name, conn| {
                let pkgs = MetadataRepository::find_filtered(
                    conn,
                    Some(&pkg.name),
                    pkg.pkg_id.as_deref(),
                    pkg.version.as_deref(),
                    None,
                    Some(SortDirection::Asc),
                )?;
                Ok(pkgs
                    .into_iter()
                    .map(|p| {
                        let mut package: Package = p.into();
                        package.repo_name = repo_name.to_string();
                        package
                    })
                    .collect())
            })?
        };

        if found_packages.is_empty() {
            diff.not_found.push(pkg.name.clone());
            continue;
        }

        let metadata_pkg = found_packages.into_iter().next().unwrap();

        let installed_packages: Vec<InstalledPackage> = diesel_db
            .with_conn(|conn| {
                CoreRepository::list_filtered(
                    conn,
                    Some(&metadata_pkg.repo_name),
                    Some(&metadata_pkg.pkg_name),
                    Some(&metadata_pkg.pkg_id),
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

        let existing_install = installed_packages.into_iter().find(|ip| ip.is_installed);

        if let Some(ref existing) = existing_install {
            let version_matches = pkg.version.as_ref().is_none_or(|v| existing.version == *v);

            if version_matches && existing.version == metadata_pkg.version {
                diff.in_sync.push(format!(
                    "{}#{}@{}",
                    existing.pkg_name, existing.pkg_id, existing.version
                ));
            } else if !existing.pinned || pkg.version.is_some() {
                let target = create_install_target(pkg, metadata_pkg, Some(existing.clone()));
                diff.to_update.push((pkg.clone(), target));
            } else {
                diff.in_sync.push(format!(
                    "{}#{}@{} (pinned)",
                    existing.pkg_name, existing.pkg_id, existing.version
                ));
            }
        } else {
            let target = create_install_target(pkg, metadata_pkg, None);
            diff.to_install.push((pkg.clone(), target));
        }
    }

    if prune {
        let all_installed: Vec<InstalledPackage> = diesel_db
            .with_conn(|conn| {
                CoreRepository::list_filtered(
                    conn,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(SortDirection::Asc),
                )
            })?
            .into_iter()
            .filter(|p| p.is_installed)
            .map(Into::into)
            .collect();

        for installed in all_installed {
            let is_declared = declared_keys.iter().any(|(name, pkg_id, repo)| {
                let name_matches = *name == installed.pkg_name;
                let pkg_id_matches = pkg_id.as_ref().is_none_or(|id| *id == installed.pkg_id);
                let repo_matches = repo.as_ref().is_none_or(|r| *r == installed.repo_name);
                name_matches && pkg_id_matches && repo_matches
            });

            if !is_declared {
                diff.to_remove.push(installed);
            }
        }
    }

    Ok(diff)
}

/// Execute an apply operation from a computed diff.
///
/// Installs new packages, updates existing ones, removes pruned ones,
/// and updates packages.toml version entries.
pub async fn execute_apply(
    ctx: &SoarContext,
    diff: ApplyDiff,
    no_verify: bool,
) -> SoarResult<ApplyReport> {
    debug!("executing apply");
    let diesel_db = ctx.diesel_core_db()?.clone();

    let mut installed_count = 0;
    let mut updated_count = 0;
    let mut removed_count = 0;
    let mut failed_count = 0;

    // Apply pending version updates for in-sync packages
    for (pkg_name, version) in &diff.pending_version_updates {
        if let Err(e) = PackagesConfig::update_package(pkg_name, None, Some(version), None) {
            warn!(
                "Failed to update version for '{}' in packages.toml: {}",
                pkg_name, e
            );
        }
    }

    // Install new packages
    if !diff.to_install.is_empty() {
        let mut version_updates: Vec<(String, String)> = Vec::new();
        for (pkg, target) in &diff.to_install {
            let declared_version = pkg
                .version
                .as_ref()
                .map(|v| v.strip_prefix('v').unwrap_or(v));
            if declared_version != Some(target.package.version.as_str()) {
                version_updates.push((pkg.name.clone(), target.package.version.clone()));
            }
        }

        let targets: Vec<InstallTarget> = diff
            .to_install
            .into_iter()
            .map(|(_, target)| target)
            .collect();

        let options = InstallOptions {
            no_verify,
            ..Default::default()
        };

        let report = perform_installation(ctx, targets, &options).await?;
        installed_count = report.installed.len();
        failed_count += report.failed.len();

        let succeeded: HashSet<&str> = report
            .installed
            .iter()
            .map(|i| i.pkg_name.as_str())
            .collect();
        for (pkg_name, version) in &version_updates {
            if succeeded.contains(pkg_name.as_str()) {
                if let Err(e) = PackagesConfig::update_package(pkg_name, None, Some(version), None)
                {
                    warn!(
                        "Failed to update version for '{}' in packages.toml: {}",
                        pkg_name, e
                    );
                }
            }
        }
    }

    // Update packages
    if !diff.to_update.is_empty() {
        let mut update_version_updates: Vec<(String, String)> = Vec::new();
        for (pkg, target) in &diff.to_update {
            let declared_version = pkg
                .version
                .as_ref()
                .map(|v| v.strip_prefix('v').unwrap_or(v));
            if declared_version != Some(target.package.version.as_str()) {
                update_version_updates.push((pkg.name.clone(), target.package.version.clone()));
            }
        }

        let targets: Vec<InstallTarget> = diff
            .to_update
            .into_iter()
            .map(|(_, target)| target)
            .collect();

        let options = InstallOptions {
            no_verify,
            ..Default::default()
        };

        let report = perform_installation(ctx, targets, &options).await?;
        updated_count = report.installed.len();
        failed_count += report.failed.len();

        let succeeded: HashSet<&str> = report
            .installed
            .iter()
            .map(|i| i.pkg_name.as_str())
            .collect();
        for (pkg_name, version) in &update_version_updates {
            if succeeded.contains(pkg_name.as_str()) {
                if let Err(e) = PackagesConfig::update_package(pkg_name, None, Some(version), None)
                {
                    warn!(
                        "Failed to update version for '{}' in packages.toml: {}",
                        pkg_name, e
                    );
                }
            }
        }
    }

    // Remove pruned packages
    if !diff.to_remove.is_empty() {
        for pkg in diff.to_remove {
            let op_id = next_op_id();
            ctx.events().emit(SoarEvent::Removing {
                op_id,
                pkg_name: pkg.pkg_name.clone(),
                pkg_id: pkg.pkg_id.clone(),
                stage: RemoveStage::RunningHook("pre_remove".into()),
            });

            let (hooks, sandbox) = get_package_hooks(&pkg.pkg_name);
            match PackageRemover::new(pkg.clone(), diesel_db.clone(), ctx.config().clone())
                .await
                .with_hooks(hooks)
                .with_sandbox(sandbox)
                .remove()
                .await
            {
                Ok(()) => {
                    ctx.events().emit(SoarEvent::Removing {
                        op_id,
                        pkg_name: pkg.pkg_name.clone(),
                        pkg_id: pkg.pkg_id.clone(),
                        stage: RemoveStage::Complete {
                            size_freed: None,
                        },
                    });
                    removed_count += 1;
                }
                Err(e) => {
                    ctx.events().emit(SoarEvent::OperationFailed {
                        op_id,
                        pkg_name: pkg.pkg_name.clone(),
                        pkg_id: pkg.pkg_id.clone(),
                        error: e.to_string(),
                    });
                    failed_count += 1;
                }
            }
        }
    }

    Ok(ApplyReport {
        installed_count,
        updated_count,
        removed_count,
        failed_count,
    })
}

/// Handle local (URL/github/gitlab) packages in apply diff.
fn handle_local_package(
    pkg: &ResolvedPackage,
    is_github_or_gitlab: bool,
    diesel_db: &DieselDatabase,
    diff: &mut ApplyDiff,
) -> SoarResult<()> {
    let local_pkg_id = if is_github_or_gitlab {
        pkg.pkg_id.clone().or_else(|| {
            pkg.github
                .as_ref()
                .or(pkg.gitlab.as_ref())
                .map(|repo| repo.replace('/', "."))
        })
    } else {
        pkg.pkg_id.clone()
    };

    let installed: Option<InstalledPackage> = diesel_db
        .with_conn(|conn| {
            CoreRepository::list_filtered(
                conn,
                Some("local"),
                Some(&pkg.name),
                local_pkg_id.as_deref(),
                None,
                Some(true),
                None,
                Some(1),
                None,
            )
        })?
        .into_iter()
        .next()
        .map(Into::into);

    // Handle version_command packages
    if let Some(ref cmd) = pkg.version_command {
        if let Some(ref declared) = pkg.version {
            let normalized = declared.strip_prefix('v').unwrap_or(declared);
            if let Some(ref existing) = installed {
                if existing.version == normalized {
                    diff.in_sync.push(format!("{} (local)", pkg.name));
                    return Ok(());
                }
            }
        }

        let result = match run_version_command(cmd) {
            Ok(r) => r,
            Err(e) => {
                warn!("Failed to run version_command for {}: {}", pkg.name, e);
                diff.not_found
                    .push(format!("{} (version_command failed: {})", pkg.name, e));
                return Ok(());
            }
        };

        let version = result
            .version
            .strip_prefix('v')
            .unwrap_or(&result.version)
            .to_string();

        if let Some(ref existing) = installed {
            if existing.version == version {
                let declared = pkg
                    .version
                    .as_ref()
                    .map(|s| s.strip_prefix('v').unwrap_or(s));
                if declared != Some(version.as_str()) {
                    diff.pending_version_updates
                        .push((pkg.name.clone(), version.clone()));
                }
                diff.in_sync.push(format!("{} (local)", pkg.name));
                return Ok(());
            }
        }

        let download_url = match result.download_url {
            Some(url) => url,
            None => {
                match &pkg.url {
                    Some(url) => substitute_placeholders(url, Some(&version)),
                    None => {
                        diff.not_found.push(format!(
                            "{} (version_command returned no URL and no url field configured)",
                            pkg.name
                        ));
                        return Ok(());
                    }
                }
            }
        };

        let mut url_pkg = UrlPackage::from_remote(
            &download_url,
            Some(&pkg.name),
            Some(&version),
            pkg.pkg_type.as_deref(),
            local_pkg_id.as_deref(),
        )?;
        url_pkg.size = result.size;

        match check_url_package_status(&url_pkg, pkg, "local", diesel_db)? {
            UrlPackageStatus::ToInstall(target) => diff.to_install.push((pkg.clone(), target)),
            UrlPackageStatus::ToUpdate(target) => diff.to_update.push((pkg.clone(), target)),
            UrlPackageStatus::InSync(label) => diff.in_sync.push(label),
        }
        return Ok(());
    }

    // Handle github/gitlab packages
    if is_github_or_gitlab {
        if let Some(ref declared) = pkg.version {
            let normalized = declared.strip_prefix('v').unwrap_or(declared);
            if let Some(ref existing) = installed {
                if existing.version == normalized {
                    diff.in_sync.push(format!("{} (local)", pkg.name));
                    return Ok(());
                }
            }
        }

        let source = match ReleaseSource::from_resolved(pkg) {
            Some(s) => s,
            None => {
                diff.not_found.push(format!(
                    "{} (missing asset_pattern for github/gitlab source)",
                    pkg.name
                ));
                return Ok(());
            }
        };
        let release = match source.resolve_version(pkg.version.as_deref()) {
            Ok(r) => r,
            Err(e) => {
                warn!("Failed to resolve release for {}: {}", pkg.name, e);
                diff.not_found.push(format!("{} ({})", pkg.name, e));
                return Ok(());
            }
        };
        let version = release
            .version
            .strip_prefix('v')
            .unwrap_or(&release.version)
            .to_string();

        let url_pkg = UrlPackage::from_remote(
            &release.download_url,
            Some(&pkg.name),
            Some(&version),
            pkg.pkg_type.as_deref(),
            local_pkg_id.as_deref(),
        )?;

        match check_url_package_status(&url_pkg, pkg, "local", diesel_db)? {
            UrlPackageStatus::ToInstall(target) => diff.to_install.push((pkg.clone(), target)),
            UrlPackageStatus::ToUpdate(target) => diff.to_update.push((pkg.clone(), target)),
            UrlPackageStatus::InSync(label) => diff.in_sync.push(label),
        }
        return Ok(());
    }

    // Handle plain URL packages
    if let Some(ref url) = pkg.url {
        if let Some(ref declared) = pkg.version {
            let normalized = declared.strip_prefix('v').unwrap_or(declared);
            if let Some(ref existing) = installed {
                if existing.version == normalized {
                    diff.in_sync.push(format!("{} (local)", pkg.name));
                    return Ok(());
                }
            }
        }

        let url = substitute_placeholders(url, pkg.version.as_deref());
        let url_pkg = UrlPackage::from_remote(
            &url,
            Some(&pkg.name),
            pkg.version.as_deref(),
            pkg.pkg_type.as_deref(),
            pkg.pkg_id.as_deref(),
        )?;

        match check_url_package_status(&url_pkg, pkg, "local", diesel_db)? {
            UrlPackageStatus::ToInstall(target) => diff.to_install.push((pkg.clone(), target)),
            UrlPackageStatus::ToUpdate(target) => diff.to_update.push((pkg.clone(), target)),
            UrlPackageStatus::InSync(label) => diff.in_sync.push(label),
        }
    }

    Ok(())
}

fn check_url_package_status(
    url_pkg: &UrlPackage,
    pkg: &ResolvedPackage,
    display_label: &str,
    diesel_db: &DieselDatabase,
) -> SoarResult<UrlPackageStatus> {
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

    let installed = installed_packages
        .iter()
        .find(|ip| ip.is_installed)
        .cloned();

    if let Some(ref existing) = installed {
        if url_pkg.version != existing.version {
            let target = create_url_install_target(url_pkg, pkg, installed);
            Ok(UrlPackageStatus::ToUpdate(target))
        } else {
            Ok(UrlPackageStatus::InSync(format!(
                "{} ({})",
                pkg.name, display_label
            )))
        }
    } else {
        let existing_install = installed_packages.into_iter().next();
        let target = create_url_install_target(url_pkg, pkg, existing_install);
        Ok(UrlPackageStatus::ToInstall(target))
    }
}

fn create_install_target(
    resolved: &ResolvedPackage,
    package: Package,
    existing: Option<InstalledPackage>,
) -> InstallTarget {
    InstallTarget {
        package,
        existing_install: existing,
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
    }
}

fn create_url_install_target(
    url_pkg: &UrlPackage,
    resolved: &ResolvedPackage,
    existing: Option<InstalledPackage>,
) -> InstallTarget {
    InstallTarget {
        package: url_pkg.to_package(),
        existing_install: existing,
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
    }
}
