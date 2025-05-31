use std::{
    fs,
    path::Path,
    sync::{atomic::Ordering, Arc, Mutex},
};

use rusqlite::{prepare_and_bind, Connection};
use soar_core::{
    database::{
        models::{InstalledPackage, Package},
        packages::{FilterCondition, PackageQueryBuilder},
    },
    error::{ErrorContext, SoarError},
    package::{install::InstallTarget, query::PackageQuery},
    SoarResult,
};
use tracing::{error, info, warn};

use crate::{
    install::{create_install_context, install_single_package, InstallContext},
    progress::{self, create_progress_bar},
    state::AppState,
    utils::ask_target_action,
};

fn get_existing(
    package: &Package,
    core_db: Arc<Mutex<Connection>>,
) -> SoarResult<Option<InstalledPackage>> {
    let existing = PackageQueryBuilder::new(core_db)
        .where_and("repo_name", FilterCondition::Eq(package.repo_name.clone()))
        .where_and("pkg_name", FilterCondition::Eq(package.pkg_name.clone()))
        .where_and("pkg_id", FilterCondition::Eq(package.pkg_id.clone()))
        .where_and("version", FilterCondition::Eq(package.version.clone()))
        .limit(1)
        .load_installed()?
        .items;

    if existing.is_empty() {
        Ok(None)
    } else {
        Ok(Some(existing[0].clone()))
    }
}

