use std::{
    fs,
    sync::{Arc, Mutex},
};

use rusqlite::{params, Connection};

use crate::{database::models::InstalledPackage, SoarResult};

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
            DELETE FROM packages WHERE id = ? AND is_installed = true
        "#,
        )?;

        // if the package is installed, it does have bin_path
        fs::remove_file(&self.package.bin_path.clone().unwrap())?;

        if let Some(ref icon_path) = self.package.icon_path {
            let _ = fs::remove_file(icon_path);
        }

        if let Some(ref desktop_path) = self.package.desktop_path {
            let _ = fs::remove_file(desktop_path);
        }

        if let Some(ref appstream_path) = self.package.appstream_path {
            let _ = fs::remove_file(appstream_path);
        }

        fs::remove_dir_all(&self.package.installed_path)?;

        stmt.execute(params![self.package.id])?;

        Ok(())
    }
}
