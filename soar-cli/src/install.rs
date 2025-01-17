use std::{
    os::unix::fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use rand::{distributions::Alphanumeric, Rng};
use rusqlite::Connection;
use soar_core::{
    config::get_config,
    database::{
        models::{InstalledPackage, Package},
        packages::{get_installed_packages_with_filter, get_packages_with_filter, PackageFilter},
    },
    error::SoarError,
    package::{
        formats::common::integrate_package,
        install::{InstallTarget, PackageInstaller},
        query::PackageQuery,
    },
    utils::calculate_checksum,
    SoarResult,
};
use soar_dl::downloader::DownloadState;
use tokio::sync::Semaphore;
use tracing::{error, info, warn};

use crate::{
    progress::{self, create_progress_bar},
    state::AppState,
    utils::interactive_ask,
};

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
}

pub fn create_install_context(
    total_packages: usize,
    parallel_limit: usize,
    portable: Option<String>,
    portable_home: Option<String>,
    portable_config: Option<String>,
) -> InstallContext {
    let multi_progress = Arc::new(MultiProgress::new());
    let total_progress_bar = multi_progress.add(ProgressBar::new(total_packages as u64));
    total_progress_bar.set_style(ProgressStyle::with_template("Installing {pos}/{len}").unwrap());

    InstallContext {
        multi_progress,
        total_progress_bar,
        semaphore: Arc::new(Semaphore::new(parallel_limit)),
        installed_count: Arc::new(AtomicU64::new(0)),
        total_packages,
        portable,
        portable_home,
        portable_config,
    }
}

pub async fn install_packages(
    packages: &[String],
    force: bool,
    yes: bool,
    portable: Option<String>,
    portable_home: Option<String>,
    portable_config: Option<String>,
) -> SoarResult<()> {
    let state = AppState::new().await?;
    let repo_db = state.repo_db().clone();
    let core_db = state.core_db().clone();

    let install_targets = resolve_packages(repo_db, core_db.clone(), packages, yes, force)?;

    let install_context = create_install_context(
        install_targets.len(),
        state.config().parallel_limit.unwrap_or(1) as usize,
        portable,
        portable_home,
        portable_config,
    );

    perform_installation(install_context, install_targets, core_db).await
}

fn resolve_packages(
    db: Arc<Mutex<Connection>>,
    core_db: Arc<Mutex<Connection>>,
    packages: &[String],
    yes: bool,
    force: bool,
) -> SoarResult<Vec<InstallTarget>> {
    let mut install_targets = Vec::new();

    for package in packages {
        let query = PackageQuery::try_from(package.as_str())?;
        let filter = PackageFilter::from_query(query);

        let existing_install = get_existing_install(&core_db, &filter)?;

        if let Some(ref existing) = existing_install {
            if existing.is_installed {
                warn!(
                    "{} is already installed - {}",
                    package,
                    if force { "reinstalling" } else { "skipping" }
                );
                if !force {
                    continue;
                }
            }
        }

        if let Some(package) = select_package(db.clone(), package, &filter, yes, &existing_install)?
        {
            install_targets.push(InstallTarget {
                package,
                existing_install,
            });
        }
    }

    Ok(install_targets)
}

fn get_existing_install(
    core_db: &Arc<Mutex<Connection>>,
    filter: &PackageFilter,
) -> SoarResult<Option<InstalledPackage>> {
    let installed_pkgs: Vec<InstalledPackage> =
        get_installed_packages_with_filter(core_db.clone(), 128, filter.clone())?
            .filter_map(Result::ok)
            .collect();

    Ok(installed_pkgs.into_iter().next())
}

fn select_package(
    db: Arc<Mutex<Connection>>,
    package_name: &str,
    filter: &PackageFilter,
    yes: bool,
    existing_install: &Option<InstalledPackage>,
) -> SoarResult<Option<Package>> {
    let filter = if let Some(existing) = existing_install {
        PackageFilter {
            repo_name: Some(existing.repo_name.clone()),
            exact_pkg_name: Some(existing.pkg_name.clone()),
            ..Default::default()
        }
    } else {
        filter.clone()
    };
    let pkgs: Vec<Package> = get_packages_with_filter(db, 1024, filter)?
        .filter_map(Result::ok)
        .collect();

    match pkgs.len() {
        0 => {
            error!("Package {package_name} not found");
            Ok(None)
        }
        1 => Ok(pkgs.into_iter().next()),
        _ if yes => Ok(pkgs.into_iter().next()),
        _ => select_package_interactively(pkgs, package_name),
    }
}

