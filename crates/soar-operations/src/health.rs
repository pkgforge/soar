use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
};

use soar_core::{SoarResult, package::remove::PackageRemover};
use soar_db::repository::core::CoreRepository;
use soar_events::{RemoveStage, SoarEvent};
use soar_utils::{error::FileSystemResult, fs::walk_dir, path::resolve_path};
use tracing::debug;

use crate::{
    BrokenPackage, FailedInfo, HealthReport, RemoveReport, RemovedInfo, SoarContext,
    progress::next_op_id, utils::get_package_hooks,
};

/// Check system health: PATH configuration, broken packages, and broken symlinks.
pub fn check_health(ctx: &SoarContext) -> SoarResult<HealthReport> {
    debug!("checking system health");
    let config = ctx.config();
    let bin_path = config.get_bin_path()?;

    let path_env = std::env::var("PATH").unwrap_or_default();
    let path_configured = path_env
        .split(':')
        .any(|p| resolve_path(p).unwrap_or_default() == bin_path);

    // Whether `man` can find what soar installed. An explicit MANPATH replaces
    // everything man-db would work out for itself, so a package's manual pages
    // can be installed correctly and still be invisible.
    let man_dir = bin_path.parent().unwrap_or(&bin_path).join("share/man");
    let man_path = man_dir.is_dir().then(|| man_dir.clone());
    let man_path_configured = match &man_path {
        None => true,
        Some(dir) => {
            let listed = |value: String| {
                value
                    .split(':')
                    .any(|p| !p.is_empty() && resolve_path(p).unwrap_or_default() == *dir)
            };
            let from_env = std::env::var("MANPATH").map(listed).unwrap_or(false);
            from_env
                || std::process::Command::new("manpath")
                    .output()
                    .ok()
                    .and_then(|o| String::from_utf8(o.stdout).ok())
                    .map(|out| listed(out.trim().to_string()))
                    .unwrap_or(false)
        }
    };

    let broken_packages = get_broken_packages(ctx)?;
    let broken_symlinks = get_broken_symlinks(ctx)?;

    Ok(HealthReport {
        path_configured,
        bin_path,
        man_path,
        man_path_configured,
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
        let repo_name = package.repo_name.clone();
        let version = package.version.clone();

        ctx.events().emit(SoarEvent::Removing {
            op_id,
            pkg_name: pkg_name.clone(),
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
                    stage: RemoveStage::Complete {
                        size_freed: None,
                    },
                });
                removed.push(RemovedInfo {
                    pkg_name,
                    repo_name,
                    version,
                });
            }
            Err(err) => {
                ctx.events().emit(SoarEvent::OperationFailed {
                    op_id,
                    pkg_name: pkg_name.clone(),
                    error: err.to_string(),
                });
                failed.push(FailedInfo {
                    pkg_name,
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
    for path in broken {
        // The link may have been repaired since the scan.
        if is_dangling(&path) && std::fs::remove_file(&path).is_ok() {
            removed.push(path);
        }
    }

    Ok(removed)
}

/// Whether `path` is a symlink whose target does not exist.
///
/// Any other failure to read the target, such as a permission error, leaves
/// the link alone, since its target may well be there.
fn is_dangling(path: &Path) -> bool {
    path.is_symlink()
        && std::fs::metadata(path)
            .is_err_and(|e| matches!(e.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory))
}

fn get_broken_packages(ctx: &SoarContext) -> SoarResult<Vec<BrokenPackage>> {
    let diesel_db = ctx.diesel_core_db()?;
    let broken = diesel_db.with_conn(CoreRepository::list_broken)?;

    Ok(broken
        .into_iter()
        .map(|p| {
            BrokenPackage {
                pkg_name: p.pkg_name,
                installed_path: p.installed_path,
            }
        })
        .collect())
}

fn get_broken_symlinks(ctx: &SoarContext) -> SoarResult<Vec<PathBuf>> {
    let config = ctx.config();
    let mut broken = Vec::new();

    // A directory that does not exist holds nothing broken. Walking it is an
    // error, and reporting that as a failed health check tells the user
    // something is wrong when nothing is.
    let bin_path = config.get_bin_path()?;
    if bin_path.is_dir() {
        walk_dir(&bin_path, &mut |path: &Path| -> FileSystemResult<()> {
            if is_dangling(path) {
                broken.push(path.to_path_buf());
            }
            Ok(())
        })?;
    }

    let desktop_path = config.get_desktop_path()?;
    let mut soar_check = |path: &Path| -> FileSystemResult<()> {
        if let Some(filename) = path.file_stem().and_then(|s| s.to_str())
            && filename.ends_with("-soar")
            && is_dangling(path)
        {
            broken.push(path.to_path_buf());
        }
        Ok(())
    };

    if desktop_path.is_dir() {
        walk_dir(&desktop_path, &mut soar_check)?;
    }
    let icons_path = config.get_icons_path();
    if icons_path.is_dir() {
        walk_dir(&icons_path, &mut soar_check)?;
    }

    Ok(broken)
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        os::unix::fs::{PermissionsExt, symlink},
    };

    use super::*;

    #[test]
    fn only_links_to_missing_targets_are_dangling() {
        let dir = tempfile::tempdir().unwrap();
        let p = |name: &str| dir.path().join(name);
        fs::write(p("file"), "").unwrap();
        symlink(p("file"), p("good")).unwrap();
        symlink(p("gone"), p("missing")).unwrap();
        symlink(p("file/below"), p("through-file")).unwrap();

        assert!(!is_dangling(&p("file")));
        assert!(!is_dangling(&p("good")));
        assert!(is_dangling(&p("missing")));
        assert!(is_dangling(&p("through-file")));

        fs::create_dir(p("locked")).unwrap();
        fs::write(p("locked/target"), "").unwrap();
        symlink(p("locked/target"), p("unreadable")).unwrap();
        fs::set_permissions(p("locked"), fs::Permissions::from_mode(0o000)).unwrap();
        // Root reads through any permission, so the case cannot arise there.
        if fs::metadata(p("unreadable")).is_err() {
            assert!(!is_dangling(&p("unreadable")));
        }
        fs::set_permissions(p("locked"), fs::Permissions::from_mode(0o755)).unwrap();
    }
}
