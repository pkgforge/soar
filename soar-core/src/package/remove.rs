use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use rusqlite::{params, Connection};

use crate::{
    config::get_config,
    database::{models::InstalledPackage, packages::ProvideStrategy},
    error::ErrorContext,
    utils::{desktop_dir, icons_dir, process_dir},
    SoarResult,
};

pub struct PackageRemover {
    package: InstalledPackage,
    db: Arc<Mutex<Connection>>,
}

impl PackageRemover {
    pub async fn new(package: InstalledPackage, db: Arc<Mutex<Connection>>) -> Self {
        Self { package, db }
    }

    pub async fn remove(&self) -> SoarResult<()> {
        let mut conn = self.db.lock()?;
        let tx = conn.transaction()?;

        // to prevent accidentally removing required files by other package,
        // remove only if the installation was successful
        if self.package.is_installed {
            let bin_path = get_config().get_bin_path()?;
            let def_bin = bin_path.join(&self.package.pkg_name);
            if def_bin.is_symlink() && def_bin.is_file() {
                fs::remove_file(&def_bin)
                    .with_context(|| format!("removing binary {}", def_bin.display()))?;
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
                                std::fs::remove_file(&target_name).with_context(|| {
                                    format!("removing provide {}", target_name.display())
                                })?;
                            }
                        }
                    }
                }
            }

            let installed_path = PathBuf::from(&self.package.installed_path);

            let mut remove_action = |path: &Path| -> SoarResult<()> {
                if path.extension() == Some(&OsString::from("desktop")) {
                    if let Ok(real_path) = fs::read_link(path) {
                        if real_path.parent() == Some(&installed_path) {
                            let _ = fs::remove_file(path);
                        }
                    }
                }
                Ok(())
            };
            process_dir(desktop_dir(), &mut remove_action)?;

            let mut remove_action = |path: &Path| -> SoarResult<()> {
                if let Ok(real_path) = fs::read_link(path) {
                    if real_path.parent() == Some(&installed_path) {
                        let _ = fs::remove_file(path);
                    }
                }
                Ok(())
            };
            process_dir(icons_dir(), &mut remove_action)?;
        }

        if let Err(err) = fs::remove_dir_all(&self.package.installed_path) {
            // if not found, the package is already removed.
            if err.kind() != std::io::ErrorKind::NotFound {
                return Err(err).with_context(|| {
                    format!("removing package directory {}", self.package.installed_path)
                })?;
            }
        };

        {
            let mut stmt = tx.prepare(
                r#"
                DELETE FROM packages WHERE id = ?
            "#,
            )?;
            stmt.execute(params![self.package.id])?;

            let mut stmt = tx.prepare(
                r#"
                DELETE FROM portable_package WHERE package_id = ?
            "#,
            )?;
            stmt.execute(params![self.package.id])?;
        }

        tx.commit()?;

        Ok(())
    }
}
