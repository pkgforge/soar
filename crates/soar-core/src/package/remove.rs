use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

use soar_config::config::get_config;
use soar_db::{models::types::ProvideStrategy, repository::core::CoreRepository};
use soar_utils::{error::FileSystemResult, fs::walk_dir, path::desktop_dir};
use tracing::{debug, trace, warn};

/// Formats bytes into human-readable string (e.g., "1.5 MiB")
fn format_size(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    const GIB: u64 = MIB * 1024;

    if bytes >= GIB {
        format!("{:.2} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.2} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.2} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{} B", bytes)
    }
}

use crate::{
    database::{connection::DieselDatabase, models::InstalledPackage},
    error::ErrorContext,
    SoarResult,
};

pub struct PackageRemover {
    package: InstalledPackage,
    db: DieselDatabase,
}

impl PackageRemover {
    pub async fn new(package: InstalledPackage, db: DieselDatabase) -> Self {
        trace!(
            pkg_name = package.pkg_name,
            pkg_id = package.pkg_id,
            "creating package remover"
        );
        Self {
            package,
            db,
        }
    }

    pub async fn remove(&self) -> SoarResult<()> {
        debug!(
            pkg_name = self.package.pkg_name,
            pkg_id = self.package.pkg_id,
            version = self.package.version,
            repo = self.package.repo_name,
            installed_path = self.package.installed_path,
            "removing {}#{}:{} ({})",
            self.package.pkg_name,
            self.package.pkg_id,
            self.package.repo_name,
            self.package.version
        );
        // Track removed symlinks for logging
        let mut removed_symlinks: Vec<PathBuf> = Vec::new();

        // to prevent accidentally removing required files by other package,
        // remove only if the installation was successful
        if self.package.is_installed {
            trace!("package was installed, removing binaries and links");
            let bin_path = get_config().get_bin_path()?;
            let def_bin = bin_path.join(&self.package.pkg_name);
            if def_bin.is_symlink() && def_bin.is_file() {
                trace!("removing binary symlink: {}", def_bin.display());
                fs::remove_file(&def_bin)
                    .with_context(|| format!("removing binary {}", def_bin.display()))?;
                removed_symlinks.push(def_bin);
            }

            if let Some(provides) = &self.package.provides {
                for provide in provides {
                    if let Some(ref target) = provide.target {
                        let is_symlink = matches!(
                            provide.strategy,
                            Some(ProvideStrategy::KeepTargetOnly) | Some(ProvideStrategy::KeepBoth)
                        );
                        if is_symlink {
                            let target_name = bin_path.join(target);
                            if target_name.exists() {
                                trace!("removing provide symlink: {}", target_name.display());
                                std::fs::remove_file(&target_name).with_context(|| {
                                    format!("removing provide {}", target_name.display())
                                })?;
                                removed_symlinks.push(target_name);
                            }
                        }
                    }
                }
            }

            let installed_path = PathBuf::from(&self.package.installed_path);

            let mut remove_action = |path: &Path| -> FileSystemResult<()> {
                if path.extension() == Some(&OsString::from("desktop")) {
                    if let Ok(real_path) = fs::read_link(path) {
                        if real_path.parent() == Some(&installed_path) {
                            trace!("removing desktop file: {}", path.display());
                            let _ = fs::remove_file(path);
                        }
                    }
                }
                Ok(())
            };
            walk_dir(desktop_dir(), &mut remove_action)?;

            let mut remove_action = |path: &Path| -> FileSystemResult<()> {
                if let Ok(real_path) = fs::read_link(path) {
                    if real_path.parent() == Some(&installed_path) {
                        trace!("removing symlink: {}", path.display());
                        let _ = fs::remove_file(path);
                    }
                }
                Ok(())
            };
            walk_dir(desktop_dir(), &mut remove_action)?;
        }

        // Calculate directory size before removal for logging
        let dir_size = fs::read_dir(&self.package.installed_path)
            .ok()
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter_map(|e| e.metadata().ok())
                    .filter(|m| m.is_file())
                    .map(|m| m.len())
                    .sum::<u64>()
            });

        let size_str = dir_size
            .map(format_size)
            .unwrap_or_else(|| "unknown".to_string());
        trace!(
            "removing package directory: {} ({})",
            self.package.installed_path,
            size_str
        );
        if let Err(err) = fs::remove_dir_all(&self.package.installed_path) {
            // if not found, the package is already removed.
            if err.kind() != std::io::ErrorKind::NotFound {
                return Err(err).with_context(|| {
                    format!("removing package directory {}", self.package.installed_path)
                })?;
            } else {
                warn!(
                    "package directory already removed: {}",
                    self.package.installed_path
                );
            }
        };

        trace!("removing package from database");
        let package_id = self.package.id as i32;
        self.db.transaction(|conn| {
            CoreRepository::delete_portable(conn, package_id)?;
            CoreRepository::delete(conn, package_id)
        })?;

        // Log removed symlinks at debug level
        for symlink in &removed_symlinks {
            debug!("removed symlink: {}", symlink.display());
        }

        debug!(
            "removed {}#{}:{} ({}) - reclaimed {}",
            self.package.pkg_name,
            self.package.pkg_id,
            self.package.repo_name,
            self.package.version,
            size_str
        );
        Ok(())
    }
}
