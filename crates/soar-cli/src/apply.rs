use std::{
    collections::HashSet,
    io::{self, Write},
    sync::atomic::Ordering,
};

use nu_ansi_term::Color::{Blue, Cyan, Green, Magenta, Red, Yellow};
use soar_config::packages::{PackagesConfig, ResolvedPackage};
use soar_core::{
    database::models::{InstalledPackage, Package},
    package::{install::InstallTarget, remove::PackageRemover, url::UrlPackage},
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
    install::{create_install_context, perform_installation},
    state::AppState,
    update::perform_update,
    utils::{display_settings, icon_or, Colored, Icons},
};

/// Result of comparing declared packages vs installed packages
#[derive(Default)]
pub struct ApplyDiff {
    /// Packages to install (declared but not installed)
    pub to_install: Vec<(ResolvedPackage, InstallTarget)>,
    /// Packages to update (version mismatch)
    pub to_update: Vec<(ResolvedPackage, InstallTarget)>,
    /// Packages to remove (installed but not declared, only with --prune)
    pub to_remove: Vec<InstalledPackage>,
    /// Packages already in sync
    pub in_sync: Vec<String>,
    /// Packages not found in metadata
    pub not_found: Vec<String>,
}

/// Main entry point for the apply command
pub async fn apply_packages(
    prune: bool,
    dry_run: bool,
    yes: bool,
    packages_config: Option<String>,
    no_verify: bool,
) -> SoarResult<()> {
    let config = PackagesConfig::load(packages_config.as_deref())?;
    let resolved = config.resolved_packages();

    if resolved.is_empty() {
        info!("No packages declared in configuration");
        return Ok(());
    }

    info!("Loaded {} package declaration(s)", resolved.len());

    let state = AppState::new();
    let diff = compute_diff(&state, &resolved, prune).await?;

    display_diff(&diff, prune);

    if diff.to_install.is_empty() && diff.to_update.is_empty() && diff.to_remove.is_empty() {
        info!("\nAll packages are in sync!");
        return Ok(());
    }

    if dry_run {
        info!("\n{} Dry run - no changes made", icon_or("", "[DRY RUN]"));
        return Ok(());
    }

    if !yes {
        print!("\nProceed? [y/N] ");
        io::stdout().flush().ok();
        let mut input = String::new();
        io::stdin().read_line(&mut input).ok();
        if !input.trim().eq_ignore_ascii_case("y") {
            info!("Aborted");
            return Ok(());
        }
    }

    execute_apply(&state, diff, no_verify).await
}

