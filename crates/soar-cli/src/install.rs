use nu_ansi_term::Color::{Blue, Cyan, Green, Magenta, Red, Yellow};
use soar_core::{package::install::InstallTarget, SoarResult};
use soar_operations::{install, InstallOptions, InstallReport, ResolveResult, SoarContext};
use tabled::{
    builder::Builder,
    settings::{themes::BorderCorrection, Panel, Style},
};
use tracing::{debug, error, info, warn};

use crate::utils::{
    ask_target_action, display_settings, icon_or, select_package_interactively,
    select_package_interactively_with_installed, Colored, Icons,
};

#[allow(clippy::too_many_arguments)]
pub async fn install_packages(
    ctx: &SoarContext,
    packages: &[String],
    force: bool,
    yes: bool,
    portable: Option<String>,
    portable_home: Option<String>,
    portable_config: Option<String>,
    portable_share: Option<String>,
    portable_cache: Option<String>,
    no_notes: bool,
    binary_only: bool,
    ask: bool,
    no_verify: bool,
    name_override: Option<String>,
    version_override: Option<String>,
    pkg_type_override: Option<String>,
    pkg_id_override: Option<String>,
    show: bool,
) -> SoarResult<()> {
    debug!(
        count = packages.len(),
        force = force,
        "starting package installation"
    );

    let options = InstallOptions {
        force,
        portable: portable.clone(),
        portable_home: portable_home.clone(),
        portable_config: portable_config.clone(),
        portable_share: portable_share.clone(),
        portable_cache: portable_cache.clone(),
        binary_only,
        no_verify,
        name_override,
        version_override,
        pkg_type_override,
        pkg_id_override,
    };

    // If --show flag is used, handle interactive selection before resolving
    if show {
        return install_with_show(ctx, packages, &options, yes, force, ask, no_notes).await;
    }

    let results = install::resolve_packages(ctx, packages, &options).await?;

    let mut install_targets = Vec::new();
    for result in results {
        match result {
            ResolveResult::Resolved(targets) => {
                install_targets.extend(targets);
            }
            ResolveResult::Ambiguous(amb) => {
                let pkg = if yes {
                    amb.candidates.into_iter().next()
                } else {
                    select_package_interactively(amb.candidates, &amb.query)?
                };

                if let Some(pkg) = pkg {
                    // Re-resolve with the specific selected package
                    let specific_query =
                        format!("{}#{}:{}", pkg.pkg_name, pkg.pkg_id, pkg.repo_name);
                    let re_results =
                        install::resolve_packages(ctx, &[specific_query], &options).await?;
                    for r in re_results {
                        if let ResolveResult::Resolved(targets) = r {
                            install_targets.extend(targets);
                        }
                    }
                }
            }
            ResolveResult::NotFound(name) => {
                error!("Package {} not found", name);
            }
            ResolveResult::AlreadyInstalled {
                pkg_name,
                pkg_id,
                repo_name,
                version,
            } => {
                warn!(
                    "{}#{}:{} ({}) is already installed - skipping",
                    pkg_name, pkg_id, repo_name, version,
                );
                if !force {
                    info!("Hint: Use --force to reinstall, or --show to see other variants");
                }
            }
        }
    }

    if install_targets.is_empty() {
        info!("No packages to install");
        return Ok(());
    }

    debug!(targets = install_targets.len(), "resolved install targets");

    if ask {
        ask_target_action(&install_targets, "install")?;
    }

    let report = install::perform_installation(ctx, install_targets, &options).await?;
    display_install_report(&report, no_notes);

    Ok(())
}

