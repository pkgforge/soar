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
use nu_ansi_term::Color::{Blue, Green};
use rand::{distr::Alphanumeric, Rng};
use rusqlite::Connection;
use soar_core::{
    config::get_config,
    database::{
        models::{InstalledPackage, Package},
        packages::{FilterCondition, PackageQueryBuilder, PaginatedResponse},
    },
    error::{ErrorContext, SoarError},
    package::{
        formats::common::integrate_package,
        install::{InstallTarget, PackageInstaller},
        query::PackageQuery,
    },
    utils::{apply_sig_variants, calculate_checksum, default_install_patterns},
    SoarResult,
};
use soar_dl::downloader::DownloadState;
use tokio::sync::Semaphore;
use tracing::{error, info, warn};

use crate::{
    progress::handle_install_progress,
    state::AppState,
    utils::{
        ask_target_action, has_desktop_integration, mangle_package_symlinks,
        select_package_interactively, Colored,
    },
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
    pub portable_share: Option<String>,
    pub warnings: Arc<Mutex<Vec<String>>>,
    pub errors: Arc<Mutex<Vec<String>>>,
    pub retrying: Arc<AtomicU64>,
    pub failed: Arc<AtomicU64>,
    pub installed_indices: Arc<Mutex<HashMap<usize, (PathBuf, Vec<(PathBuf, PathBuf)>)>>>,
    pub binary_only: bool,
}

pub fn create_install_context(
    total_packages: usize,
    parallel_limit: u32,
    portable: Option<String>,
    portable_home: Option<String>,
    portable_config: Option<String>,
    portable_share: Option<String>,
    binary_only: bool,
) -> InstallContext {
    let multi_progress = Arc::new(MultiProgress::new());
    let total_progress_bar = multi_progress.add(ProgressBar::new(total_packages as u64));
    total_progress_bar
        .set_style(ProgressStyle::with_template("Installing {pos}/{len} {msg}").unwrap());

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
        warnings: Arc::new(Mutex::new(Vec::new())),
        errors: Arc::new(Mutex::new(Vec::new())),
        retrying: Arc::new(AtomicU64::new(0)),
        failed: Arc::new(AtomicU64::new(0)),
        installed_indices: Arc::new(Mutex::new(HashMap::new())),
        binary_only,
    }
}

pub async fn install_packages(
    packages: &[String],
    force: bool,
    yes: bool,
    portable: Option<String>,
    portable_home: Option<String>,
    portable_config: Option<String>,
    portable_share: Option<String>,
    no_notes: bool,
    binary_only: bool,
    ask: bool,
) -> SoarResult<()> {
    let state = AppState::new();
    let repo_db = state.repo_db().await?;
    let core_db = state.core_db()?;

    let install_targets = resolve_packages(repo_db.clone(), core_db.clone(), packages, yes, force)?;

    if install_targets.is_empty() {
        info!("No packages to install");
        return Ok(());
    }

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
        binary_only,
    );

    perform_installation(install_context, install_targets, core_db.clone(), no_notes).await
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
        let mut query = PackageQuery::try_from(package.as_str())?;
        let builder = PackageQueryBuilder::new(db.clone());

        if let Some(ref pkg_id) = query.pkg_id {
            if pkg_id == "all" {
                let builder = query.apply_filters(builder.clone());
                let packages: PaginatedResponse<Package> = builder.load()?;

                if packages.total == 0 {
                    error!("Package {} not found", query.name.unwrap());
                    continue;
                }
                let pkg = if packages.total > 1 {
                    let pkgs = packages.items;
                    &select_package_interactively(pkgs, &query.name.unwrap_or(package.clone()))?
                        .unwrap()
                } else {
                    packages.items.first().unwrap()
                };
                query.pkg_id = Some(pkg.pkg_id.clone());
                query.name = None;
            }
        }

        let mut builder = query.apply_filters(builder);
        if query.pkg_id.is_none() {
            builder = builder.limit(1);
        }

        let installed_packages = builder
            .clone()
            .database(core_db.clone())
            .load_installed()?
            .items;

        if query.name.is_none() && query.pkg_id.is_some() {
            let packages: PaginatedResponse<Package> = builder.load()?;
            for pkg in packages.items {
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
                            "{}#{} is already installed - {}",
                            existing.pkg_name,
                            existing.pkg_id,
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
                    profile: None,
                });
            }
        } else {
            let existing_install = if installed_packages.is_empty() {
                None
            } else {
                Some(installed_packages.first().unwrap().clone())
            };

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

            if let Some(package) =
                select_package(package, builder.clear_limit(), yes, &existing_install)?
            {
                install_targets.push(InstallTarget {
                    package,
                    existing_install,
                    with_pkg_id: false,
                    profile: None,
                });
            }
        }
    }

    Ok(install_targets)
}