/// Compute the difference between declared and installed packages
async fn compute_diff(
    state: &AppState,
    resolved: &[ResolvedPackage],
    prune: bool,
) -> SoarResult<ApplyDiff> {
    let metadata_mgr = state.metadata_manager().await?;
    let diesel_db = state.diesel_core_db()?.clone();

    let mut diff = ApplyDiff::default();
    let mut declared_keys: HashSet<(String, Option<String>, Option<String>)> = HashSet::new();

    for pkg in resolved {
        // Track declared package
        declared_keys.insert((pkg.name.clone(), pkg.pkg_id.clone(), pkg.repo.clone()));

        // Handle URL packages
        if let Some(ref url) = pkg.url {
            let url_pkg = UrlPackage::from_url(
                url,
                Some(&pkg.name),
                pkg.version.as_deref(),
                pkg.pkg_type.as_deref(),
                pkg.pkg_id.as_deref(),
            )?;

            // Check if installed in core DB with repo_name="local"
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

            let is_already_installed = installed_packages.iter().any(|ip| ip.is_installed);

            let existing_install = installed_packages.into_iter().next();
            let target = InstallTarget {
                package: url_pkg.to_package(),
                existing_install: existing_install.clone(),
                with_pkg_id: url_pkg.pkg_type.is_some(),
                pinned: false,
                profile: pkg.profile.clone(),
                portable: pkg.portable.as_ref().and_then(|p| p.path.clone()),
                portable_home: pkg.portable.as_ref().and_then(|p| p.home.clone()),
                portable_config: pkg.portable.as_ref().and_then(|p| p.config.clone()),
                portable_share: pkg.portable.as_ref().and_then(|p| p.share.clone()),
                portable_cache: pkg.portable.as_ref().and_then(|p| p.cache.clone()),
            };

            if !is_already_installed {
                diff.to_install.push((pkg.clone(), target));
            } else if url_pkg.version != existing_install.unwrap().version {
                diff.to_update.push((pkg.clone(), target));
            } else {
                diff.in_sync.push(format!("{} (local)", pkg.name));
            }
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

        // Use first matching package (like --yes behavior)
        let metadata_pkg = found_packages.into_iter().next().unwrap();

        // Check if installed
        let installed_packages: Vec<InstalledPackage> = diesel_db
            .with_conn(|conn| {
                CoreRepository::list_filtered(
                    conn,
                    Some(&metadata_pkg.repo_name),
                    Some(&metadata_pkg.pkg_name),
                    Some(&metadata_pkg.pkg_id),
                    None, // Don't filter by version - we want to find any installed version
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
                let pkg_id_matches = pkg_id.as_ref().map_or(true, |id| *id == installed.pkg_id);
                let repo_matches = repo.as_ref().map_or(true, |r| *r == installed.repo_name);
                name_matches && pkg_id_matches && repo_matches
            });

            if !is_declared {
                diff.to_remove.push(installed);
            }
        }
    }

    Ok(diff)
}

/// Create an InstallTarget from resolved package info
fn create_install_target(
    resolved: &ResolvedPackage,
    package: Package,
    existing: Option<InstalledPackage>,
) -> InstallTarget {
    InstallTarget {
        package,
        existing_install: existing,
        with_pkg_id: resolved.pkg_id.is_some(),
        pinned: resolved.pinned,
        profile: resolved.profile.clone(),
        portable: resolved.portable.as_ref().and_then(|p| p.path.clone()),
        portable_home: resolved.portable.as_ref().and_then(|p| p.home.clone()),
        portable_config: resolved.portable.as_ref().and_then(|p| p.config.clone()),
        portable_share: resolved.portable.as_ref().and_then(|p| p.share.clone()),
        portable_cache: resolved.portable.as_ref().and_then(|p| p.cache.clone()),
    }
}

/// Display the computed diff
fn display_diff(diff: &ApplyDiff, prune: bool) {
    let settings = display_settings();
    let use_icons = settings.icons();

    // Build packages table if there are changes
    if !diff.to_install.is_empty()
        || !diff.to_update.is_empty()
        || (prune && !diff.to_remove.is_empty())
    {
        let mut builder = Builder::new();
        builder.push_record(["", "Package", "Version", "Repository"]);

        // Add packages to install
        for (_resolved, target) in &diff.to_install {
            let pkg = &target.package;
            builder.push_record([
                format!("{}", Colored(Green, icon_or("+", "+"))),
                format!(
                    "{}#{}",
                    Colored(Blue, &pkg.pkg_name),
                    Colored(Cyan, &pkg.pkg_id)
                ),
                format!("{}", Colored(Green, &pkg.version)),
                format!("{}", Colored(Magenta, &pkg.repo_name)),
            ]);
        }

        // Add packages to update
        for (_resolved, target) in &diff.to_update {
            let pkg = &target.package;
            let old_version = target
                .existing_install
                .as_ref()
                .map_or("?".to_string(), |e| e.version.clone());
            builder.push_record([
                format!("{}", Colored(Yellow, icon_or("~", "~"))),
                format!(
                    "{}#{}",
                    Colored(Blue, &pkg.pkg_name),
                    Colored(Cyan, &pkg.pkg_id)
                ),
                format!(
                    "{} -> {}",
                    Colored(Red, &old_version),
                    Colored(Green, &pkg.version)
                ),
                format!("{}", Colored(Magenta, &pkg.repo_name)),
            ]);
        }

        // Add packages to remove
        if prune {
            for pkg in &diff.to_remove {
                builder.push_record([
                    format!("{}", Colored(Red, icon_or("-", "-"))),
                    format!(
                        "{}#{}",
                        Colored(Blue, &pkg.pkg_name),
                        Colored(Cyan, &pkg.pkg_id)
                    ),
                    format!("{}", Colored(Yellow, &pkg.version)),
                    format!("{}", Colored(Magenta, &pkg.repo_name)),
                ]);
            }
        }

        let table = builder
            .build()
            .with(Panel::header("Package Changes"))
            .with(Style::rounded())
            .with(BorderCorrection {})
            .to_string();

        info!("\n{table}");
    }

    // Show packages not found
    if !diff.not_found.is_empty() {
        info!("\n{} Packages not found:", icon_or(Icons::WARNING, "!"));
        for name in &diff.not_found {
            warn!("  {} {}", icon_or("?", "?"), Colored(Yellow, name));
        }
    }

    // Summary table
    let mut summary_builder = Builder::new();

    if !diff.to_install.is_empty() {
        summary_builder.push_record([
            format!("{} To Install", icon_or("+", "+")),
            format!("{}", Colored(Green, diff.to_install.len())),
        ]);
    }
    if !diff.to_update.is_empty() {
        summary_builder.push_record([
            format!("{} To Update", icon_or("~", "~")),
            format!("{}", Colored(Yellow, diff.to_update.len())),
        ]);
    }
    if prune && !diff.to_remove.is_empty() {
        summary_builder.push_record([
            format!("{} To Remove", icon_or("-", "-")),
            format!("{}", Colored(Red, diff.to_remove.len())),
        ]);
    }
    if !diff.in_sync.is_empty() {
        summary_builder.push_record([
            format!("{} In Sync", icon_or(Icons::CHECK, "*")),
            format!("{}", Colored(Cyan, diff.in_sync.len())),
        ]);
    }
    if !diff.not_found.is_empty() {
        summary_builder.push_record([
            format!("{} Not Found", icon_or(Icons::WARNING, "?")),
            format!("{}", Colored(Yellow, diff.not_found.len())),
        ]);
    }

    if use_icons {
        let summary_table = summary_builder
            .build()
            .with(Panel::header("Summary"))
            .with(Style::rounded())
            .with(BorderCorrection {})
            .to_string();

        info!("\n{summary_table}");
    } else {
        let total_changes = diff.to_install.len() + diff.to_update.len() + diff.to_remove.len();
        if total_changes > 0 || !diff.in_sync.is_empty() {
            info!(
                "\nSummary: {} to install, {} to update, {} to remove, {} in sync",
                diff.to_install.len(),
                diff.to_update.len(),
                if prune { diff.to_remove.len() } else { 0 },
                diff.in_sync.len()
            );
        }
    }
}

/// Execute the apply operation
async fn execute_apply(state: &AppState, diff: ApplyDiff, no_verify: bool) -> SoarResult<()> {
    let diesel_db = state.diesel_core_db()?.clone();
    let config = state.config();

    let mut installed_count = 0;
    let mut updated_count = 0;
    let mut removed_count = 0;
    let mut failed_count = 0;

    if !diff.to_install.is_empty() {
        info!("\nInstalling {} package(s)...", diff.to_install.len());

        let targets: Vec<InstallTarget> = diff
            .to_install
            .into_iter()
            .map(|(_, target)| target)
            .collect();

        let ctx = create_install_context(
            targets.len(),
            config.parallel_limit.unwrap_or(4),
            None,
            None,
            None,
            None,
            None,
            false,
            no_verify,
        );

        perform_installation(ctx.clone(), targets, diesel_db.clone(), true).await?;
        installed_count = ctx.installed_count.load(Ordering::Relaxed) as usize;
        failed_count += ctx.failed.load(Ordering::Relaxed) as usize;
    }

    if !diff.to_update.is_empty() {
        info!("\nUpdating {} package(s)...", diff.to_update.len());

        let targets: Vec<InstallTarget> = diff
            .to_update
            .into_iter()
            .map(|(_, target)| target)
            .collect();

        let ctx = create_install_context(
            targets.len(),
            config.parallel_limit.unwrap_or(4),
            None,
            None,
            None,
            None,
            None,
            false,
            no_verify,
        );

        perform_update(ctx.clone(), targets, diesel_db.clone(), false).await?;
        updated_count = ctx.installed_count.load(Ordering::Relaxed) as usize;
        failed_count += ctx.failed.load(Ordering::Relaxed) as usize;
    }

    if !diff.to_remove.is_empty() {
        info!("\nRemoving {} package(s)...", diff.to_remove.len());

        for pkg in diff.to_remove {
            match PackageRemover::new(pkg.clone(), diesel_db.clone())
                .await
                .remove()
                .await
            {
                Ok(_) => {
                    info!("  Removed {}#{}", pkg.pkg_name, pkg.pkg_id);
                    removed_count += 1;
                }
                Err(e) => {
                    error!("  Failed to remove {}#{}: {}", pkg.pkg_name, pkg.pkg_id, e);
                    failed_count += 1;
                }
            }
        }
    }

    info!("\n{} Apply Summary", icon_or(Icons::CHECK, "*"));
    info!("  Installed: {}", installed_count);
    info!("  Updated:   {}", updated_count);
    info!("  Removed:   {}", removed_count);
    if failed_count > 0 {
        warn!("  Failed:    {}", failed_count);
    }

    Ok(())
}
