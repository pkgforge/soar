use std::{
    collections::HashMap,
    fs::{self, File},
    io::{BufReader, Read},
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use minisign_verify::{PublicKey, Signature};
use nu_ansi_term::Color::{Blue, Cyan, Green, Magenta, Red, Yellow};
use rand::{distr::Alphanumeric, Rng};
use soar_config::{config::get_config, utils::default_install_patterns};
use soar_core::{
    database::{connection::DieselDatabase, models::Package},
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
use soar_dl::types::Progress;
use soar_package::integrate_package;
use soar_utils::{hash::calculate_checksum, pattern::apply_sig_variants};
use tabled::{
    builder::Builder,
    settings::{themes::BorderCorrection, Panel, Style},
};
use tokio::sync::Semaphore;
use tracing::{debug, error, info, trace, warn};

use crate::{
    logging::{clear_multi_progress, set_multi_progress},
    progress::handle_install_progress,
    state::AppState,
    utils::{
        ask_target_action, confirm_action, display_settings, has_desktop_integration, icon_or,
        mangle_package_symlinks, progress_enabled, select_package_interactively,
        select_package_interactively_with_installed, Colored, Icons,
    },
};

// Represents an installed directory and its contents:
// - The first element is the root installation path.
// - The second element is a list of (file path, symlink target) pairs.
type InstalledPath = (PathBuf, Vec<(PathBuf, PathBuf)>);

#[derive(Clone)]
pub struct InstallContext {
    pub multi_progress: Arc<MultiProgress>,
    pub total_progress_bar: ProgressBar,
    pub semaphore: Arc<Semaphore>,
    pub installed_count: Arc<AtomicU64>,
    pub total_packages: usize,
    pub portable: Option<String>,
    pub portable_home: Option<String>,
    pub portable_config: Option<String>,
    pub portable_share: Option<String>,
    pub portable_cache: Option<String>,
    pub warnings: Arc<Mutex<Vec<String>>>,
    pub errors: Arc<Mutex<Vec<String>>>,
    pub retrying: Arc<AtomicU64>,
    pub failed: Arc<AtomicU64>,
    pub installed_indices: Arc<Mutex<HashMap<usize, InstalledPath>>>,
    pub binary_only: bool,
    pub no_verify: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn create_install_context(
    total_packages: usize,
    parallel_limit: u32,
    portable: Option<String>,
    portable_home: Option<String>,
    portable_config: Option<String>,
    portable_share: Option<String>,
    portable_cache: Option<String>,
    binary_only: bool,
    no_verify: bool,
) -> InstallContext {
    let multi_progress = Arc::new(MultiProgress::new());
    let total_progress_bar = multi_progress.add(ProgressBar::new(total_packages as u64));

    if !progress_enabled() {
        multi_progress.set_draw_target(indicatif::ProgressDrawTarget::hidden());
        total_progress_bar.set_draw_target(indicatif::ProgressDrawTarget::hidden());
    } else {
        let settings = display_settings();
        let style = if settings.icons() {
            ProgressStyle::with_template(&format!(
                "{} Installing {{pos}}/{{len}} {{msg}}",
                Icons::PACKAGE
            ))
            .unwrap()
        } else {
            ProgressStyle::with_template("Installing {pos}/{len} {msg}").unwrap()
        };
        total_progress_bar.set_style(style);
    }

    InstallContext {
        multi_progress,
        total_progress_bar,
        semaphore: Arc::new(Semaphore::new(parallel_limit as usize)),
        installed_count: Arc::new(AtomicU64::new(0)),
        total_packages,
        portable,
        portable_home,
        portable_config,
        portable_share,
        portable_cache,
        warnings: Arc::new(Mutex::new(Vec::new())),
        errors: Arc::new(Mutex::new(Vec::new())),
        retrying: Arc::new(AtomicU64::new(0)),
        failed: Arc::new(AtomicU64::new(0)),
        installed_indices: Arc::new(Mutex::new(HashMap::new())),
        binary_only,
        no_verify,
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn install_packages(
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
    let state = AppState::new();
    let metadata_mgr = state.metadata_manager().await?;
    let diesel_db = state.diesel_core_db()?.clone();

    let install_targets = resolve_packages(
        &state,
        metadata_mgr,
        &diesel_db,
        packages,
        yes,
        force,
        name_override.as_deref(),
        version_override.as_deref(),
        pkg_type_override.as_deref(),
        pkg_id_override.as_deref(),
        show,
    )?;

    if install_targets.is_empty() {
        info!("No packages to install");
        return Ok(());
    }

    debug!(targets = install_targets.len(), "resolved install targets");

    if ask {
        ask_target_action(&install_targets, "install")?;
    }

    let install_context = create_install_context(
        install_targets.len(),
        state.config().parallel_limit.unwrap_or(4),
        portable,
        portable_home,
        portable_config,
        portable_share,
        portable_cache,
        binary_only,
        no_verify,
    );

    perform_installation(install_context, install_targets, diesel_db, no_notes).await
}

#[allow(clippy::too_many_arguments)]
fn resolve_packages(
    state: &AppState,
    metadata_mgr: &soar_core::database::connection::MetadataManager,
    diesel_db: &DieselDatabase,
    packages: &[String],
    yes: bool,
    force: bool,
    name_override: Option<&str>,
    version_override: Option<&str>,
    pkg_type_override: Option<&str>,
    pkg_id_override: Option<&str>,
    show: bool,
) -> SoarResult<Vec<InstallTarget>> {
    use soar_core::database::models::InstalledPackage;

    let mut install_targets = Vec::new();

    for package in packages {
        // Check if input is a URL
        if UrlPackage::is_url(package) {
            let url_pkg = UrlPackage::from_url(
                package,
                name_override,
                version_override,
                pkg_type_override,
                pkg_id_override,
            )?;

            // Check if already installed in core DB (repo_name="local")
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
                if !force {
                    warn!(
                        "{}#{}:{} ({}) is already installed - skipping",
                        installed.pkg_name,
                        installed.pkg_id,
                        installed.repo_name,
                        installed.version,
                    );
                    continue;
                }
            }

            let existing_install = installed_pkg
                .cloned()
                .or_else(|| installed_packages.into_iter().next());

            install_targets.push(InstallTarget {
                package: url_pkg.to_package(),
                existing_install,
                with_pkg_id: url_pkg.pkg_type.is_some(),
                pinned: false,
                profile: None,
                ..Default::default()
            });
            continue;
        }

        let query = PackageQuery::try_from(package.as_str())?;

        if show && query.pkg_id.is_none() && query.name.is_some() {
            let repo_pkgs: Vec<Package> = if let Some(ref repo_name) = query.repo_name {
                metadata_mgr
                    .query_repo(repo_name, |conn| {
                        MetadataRepository::find_filtered(
                            conn,
                            query.name.as_deref(),
                            None,
                            query.version.as_deref(),
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
                        query.version.as_deref(),
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

            // Always show interactive selection when --show is used
            let pkg = select_package_interactively_with_installed(
                repo_pkgs,
                &query.name.clone().unwrap_or(package.clone()),
                &installed_packages,
            )?;

            let Some(pkg) = pkg else {
                continue;
            };

            // Check if this specific package is already installed
            let existing_install = diesel_db
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
                let existing: &InstalledPackage = existing;
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

            install_targets.push(InstallTarget {
                package: pkg,
                existing_install,
                with_pkg_id: true,
                pinned: query.version.is_some(),
                profile: None,
                ..Default::default()
            });
            continue;
        }

        if let Some(ref pkg_id) = query.pkg_id {
            if pkg_id == "all" {
                // Find all variants of this package
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
                    error!("Package {} not found", query.name.as_ref().unwrap());
                    continue;
                }

                let selected_pkg = if variants.len() > 1 {
                    if yes {
                        variants.into_iter().next().unwrap()
                    } else {
                        select_package_interactively(variants, query.name.as_ref().unwrap())?
                            .unwrap()
                    }
                } else {
                    variants.into_iter().next().unwrap()
                };

                let target_pkg_id = selected_pkg.pkg_id.clone();

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

                // Get installed packages for this pkg_id
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

                // Show confirmation for bulk install
                if all_pkgs.len() > 1 && !yes {
                    use nu_ansi_term::Color::{Blue, Cyan, Green};
                    info!(
                        "The following {} packages will be installed:",
                        Colored(Cyan, all_pkgs.len())
                    );
                    for pkg in &all_pkgs {
                        info!(
                            "  - {}#{}:{}",
                            Colored(Blue, &pkg.pkg_name),
                            Colored(Cyan, &pkg.pkg_id),
                            Colored(Green, &pkg.repo_name)
                        );
                    }
                    if !confirm_action("Proceed with installation?")? {
                        info!("Installation cancelled");
                        continue;
                    }
                }

                for pkg in all_pkgs {
                    let existing_install = installed_packages
                        .iter()
                        .find(|ip| ip.pkg_name == pkg.pkg_name)
                        .cloned();

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
                                continue;
                            }
                        }
                    }

                    install_targets.push(InstallTarget {
                        package: pkg,
                        existing_install,
                        with_pkg_id: true,
                        pinned: query.version.is_some(),
                        profile: None,
                        ..Default::default()
                    });
                }
                continue;
            }
        }

        let installed_packages: Vec<InstalledPackage> = diesel_db
            .with_conn(|conn| {
                CoreRepository::list_filtered(
                    conn,
                    query.repo_name.as_deref(),
                    query.name.as_deref(),
                    query.pkg_id.as_deref(),
                    query.version.as_deref(),
                    None,
                    None,
                    None,
                    Some(SortDirection::Asc),
                )
            })?
            .into_iter()
            .map(Into::into)
            .collect();

        if query.name.is_none() && query.pkg_id.is_some() {
            let repo_pkgs: Vec<Package> = if let Some(ref repo_name) = query.repo_name {
                metadata_mgr
                    .query_repo(repo_name, |conn| {
                        MetadataRepository::find_filtered(
                            conn,
                            None,
                            query.pkg_id.as_deref(),
                            query.version.as_deref(),
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
                        query.version.as_deref(),
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

            for pkg in repo_pkgs {
                let existing_install = installed_packages
                    .iter()
                    .find(|ip| ip.pkg_name == pkg.pkg_name)
                    .cloned();
                if let Some(ref existing) = existing_install {
                    if !existing.with_pkg_id {
                        continue;
                    }
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
                            continue;
                        }
                    }
                }

                install_targets.push(InstallTarget {
                    package: pkg,
                    existing_install,
                    with_pkg_id: true,
                    pinned: query.version.is_some(),
                    profile: None,
                    ..Default::default()
                });
            }
        } else {
            let maybe_existing = if installed_packages.is_empty() {
                None
            } else {
                Some(installed_packages.first().unwrap().clone())
            };

            if let Some(db_pkg) =
                select_package(state, metadata_mgr, package, &query, yes, &maybe_existing)?
            {
                let installed_pkg = installed_packages.iter().find(|ip| ip.is_installed);

                if let Some(installed) = installed_pkg {
                    warn!(
                        "{}#{}:{} ({}) is already installed - {}",
                        installed.pkg_name,
                        installed.pkg_id,
                        installed.repo_name,
                        installed.version,
                        if force { "reinstalling" } else { "skipping" }
                    );
                    if !force {
                        info!("Hint: Use --force to reinstall, or --show to see other variants");
                        continue;
                    }
                }

                let existing_install = installed_packages
                    .iter()
                    .find(|ip| ip.version == db_pkg.version)
                    .cloned();

                install_targets.push(InstallTarget {
                    package: db_pkg,
                    existing_install,
                    with_pkg_id: false,
                    pinned: query.version.is_some(),
                    profile: None,
                    ..Default::default()
                });
            }
        }
    }

    Ok(install_targets)
}

fn select_package(
    _state: &AppState,
    metadata_mgr: &soar_core::database::connection::MetadataManager,
    package_name: &str,
    query: &PackageQuery,
    yes: bool,
    existing_install: &Option<soar_core::database::models::InstalledPackage>,
) -> SoarResult<Option<Package>> {
    // If we have an existing install, try to find it in its original repo first
    let packages: Vec<Package> = if let Some(existing) = existing_install {
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

        // If package not found in original repo (repo removed or package removed),
        // fall back to searching all repos by package name
        if existing_pkgs.is_empty() {
            metadata_mgr.query_all_flat(|repo_name, conn| {
                let pkgs = MetadataRepository::find_filtered(
                    conn,
                    query.name.as_deref(),
                    query.pkg_id.as_deref(),
                    query.version.as_deref(),
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
        } else {
            existing_pkgs
        }
    } else if let Some(ref repo_name) = query.repo_name {
        metadata_mgr
            .query_repo(repo_name, |conn| {
                MetadataRepository::find_filtered(
                    conn,
                    query.name.as_deref(),
                    query.pkg_id.as_deref(),
                    query.version.as_deref(),
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
                query.name.as_deref(),
                query.pkg_id.as_deref(),
                query.version.as_deref(),
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

    match packages.len() {
        0 => {
            error!("Package {package_name} not found");
            Ok(None)
        }
        1 => Ok(packages.into_iter().next()),
        _ if yes => Ok(packages.into_iter().next()),
        _ => select_package_interactively(packages, package_name),
    }
}

pub async fn perform_installation(
    ctx: InstallContext,
    targets: Vec<InstallTarget>,
    core_db: DieselDatabase,
    no_notes: bool,
) -> SoarResult<()> {
    // Set the multi-progress for log suspension during progress bar updates
    set_multi_progress(&ctx.multi_progress);

    let mut handles = Vec::new();
    let fixed_width = 40;

    for (idx, target) in targets.iter().enumerate() {
        let handle =
            spawn_installation_task(&ctx, target.clone(), core_db.clone(), idx, fixed_width).await;
        handles.push(handle);
    }

    for handle in handles {
        handle
            .await
            .map_err(|err| SoarError::Custom(format!("Join handle error: {err}")))?;
    }

    ctx.total_progress_bar.finish_and_clear();

    // Clear the multi-progress reference now that progress bars are done
    clear_multi_progress();

    for warn in ctx.warnings.lock().unwrap().iter() {
        warn!("{warn}");
    }

    for error in ctx.errors.lock().unwrap().iter() {
        error!("{error}");
    }

    let installed_indices = ctx.installed_indices.lock().unwrap();
    let settings = display_settings();
    let use_icons = settings.icons();

    for (idx, target) in targets.into_iter().enumerate() {
        let pkg = target.package;
        let Some((install_dir, symlinks)) = installed_indices.get(&idx) else {
            continue;
        };

        info!(
            "\n{} {}#{}:{} [{}]",
            icon_or(Icons::CHECK, "*"),
            Colored(Blue, &pkg.pkg_name),
            Colored(Cyan, &pkg.pkg_id),
            Colored(Green, &pkg.repo_name),
            Colored(Magenta, install_dir.display())
        );

        if !symlinks.is_empty() {
            info!("  {} Binaries:", icon_or("📂", "-"));
            for (target, link) in symlinks {
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
            if let Some(notes) = pkg.notes {
                info!(
                    "  {} Notes:\n    {}",
                    icon_or("📝", "-"),
                    Colored(Yellow, notes.join("\n    "))
                );
            }
        }
    }

    let installed_count = ctx.installed_count.load(Ordering::Relaxed);
    let failed_count = ctx.failed.load(Ordering::Relaxed);

    if use_icons {
        let mut builder = Builder::new();

        if installed_count > 0 {
            builder.push_record([
                format!("{} Installed", icon_or(Icons::CHECK, "+")),
                format!(
                    "{}/{}",
                    Colored(Green, installed_count),
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
            ctx.total_packages,
            if failed_count > 0 {
                format!(", {} failed", failed_count)
            } else {
                String::new()
            }
        );
    } else {
        info!("No packages installed.");
    }

    Ok(())
}

async fn spawn_installation_task(
    ctx: &InstallContext,
    target: InstallTarget,
    core_db: DieselDatabase,
    idx: usize,
    fixed_width: usize,
) -> tokio::task::JoinHandle<()> {
    let permit = ctx.semaphore.clone().acquire_owned().await.unwrap();
    let progress_bar = Arc::new(Mutex::new(None));

    // Pre-compute the prefix string to avoid cloning the entire Package struct
    let prefix = {
        let prefix = format!(
            "[{}/{}] {}#{}",
            idx + 1,
            ctx.total_packages,
            target.package.pkg_name,
            target.package.pkg_id
        );
        if prefix.len() > fixed_width {
            format!("{prefix:.fixed_width$}")
        } else {
            format!("{prefix:<fixed_width$}")
        }
    };

    let progress_callback = {
        let ctx = ctx.clone();
        let progress_bar = progress_bar.clone();

        Arc::new(move |state| {
            let mut pb_lock = progress_bar.lock().unwrap();
            handle_install_progress(state, &mut pb_lock, &ctx, &prefix);
        })
    };

    let total_pb = ctx.total_progress_bar.clone();
    let installed_count = ctx.installed_count.clone();
    let installed_indices = ctx.installed_indices.clone();
    let ctx = ctx.clone();

    tokio::spawn(async move {
        let result =
            install_single_package(&ctx, &target, progress_callback, core_db.clone()).await;

        match result {
            Ok((install_dir, symlinks)) => {
                installed_indices
                    .lock()
                    .unwrap()
                    .insert(idx, (install_dir, symlinks));
                installed_count.fetch_add(1, Ordering::Relaxed);
                total_pb.inc(1);

                let _ = remove_old_versions(&target.package, &core_db, false);
            }
            Err(err) => {
                match err {
                    SoarError::Warning(err) => {
                        let mut warnings = ctx.warnings.lock().unwrap();
                        warnings.push(err);

                        let _ = remove_old_versions(&target.package, &core_db, false);
                    }
                    _ => {
                        let mut errors = ctx.errors.lock().unwrap();
                        errors.push(err.to_string());
                    }
                }
            }
        }

        drop(permit);
    })
}

pub async fn install_single_package(
    ctx: &InstallContext,
    target: &InstallTarget,
    progress_callback: Arc<dyn Fn(Progress) + Send + Sync>,
    core_db: DieselDatabase,
) -> SoarResult<(PathBuf, Vec<(PathBuf, PathBuf)>)> {
    debug!(
        pkg_name = target.package.pkg_name,
        pkg_id = target.package.pkg_id,
        version = target.package.version,
        repo = target.package.repo_name,
        size = target.package.ghcr_size.or(target.package.size),
        "installing {}#{}:{} ({})",
        target.package.pkg_name,
        target.package.pkg_id,
        target.package.repo_name,
        target.package.version
    );
    let bin_dir = get_config().get_bin_path()?;

    let dir_suffix: String = target
        .package
        .bsum
        .as_ref()
        .filter(|s| s.len() >= 12)
        .map(|s| s[..12].to_string())
        .unwrap_or_else(|| {
            rand::rng()
                .sample_iter(&Alphanumeric)
                .take(12)
                .map(char::from)
                .collect()
        });

    let install_dir = get_config()
        .get_packages_path(target.profile.clone())
        .unwrap()
        .join(format!(
            "{}-{}-{}",
            target.package.pkg_name, target.package.pkg_id, dir_suffix
        ));
    let real_bin = install_dir.join(&target.package.pkg_name);

    let (
        unlinked,
        portable,
        portable_home,
        portable_config,
        portable_share,
        portable_cache,
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
            ctx.portable.as_deref(),
            ctx.portable_home.as_deref(),
            ctx.portable_config.as_deref(),
            ctx.portable_share.as_deref(),
            ctx.portable_cache.as_deref(),
            None,
        )
    };

    let should_cleanup = if let Some(ref existing) = target.existing_install {
        if existing.is_installed {
            true
        } else {
            match InstallMarker::read_from_dir(&install_dir) {
                Some(marker) => !marker.matches_package(&target.package),
                None => true,
            }
        }
    } else {
        false
    };

    if should_cleanup && install_dir.exists() {
        debug!(path = %install_dir.display(), "cleaning up existing installation directory");
        if let Err(err) = std::fs::remove_dir_all(&install_dir) {
            return Err(SoarError::Custom(format!(
                "Failed to clean up install directory {}: {}",
                install_dir.display(),
                err
            )));
        }
    }

    let install_patterns = excludes.map(|e| e.to_vec()).unwrap_or_else(|| {
        if ctx.binary_only {
            let mut patterns = default_install_patterns();
            patterns.extend(
                ["!*.png", "!*.svg", "!*.desktop", "!LICENSE", "!CHECKSUM"]
                    .iter()
                    .map(ToString::to_string),
            );
            patterns
        } else {
            get_config().install_patterns.clone().unwrap_or_default()
        }
    });
    let install_patterns = apply_sig_variants(install_patterns);

    trace!(install_dir = %install_dir.display(), patterns = ?install_patterns, "creating package installer");
    let installer = PackageInstaller::new(
        target,
        &install_dir,
        Some(progress_callback),
        core_db,
        target.with_pkg_id,
        install_patterns.to_vec(),
    )
    .await?;

    debug!(
        pkg_name = target.package.pkg_name,
        source = target
            .package
            .ghcr_pkg
            .as_deref()
            .unwrap_or(&target.package.download_url),
        "downloading {}",
        target.package.pkg_name
    );
    let downloaded_checksum = installer.download_package().await?;
    trace!(
        pkg_name = target.package.pkg_name,
        checksum = ?downloaded_checksum,
        "download complete for {}",
        target.package.pkg_name
    );

    if let Some(repository) = get_config().get_repository(&target.package.repo_name) {
        if repository.signature_verification() {
            debug!(
                repo_name = target.package.repo_name,
                "performing signature verification"
            );
            let repository_path = repository.get_path()?;
            let pubkey_file = repository_path.join("minisign.pub");
            if pubkey_file.exists() {
                trace!(pubkey = %pubkey_file.display(), "loading public key");
                let pubkey = PublicKey::from_base64(
                    fs::read_to_string(&pubkey_file)
                        .with_context(|| {
                            format!("reading minisign key from {}", pubkey_file.display())
                        })?
                        .trim(),
                )
                .map_err(|err| {
                    SoarError::Custom(format!(
                        "Failed to load public key from {}: {}",
                        pubkey_file.display(),
                        err
                    ))
                })?;
                let entries = fs::read_dir(&install_dir).with_context(|| {
                    format!("reading package directory {}", install_dir.display())
                })?;
                for entry in entries {
                    let path = entry
                        .with_context(|| {
                            format!("reading entry from directory {}", install_dir.display())
                        })?
                        .path();
                    let is_signature_file =
                        path.extension().map_or_else(|| false, |ext| ext == "sig");
                    let original_file = path.with_extension("");
                    if is_signature_file && path.is_file() && original_file.is_file() {
                        let signature = Signature::from_file(&path).map_err(|err| {
                            SoarError::Custom(format!(
                                "Failed to load signature file from {}: {}",
                                path.display(),
                                err
                            ))
                        })?;
                        let mut stream_verifier =
                            pubkey.verify_stream(&signature).map_err(|err| {
                                SoarError::Custom(
                                    format!("Failed to setup stream verifier: {err}",),
                                )
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
                        trace!(file = %original_file.display(), "signature verified");

                        // we can safely remove the signature file
                        fs::remove_file(&path).with_context(|| {
                            format!("removing minisign file {}", path.display())
                        })?;
                    }
                }
                debug!("signature verification completed successfully");
            } else {
                ctx.warnings.lock().unwrap().push(format!(
                    "{}#{} - Signature verification skipped as no pubkey was found.",
                    target.package.pkg_name, target.package.pkg_id
                ))
            }
        }
    }

    if target.package.provides.is_some() {
        trace!("calculating final checksum for verification");
        let final_checksum = if target.package.ghcr_pkg.is_some() {
            if real_bin.exists() {
                Some(calculate_checksum(&real_bin)?)
            } else {
                None
            }
        } else {
            downloaded_checksum
        };

        if !ctx.no_verify {
            match (final_checksum, target.package.bsum.as_ref()) {
                (Some(calculated), Some(expected)) if calculated != *expected => {
                    return Err(SoarError::Custom(format!(
                        "{}#{} - Invalid checksum, skipped installation.",
                        target.package.pkg_name, target.package.pkg_id
                    )));
                }
                (Some(_), None) => {
                    ctx.warnings.lock().unwrap().push(format!(
                        "{}#{} - Blake3 checksum not found. Skipped checksum validation.",
                        target.package.pkg_name, target.package.pkg_id
                    ));
                }
                (Some(ref calculated), Some(expected)) if calculated == expected => {
                    trace!("checksum verification passed");
                }
                _ => {}
            }
        }
    }

    trace!("creating symlinks for package binaries");
    let symlinks =
        mangle_package_symlinks(&install_dir, &bin_dir, target.package.provides.as_deref()).await?;
    debug!(symlink_count = symlinks.len(), "symlinks created");

    if !unlinked || has_desktop_integration(&target.package) {
        trace!("integrating package (desktop files, icons, etc.)");
        integrate_package(
            &install_dir,
            &target.package,
            portable,
            portable_home,
            portable_config,
            portable_share,
            portable_cache,
        )
        .await?;
    }

    trace!("recording installation to database");
    installer
        .record(
            unlinked,
            portable,
            portable_home,
            portable_config,
            portable_share,
            portable_cache,
        )
        .await?;

    debug!(
        pkg_name = target.package.pkg_name,
        pkg_id = target.package.pkg_id,
        version = target.package.version,
        install_dir = %install_dir.display(),
        symlinks = symlinks.len(),
        "installed {}#{}:{} ({}) to {}",
        target.package.pkg_name,
        target.package.pkg_id,
        target.package.repo_name,
        target.package.version,
        install_dir.display()
    );
    Ok((install_dir, symlinks))
}