pub async fn update_packages(
    packages: Option<Vec<String>>,
    keep: bool,
    ask: bool,
) -> SoarResult<()> {
    let state = AppState::new();
    let core_db = state.core_db()?;
    let repo_db = state.repo_db().await?;
    let config = state.config();

    let mut update_targets = Vec::new();

    if let Some(packages) = packages {
        for package in packages {
            let query = PackageQuery::try_from(package.as_str())?;
            let builder = PackageQueryBuilder::new(core_db.clone());
            let mut builder = query
                .apply_filters(builder.clone())
                .where_and("is_installed", FilterCondition::Eq("1".to_string()))
                .limit(1);
            let installed_pkgs = builder.load_installed()?.items;

            for pkg in installed_pkgs {
                builder = builder
                    .database(repo_db.clone())
                    .where_and("repo_name", FilterCondition::Eq(pkg.repo_name.clone()))
                    .where_and("version", FilterCondition::Gt(pkg.version.clone()))
                    .where_and(
                        &format!(
                        "(version > '{}' OR version LIKE 'HEAD-%' AND substr(version, 14) > '{}')",
                        pkg.version,
                        if pkg.version.starts_with("HEAD-") && pkg.version.len() > 13 {
                            &pkg.version[14..]
                        } else {
                            ""
                        }
                    ),
                        FilterCondition::None,
                    )
                    .limit(1);
                let new_pkg: Vec<Package> = builder.load()?.items;

                if !new_pkg.is_empty() {
                    let with_pkg_id = pkg.with_pkg_id;
                    let package = new_pkg.first().unwrap().clone();

                    let existing_install = get_existing(&package, core_db.clone())?;
                    if let Some(ref existing_install) = existing_install {
                        if existing_install.is_installed {
                            continue;
                        }
                    }

                    update_targets.push(InstallTarget {
                        package,
                        existing_install,
                        with_pkg_id,
                        profile: Some(pkg.profile),
                    })
                }
            }
        }
    } else {
        let installed_packages = PackageQueryBuilder::new(core_db.clone())
            .where_and("is_installed", FilterCondition::Eq("1".to_string()))
            .where_and("pinned", FilterCondition::Eq(String::from("0")))
            .load_installed()?
            .items;

        for pkg in installed_packages {
            let new_pkg: Vec<Package> = PackageQueryBuilder::new(repo_db.clone())
                .where_and("repo_name", FilterCondition::Eq(pkg.repo_name.clone()))
                .where_and("pkg_name", FilterCondition::Eq(pkg.pkg_name.clone()))
                .where_and("pkg_id", FilterCondition::Eq(pkg.pkg_id.clone()))
                .where_and(
                    &format!(
                        "(version > '{}' OR version LIKE 'HEAD-%' AND substr(version, 14) > '{}')",
                        pkg.version,
                        if pkg.version.starts_with("HEAD-") && pkg.version.len() > 13 {
                            &pkg.version[14..]
                        } else {
                            ""
                        }
                    ),
                    FilterCondition::None,
                )
                .limit(1)
                .load()?
                .items;

            if !new_pkg.is_empty() {
                let with_pkg_id = pkg.with_pkg_id;
                let package = new_pkg.first().unwrap().clone();

                let existing_install = get_existing(&package, core_db.clone())?;
                if let Some(ref existing_install) = existing_install {
                    if existing_install.is_installed {
                        continue;
                    }
                }

                update_targets.push(InstallTarget {
                    package,
                    existing_install,
                    with_pkg_id,
                    profile: Some(pkg.profile),
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
        false,
    );

    perform_update(ctx, update_targets, core_db.clone(), keep).await?;

    Ok(())
}

async fn perform_update(
    ctx: InstallContext,
    targets: Vec<InstallTarget>,
    core_db: Arc<Mutex<Connection>>,
    keep: bool,
) -> SoarResult<()> {
    let mut handles = Vec::new();
    let fixed_width = 40;

    for (idx, target) in targets.iter().enumerate() {
        let handle = spawn_update_task(
            &ctx,
            target.clone(),
            core_db.clone(),
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
            .map_err(|err| SoarError::Custom(format!("Join handle error: {}", err)))?;
    }

    ctx.total_progress_bar.finish_and_clear();
    for warn in ctx.warnings.lock().unwrap().iter() {
        warn!("{warn}");
    }

    for error in ctx.errors.lock().unwrap().iter() {
        error!("{error}");
    }
    info!(
        "Updated {}/{} packages",
        ctx.installed_count.load(Ordering::Relaxed),
        ctx.total_packages
    );

    Ok(())
}

async fn spawn_update_task(
    ctx: &InstallContext,
    target: InstallTarget,
    core_db: Arc<Mutex<Connection>>,
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
        format!("{:.width$}", message, width = fixed_width)
    } else {
        format!("{:<width$}", message, width = fixed_width)
    };
    progress_bar.set_prefix(message);

    let progress_callback = Arc::new(move |state| {
        progress::handle_progress(state, &progress_bar);
    });

    let total_pb = ctx.total_progress_bar.clone();
    let installed_count = ctx.installed_count.clone();
    let ctx = ctx.clone();

    tokio::spawn(async move {
        let result =
            install_single_package(&ctx, &target, progress_callback, core_db.clone()).await;

        if let Err(err) = result {
            match err {
                SoarError::Warning(err) => {
                    let mut warnings = ctx.warnings.lock().unwrap();
                    warnings.push(err);

                    if !keep {
                        let _ = remove_old_package(&target.package, core_db.clone());
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
                let _ = remove_old_package(&target.package, core_db.clone());
            }
        }

        drop(permit);
    })
}

fn remove_old_package(package: &Package, core_db: Arc<Mutex<Connection>>) -> SoarResult<()> {
    let conn = core_db.lock()?;

    let Package {
        pkg_id,
        pkg_name,
        repo_name,
        ..
    } = package;

    let mut stmt = conn.prepare(
        "SELECT installed_path
        FROM packages
        WHERE
            pkg_id = ?
            AND pkg_name = ?
            AND repo_name = ?
            AND pinned = 0
        AND rowid NOT IN (
            SELECT rowid
            FROM packages
            WHERE
                pkg_id = ?
                AND pkg_name = ?
                AND repo_name = ?
            ORDER BY rowid DESC
            LIMIT 1
        )",
    )?;

    let paths: Vec<String> = stmt
        .query_map(
            [pkg_id, pkg_name, repo_name, pkg_id, pkg_name, repo_name],
            |row| row.get(0),
        )?
        .filter_map(Result::ok)
        .collect();

    for path in paths {
        let path = Path::new(&path);
        if path.exists() {
            fs::remove_dir_all(path)
                .with_context(|| format!("removing directory {}", path.display()))?;
        }
    }

    let mut stmt = prepare_and_bind!(
        conn,
        "DELETE FROM packages
        WHERE rowid NOT IN (
            SELECT rowid
            FROM packages
            WHERE
                repo_name = $repo_name
                AND pkg_id = $pkg_id
                AND pkg_name = $pkg_name
            ORDER BY rowid DESC
            LIMIT 1
        )
        AND pkg_id = $pkg_id
        AND pkg_name = $pkg_name
        AND pinned = 0
        "
    );
    stmt.raw_execute()?;

    Ok(())
}