fn select_package(
    package_name: &str,
    builder: PackageQueryBuilder,
    yes: bool,
    existing_install: &Option<InstalledPackage>,
) -> SoarResult<Option<Package>> {
    let builder = if let Some(existing) = existing_install {
        builder
            .clear_filters()
            .where_and("r.name", FilterCondition::Eq(existing.repo_name.clone()))
            .where_and("pkg_name", FilterCondition::Eq(existing.pkg_name.clone()))
            .where_and("pkg_id", FilterCondition::Eq(existing.pkg_id.clone()))
    } else {
        builder
    };

    let packages = builder.load()?.items;

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
    core_db: Arc<Mutex<Connection>>,
    no_notes: bool,
) -> SoarResult<()> {
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
            .map_err(|err| SoarError::Custom(format!("Join handle error: {}", err)))?;
    }

    ctx.total_progress_bar.finish_and_clear();
    for warn in ctx.warnings.lock().unwrap().iter() {
        warn!("{warn}");
    }

    for error in ctx.errors.lock().unwrap().iter() {
        error!("{error}");
    }

    let installed_indices = ctx.installed_indices.lock().unwrap();
    for (idx, target) in targets.into_iter().enumerate() {
        let pkg = target.package;
        let Some((install_dir, symlinks)) = installed_indices.get(&idx) else {
            continue;
        };

        info!(
            "\n* {}#{} [Installed to: {}]",
            pkg.pkg_name,
            pkg.pkg_id,
            Colored(Blue, install_dir.display())
        );

        if !symlinks.is_empty() {
            info!("  Binaries:");
            for (target, link) in symlinks {
                info!(
                    "    {} -> {}",
                    Colored(Green, link.display()),
                    Colored(Blue, target.display())
                );
            }
        }

        if !no_notes {
            if let Some(notes) = pkg.notes {
                info!("  Notes:\n  {}", notes.join("\n  "));
            }
            info!("\n");
        }
    }

    let installed_count = ctx.installed_count.load(Ordering::Relaxed);
    if installed_count > 0 {
        info!(
            "Installed {}/{} packages",
            ctx.installed_count.load(Ordering::Relaxed),
            ctx.total_packages
        );
    } else {
        info!("No packages installed.");
    }

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
    let progress_bar = Arc::new(Mutex::new(None));

    let progress_callback = {
        let ctx = ctx.clone();
        let progress_bar = progress_bar.clone();
        let package = target.package.clone();

        Arc::new(move |state| {
            let mut pb_lock = progress_bar.lock().unwrap();

            handle_install_progress(state, &mut pb_lock, &ctx, &package, idx, fixed_width);
        })
    };

    let total_pb = ctx.total_progress_bar.clone();
    let installed_count = ctx.installed_count.clone();
    let installed_indices = ctx.installed_indices.clone();
    let ctx = ctx.clone();

    tokio::spawn(async move {
        let result = install_single_package(&ctx, &target, progress_callback, core_db).await;

        match result {
            Ok((install_dir, symlinks)) => {
                installed_indices
                    .lock()
                    .unwrap()
                    .insert(idx, (install_dir, symlinks));
                installed_count.fetch_add(1, Ordering::Relaxed);
                total_pb.inc(1);
            }
            Err(err) => match err {
                SoarError::Warning(err) => {
                    let mut warnings = ctx.warnings.lock().unwrap();
                    warnings.push(err);
                }
                _ => {
                    let mut errors = ctx.errors.lock().unwrap();
                    errors.push(err.to_string());
                }
            },
        }

        drop(permit);
    })
}

