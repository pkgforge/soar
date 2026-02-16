use std::path::PathBuf;

use soar_core::{package::remove::PackageRemover, SoarResult};
use soar_db::repository::core::CoreRepository;
use soar_events::{RemoveStage, SoarEvent};
use soar_utils::{error::FileSystemResult, fs::walk_dir};
use tracing::debug;

use crate::{
    progress::next_op_id, utils::get_package_hooks, BrokenPackage, FailedInfo, HealthReport,
    RemoveReport, RemovedInfo, SoarContext,
};

/// Check system health: PATH configuration, broken packages, and broken symlinks.
pub fn check_health(ctx: &SoarContext) -> SoarResult<HealthReport> {
    debug!("checking system health");
    let config = ctx.config();
    let bin_path = config.get_bin_path()?;

    let path_env = std::env::var("PATH").unwrap_or_default();
    let path_configured = path_env
        .split(':')
        .any(|p| std::path::Path::new(p) == bin_path);

    let broken_packages = get_broken_packages(ctx)?;
    let broken_symlinks = get_broken_symlinks(ctx)?;

    Ok(HealthReport {
        path_configured,
        bin_path,
        broken_packages,
        broken_symlinks,
    })
}

/// Remove all broken packages (those whose installed_path no longer exists).
pub async fn remove_broken_packages(ctx: &SoarContext) -> SoarResult<RemoveReport> {
    debug!("removing broken packages");
    let diesel_db = ctx.diesel_core_db()?.clone();

    let broken = diesel_db.with_conn(CoreRepository::list_broken)?;

    let mut removed = Vec::new();
    let mut failed = Vec::new();

    for package in broken {
        let op_id = next_op_id();
        let pkg_name = package.pkg_name.clone();
        let pkg_id = package.pkg_id.clone();
        let repo_name = package.repo_name.clone();
        let version = package.version.clone();

        ctx.events().emit(SoarEvent::Removing {
            op_id,
            pkg_name: pkg_name.clone(),
            pkg_id: pkg_id.clone(),
            stage: RemoveStage::RemovingDirectory,
        });

        let (hooks, sandbox) = get_package_hooks(&pkg_name);
        let installed_pkg = package.into();
        let remover = PackageRemover::new(installed_pkg, diesel_db.clone(), ctx.config().clone())
            .await
            .with_hooks(hooks)
            .with_sandbox(sandbox);

        match remover.remove().await {
            Ok(()) => {
                ctx.events().emit(SoarEvent::Removing {
                    op_id,
                    pkg_name: pkg_name.clone(),
                    pkg_id: pkg_id.clone(),
                    stage: RemoveStage::Complete {
                        size_freed: None,
                    },
                });
                removed.push(RemovedInfo {
                    pkg_name,
                    pkg_id,
                    repo_name,
                    version,
                });
            }
            Err(err) => {
                ctx.events().emit(SoarEvent::OperationFailed {
                    op_id,
                    pkg_name: pkg_name.clone(),
                    pkg_id: pkg_id.clone(),
                    error: err.to_string(),
                });
                failed.push(FailedInfo {
                    pkg_name,
                    pkg_id,
                    error: err.to_string(),
                });
            }
        }
    }

    Ok(RemoveReport {
        removed,
        failed,
    })
}

/// Remove broken symlinks in bin, desktop, and icons directories.
pub fn remove_broken_symlinks(ctx: &SoarContext) -> SoarResult<Vec<PathBuf>> {
    let broken = get_broken_symlinks(ctx)?;

    let mut removed = Vec::new();
    for path in &broken {
        if std::fs::remove_file(path).is_ok() {
            removed.push(path.clone());
        }
    }

    Ok(removed)
}

fn get_broken_packages(ctx: &SoarContext) -> SoarResult<Vec<BrokenPackage>> {
    let diesel_db = ctx.diesel_core_db()?;
    let broken = diesel_db.with_conn(CoreRepository::list_broken)?;

    Ok(broken
        .into_iter()
        .map(|p| {
            BrokenPackage {
                pkg_name: p.pkg_name,
                pkg_id: p.pkg_id,
                installed_path: p.installed_path,
            }
        })
        .collect())
}

fn get_broken_symlinks(ctx: &SoarContext) -> SoarResult<Vec<PathBuf>> {
    let config = ctx.config();
    let mut broken = Vec::new();

    let bin_path = config.get_bin_path()?;
    walk_dir(
        &bin_path,
        &mut |path: &std::path::Path| -> FileSystemResult<()> {
            if !path.exists() {
                broken.push(path.to_path_buf());
            }
            Ok(())
        },
    )?;

    let desktop_path = config.get_desktop_path()?;
    let mut soar_check = |path: &std::path::Path| -> FileSystemResult<()> {
        if let Some(filename) = path.file_stem().and_then(|s| s.to_str()) {
            if filename.ends_with("-soar") && !path.exists() {
                broken.push(path.to_path_buf());
            }
        }
        Ok(())
    };

    walk_dir(&desktop_path, &mut soar_check)?;
    walk_dir(config.get_icons_path(), &mut soar_check)?;

    Ok(broken)
}