fn select_package_interactively(
    pkgs: Vec<Package>,
    package_name: &str,
) -> SoarResult<Option<Package>> {
    info!("Multiple packages found for {package_name}");
    for (idx, pkg) in pkgs.iter().enumerate() {
        info!(
            "[{}] {}#{}-{}:{}",
            idx + 1,
            pkg.pkg_name,
            pkg.pkg_id,
            pkg.version,
            pkg.repo_name
        );
    }

    let selection = get_valid_selection(pkgs.len())?;
    Ok(pkgs.into_iter().nth(selection))
}

fn get_valid_selection(max: usize) -> SoarResult<usize> {
    loop {
        let response = interactive_ask("Select a package: ")?;
        match response.parse::<usize>() {
            Ok(n) if n > 0 && n <= max => return Ok(n - 1),
            _ => error!("Invalid selection, please try again."),
        }
    }
}

pub async fn perform_installation(
    ctx: InstallContext,
    targets: Vec<InstallTarget>,
    core_db: Arc<Mutex<Connection>>,
) -> SoarResult<()> {
    let mut handles = Vec::new();
    let fixed_width = 30;

    if targets.is_empty() {
        info!("No packages to install");
        return Ok(());
    }

    for (idx, target) in targets.iter().enumerate() {
        let handle =
            spawn_installation_task(&ctx, target.clone(), core_db.clone(), idx, fixed_width).await;
        handles.push(handle);
    }

    for handle in handles {
        handle
            .await
            .map_err(|err| SoarError::Custom(format!("Join handle error: {}", err)))?;
    }

    ctx.total_progress_bar.finish_and_clear();
    info!(
        "Installed {}/{} packages",
        ctx.installed_count.load(Ordering::Relaxed),
        ctx.total_packages
    );

    Ok(())
}

async fn spawn_installation_task(
    ctx: &InstallContext,
    target: InstallTarget,
    core_db: Arc<Mutex<Connection>>,
    idx: usize,
    fixed_width: usize,
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
        format!("{:.width$}", message, width = fixed_width)
    } else {
        format!("{:<width$}", message, width = fixed_width)
    };
    progress_bar.set_message(message);

    let progress_callback = Arc::new(move |state| {
        progress::handle_progress(state, &progress_bar);
    });

    let total_pb = ctx.total_progress_bar.clone();
    let installed_count = ctx.installed_count.clone();
    let ctx = ctx.clone();

    tokio::spawn(async move {
        let result = install_single_package(&ctx, target, progress_callback, core_db).await;

        if let Err(err) = result {
            error!("{err}");
        } else {
            installed_count.fetch_add(1, Ordering::Relaxed);
            total_pb.inc(1);
        }

        drop(permit);
    })
}

async fn install_single_package(
    ctx: &InstallContext,
    target: InstallTarget,
    progress_callback: Arc<dyn Fn(DownloadState) + Send + Sync>,
    core_db: Arc<Mutex<Connection>>,
) -> SoarResult<()> {
    let (install_dir, real_bin, bin_name) = if let Some(ref existing) = target.existing_install {
        let install_dir = PathBuf::from(&existing.installed_path);
        let real_bin = install_dir.join(&target.package.pkg_name);
        let bin_name = existing
            .bin_path
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                get_config()
                    .get_bin_path()
                    .unwrap()
                    .join(&target.package.pkg_name)
            });

        (install_dir, real_bin, bin_name)
    } else {
        let rand_str: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(12)
            .map(char::from)
            .collect();

        let install_dir = get_config()
            .get_packages_path()
            .unwrap()
            .join(format!("{}-{}", target.package.pkg, rand_str));
        let real_bin = install_dir.join(&target.package.pkg_name);
        let bin_name = get_config()
            .get_bin_path()
            .unwrap()
            .join(&target.package.pkg_name);

        (install_dir, real_bin, bin_name)
    };

    if bin_name.exists() {
        if let Err(err) = std::fs::remove_file(&bin_name) {
            return Err(SoarError::Custom(format!(
                "Failed to remove existing symlink: {}",
                err
            )));
        }
    }

    if install_dir.exists() {
        if let Err(err) = std::fs::remove_dir_all(&install_dir) {
            return Err(SoarError::Custom(format!(
                "Failed to clean up install directory {}: {}",
                install_dir.display(),
                err
            )));
        }
    }

    let installer = PackageInstaller::new(
        &target,
        install_dir,
        Some(progress_callback),
        core_db,
        false,
    )
    .await?;

    installer.install().await?;

    let final_checksum = calculate_checksum(&real_bin)?;
    fs::symlink(&real_bin, &bin_name)?;
    installer.record(&final_checksum, &bin_name).await?;

    integrate_package(
        &real_bin,
        &target.package,
        ctx.portable.clone(),
        ctx.portable_home.clone(),
        ctx.portable_config.clone(),
    )
    .await?;

    Ok(())
}