pub async fn install_single_package(
    ctx: &InstallContext,
    target: &InstallTarget,
    progress_callback: Arc<dyn Fn(DownloadState) + Send + Sync>,
    core_db: Arc<Mutex<Connection>>,
) -> SoarResult<(PathBuf, Vec<(PathBuf, PathBuf)>)> {
    let bin_dir = get_config().get_bin_path()?;

    let (
        install_dir,
        real_bin,
        unlinked,
        portable,
        portable_home,
        portable_config,
        portable_share,
        excludes,
    ) = if let Some(ref existing) = target.existing_install {
        let install_dir = PathBuf::from(&existing.installed_path);
        let real_bin = install_dir.join(&target.package.pkg_name);

        (
            install_dir,
            real_bin,
            existing.unlinked,
            existing.portable_path.as_deref(),
            existing.portable_home.as_deref(),
            existing.portable_config.as_deref(),
            existing.portable_share.as_deref(),
            existing.install_patterns.as_deref(),
        )
    } else {
        let rand_str: String = rand::rng()
            .sample_iter(&Alphanumeric)
            .take(12)
            .map(char::from)
            .collect();

        let install_dir = get_config()
            .get_packages_path(target.profile.clone())
            .unwrap()
            .join(format!(
                "{}-{}-{}",
                target.package.pkg_name, target.package.pkg_id, rand_str
            ));
        let real_bin = install_dir.join(&target.package.pkg_name);

        (
            install_dir,
            real_bin,
            false,
            ctx.portable.as_deref(),
            ctx.portable_home.as_deref(),
            ctx.portable_config.as_deref(),
            ctx.portable_share.as_deref(),
            None,
        )
    };

    if install_dir.exists() {
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

    let installer = PackageInstaller::new(
        target,
        &install_dir,
        Some(progress_callback),
        core_db,
        target.with_pkg_id,
        install_patterns.to_vec(),
    )
    .await?;

    let downloaded_checksum = installer.download_package().await?;

    if let Some(repository) = get_config().get_repository(&target.package.repo_name) {
        if repository.signature_verification() {
            let repository_path = repository.get_path()?;
            let pubkey_file = repository_path.join("minisign.pub");
            if pubkey_file.exists() {
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
                                SoarError::Custom(format!(
                                    "Failed to setup stream verifier: {}",
                                    err
                                ))
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

                        // we can safely remove the signature file
                        fs::remove_file(&path).with_context(|| {
                            format!("removing minisign file {}", path.display())
                        })?;
                    }
                }
            } else {
                ctx.warnings.lock().unwrap().push(format!(
                    "{}#{} - Signature verification skipped as no pubkey was found.",
                    target.package.pkg_name, target.package.pkg_id
                ))
            }
        }
    }

    if target.package.provides.is_some() {
        let final_checksum = if target.package.ghcr_pkg.is_some() {
            if real_bin.exists() {
                Some(calculate_checksum(&real_bin)?)
            } else {
                None
            }
        } else {
            downloaded_checksum
        };

        if let Some(calculated_checksum) = final_checksum {
            if let Some(ref expected_checksum) = target.package.bsum {
                if calculated_checksum != *expected_checksum {
                    return Err(SoarError::Custom(format!(
                        "{}#{} - Invalid checksum, skipped installation.",
                        target.package.pkg_name, target.package.pkg_id
                    )));
                }
            } else {
                ctx.warnings.lock().unwrap().push(format!(
                    "{}#{} - Blake3 checksum not found. Skipped checksum validation.",
                    target.package.pkg_name, target.package.pkg_id
                ));
            }
        }
    }

    let symlinks =
        mangle_package_symlinks(&install_dir, &bin_dir, target.package.provides.as_deref()).await?;

    if !unlinked || has_desktop_integration(&target.package) {
        integrate_package(
            &install_dir,
            &target.package,
            portable,
            portable_home,
            portable_config,
            portable_share,
        )
        .await?;
    }

    installer
        .record(
            unlinked,
            portable,
            portable_home,
            portable_config,
            portable_share,
        )
        .await?;

    Ok((install_dir, symlinks))
}
