use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use rusqlite::{params, Connection};

use crate::{
    database::{models::InstalledPackage, packages::ProvideStrategy},
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
        let conn = self.db.lock()?;
        let mut stmt = conn.prepare(
            r#"
            DELETE FROM packages WHERE id = ?
        "#,
        )?;

        // to prevent accidentally removing required files by other package,
        // remove only if the installation was successful
        if self.package.is_installed {
            // if the package is installed, it does have bin_path
            let bin_path = PathBuf::from(self.package.bin_path.clone().unwrap());
            let def_bin = bin_path.join(&self.package.pkg_name);
            if def_bin.is_symlink() && def_bin.is_file() {
                fs::remove_file(def_bin)?;
            }

            if let Some(provides) = &self.package.provides {
                for provide in provides {
                    if let Some(ref target) = provide.target {
                        let is_symlink = match provide.strategy {
                            Some(ProvideStrategy::KeepTargetOnly)
                            | Some(ProvideStrategy::KeepBoth) => true,
                            _ => false,
                        };
                        if is_symlink {
                            let target_name = bin_path.join(&target);
                            if target_name.exists() {
                                std::fs::remove_file(&target_name)?;
                            }
                        }
                    }
                }
            }

            if let Some(ref icon_path) = self.package.icon_path {
                let _ = fs::remove_file(icon_path);
            }

            if let Some(ref desktop_path) = self.package.desktop_path {
                let _ = fs::remove_file(desktop_path);
            }

            if let Some(ref appstream_path) = self.package.appstream_path {
                let _ = fs::remove_file(appstream_path);
            }
        }

        if let Err(err) = fs::remove_dir_all(&self.package.installed_path) {
            // if not found, the package is already removed.
            if err.kind() != std::io::ErrorKind::NotFound {
                return Err(err)?;
            }
        };

        stmt.execute(params![self.package.id])?;

        Ok(())
    }
}