async fn install_with_show(
    ctx: &SoarContext,
    packages: &[String],
    options: &InstallOptions,
    _yes: bool,
    force: bool,
    ask: bool,
    no_notes: bool,
) -> SoarResult<()> {
    use soar_core::{database::models::Package, package::query::PackageQuery};
    use soar_db::repository::{
        core::{CoreRepository, SortDirection},
        metadata::MetadataRepository,
    };

    let metadata_mgr = ctx.metadata_manager().await?;
    let diesel_db = ctx.diesel_core_db()?;
    let mut install_targets = Vec::new();

    for package in packages {
        let query = PackageQuery::try_from(package.as_str())?;

        // --show requires a name and no pkg_id
        if query.pkg_id.is_some() || query.name.is_none() {
            // Fall through to normal resolve for non-show cases
            let results =
                install::resolve_packages(ctx, std::slice::from_ref(package), options).await?;
            for result in results {
                if let ResolveResult::Resolved(targets) = result {
                    install_targets.extend(targets);
                }
            }
            continue;
        }

        let repo_pkgs: Vec<Package> = if let Some(ref repo_name) = query.repo_name {
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

        let repo_pkgs: Vec<Package> = if let Some(ref version) = query.version {
            repo_pkgs
                .into_iter()
                .filter(|p| p.has_version(version))
                .collect()
        } else {
            repo_pkgs
        };

        if repo_pkgs.is_empty() {
            error!("Package {} not found", query.name.as_ref().unwrap());
            continue;
        }

        // Get installed packages to show [installed] marker
        let installed_packages: Vec<(String, String, String)> = diesel_db
            .with_conn(|conn| {
                CoreRepository::list_filtered(
                    conn,
                    query.repo_name.as_deref(),
                    query.name.as_deref(),
                    None,
                    None,
                    Some(true),
                    None,
                    None,
                    None,
                )
            })?
            .into_iter()
            .map(|p| (p.pkg_id, p.repo_name, p.version))
            .collect();

        let pkg = select_package_interactively_with_installed(
            repo_pkgs,
            &query.name.clone().unwrap_or(package.clone()),
            &installed_packages,
        )?;

        let Some(pkg) = pkg else {
            continue;
        };

        // Check if this specific package is already installed
        let existing_install: Option<soar_core::database::models::InstalledPackage> = diesel_db
            .with_conn(|conn| {
                CoreRepository::list_filtered(
                    conn,
                    Some(&pkg.repo_name),
                    Some(&pkg.pkg_name),
                    Some(&pkg.pkg_id),
                    None,
                    None,
                    None,
                    None,
                    Some(SortDirection::Asc),
                )
            })?
            .into_iter()
            .map(Into::into)
            .next();

        if let Some(ref existing) = existing_install {
            if existing.is_installed {
                warn!(
                    "{}#{}:{} ({}) is already installed - {}",
                    existing.pkg_name,
                    existing.pkg_id,
                    existing.repo_name,
                    existing.version,
                    if force { "reinstalling" } else { "skipping" }
                );
                if !force {
                    info!("Hint: Use --force to reinstall, or --show to see other variants");
                    continue;
                }
            }
        }

        let pkg = pkg.resolve(query.version.as_deref());

        install_targets.push(InstallTarget {
            package: pkg,
            existing_install,
            pinned: query.version.is_some(),
            profile: None,
            ..Default::default()
        });
    }

    if install_targets.is_empty() {
        info!("No packages to install");
        return Ok(());
    }

    if ask {
        ask_target_action(&install_targets, "install")?;
    }

    let report = install::perform_installation(ctx, install_targets, options).await?;
    display_install_report(&report, no_notes);

    Ok(())
}

fn display_install_report(report: &InstallReport, no_notes: bool) {
    let settings = display_settings();
    let use_icons = settings.icons();

    for warn_msg in &report.warnings {
        warn!("{warn_msg}");
    }

    for info in &report.installed {
        info!(
            "\n{} {}#{}:{} [{}]",
            icon_or(Icons::CHECK, "*"),
            Colored(Blue, &info.pkg_name),
            Colored(Cyan, &info.pkg_id),
            Colored(Green, &info.repo_name),
            Colored(Magenta, info.install_dir.display())
        );

        if !info.symlinks.is_empty() {
            info!("  {} Binaries:", icon_or("📂", "-"));
            for (target, link) in &info.symlinks {
                info!(
                    "    {} {} {} {}",
                    icon_or(Icons::ARROW, "->"),
                    Colored(Green, link.display()),
                    icon_or("←", "<-"),
                    Colored(Blue, target.display())
                );
            }
        }

        if !no_notes {
            if let Some(ref notes) = info.notes {
                info!(
                    "  {} Notes:\n    {}",
                    icon_or("📝", "-"),
                    Colored(Yellow, notes.join("\n    "))
                );
            }
        }
    }

    for err_info in &report.failed {
        error!(
            "Failed to install {}#{}: {}",
            err_info.pkg_name, err_info.pkg_id, err_info.error
        );
    }

    let installed_count = report.installed.len();
    let failed_count = report.failed.len();
    let total_packages = installed_count + failed_count;

    if use_icons {
        let mut builder = Builder::new();

        if installed_count > 0 {
            builder.push_record([
                format!("{} Installed", icon_or(Icons::CHECK, "+")),
                format!(
                    "{}/{}",
                    Colored(Green, installed_count),
                    Colored(Cyan, total_packages)
                ),
            ]);
        }
        if failed_count > 0 {
            builder.push_record([
                format!("{} Failed", icon_or(Icons::CROSS, "!")),
                format!("{}", Colored(Red, failed_count)),
            ]);
        }
        if installed_count == 0 && failed_count == 0 {
            builder.push_record([
                format!("{} Status", icon_or(Icons::WARNING, "!")),
                "No packages installed".to_string(),
            ]);
        }

        let table = builder
            .build()
            .with(Panel::header("Installation Summary"))
            .with(Style::rounded())
            .with(BorderCorrection {})
            .to_string();

        info!("\n{table}");
    } else if installed_count > 0 {
        info!(
            "Installed {}/{} packages{}",
            installed_count,
            total_packages,
            if failed_count > 0 {
                format!(", {} failed", failed_count)
            } else {
                String::new()
            }
        );
    } else {
        info!("No packages installed.");
    }
}
