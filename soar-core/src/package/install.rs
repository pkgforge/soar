use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use chrono::Utc;
use rusqlite::{params, Connection};
use soar_dl::downloader::{DownloadOptions, DownloadState, Downloader};

use crate::{database::models::Package, utils::validate_checksum, SoarResult};

pub struct PackageInstaller {
    package: Package,
    install_dir: PathBuf,
    progress_callback: Option<Arc<dyn Fn(DownloadState) + Send + Sync>>,
    db: Arc<Mutex<Connection>>,
    installed_with_family: bool,
}

impl PackageInstaller {
    pub async fn new<P: AsRef<Path>>(
        package: Package,
        install_dir: P,
        progress_callback: Option<Arc<dyn Fn(DownloadState) + Send + Sync>>,
        db: Arc<Mutex<Connection>>,
        installed_with_family: bool,
    ) -> SoarResult<Self> {
        let install_dir = install_dir.as_ref().to_path_buf();
        {
            let conn = db.lock()?;
            let mut stmt = conn.prepare(
                r#"
                INSERT OR IGNORE INTO packages (
                    repo_name, collection, family, pkg_name,
                    pkg, pkg_id, app_id, description,
                    version, size, checksum, build_date,
                    build_script, build_log, category,
                    installed_path, installed_with_family
                )
                VALUES 
                (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                    ?11, ?12, ?13, ?14, ?15, ?16, ?17
                )
                "#,
            )?;
            stmt.execute(params![
                package.repo_name,
                package.collection,
                package.family,
                package.pkg_name,
                package.pkg,
                package.pkg_id,
                package.app_id,
                package.description,
                package.version,
                package.size,
                package.checksum,
                package.build_date,
                package.build_script,
                package.build_log,
                package.category,
                install_dir.to_string_lossy(),
                installed_with_family
            ])?;
        }
        Ok(Self {
            package,
            install_dir,
            progress_callback,
            db: db.clone(),
            installed_with_family,
        })
    }

    pub async fn install(&self) -> SoarResult<()> {
        let package = &self.package;
        let output_path = self.install_dir.join(&package.pkg);

        self.download_package(&output_path).await?;

        validate_checksum(&package.checksum, &output_path)?;

        Ok(())
    }

    async fn download_package<P: AsRef<Path>>(&self, output_path: P) -> SoarResult<()> {
        let downloader = Downloader::default();
        let options = DownloadOptions {
            url: self.package.download_url.clone(),
            output_path: Some(output_path.as_ref().to_string_lossy().to_string()),
            progress_callback: self.progress_callback.clone(),
        };

        downloader.download(options).await?;

        Ok(())
    }

    pub async fn record<P: AsRef<Path>>(
        &self,
        final_checksum: &str,
        bin_path: P,
    ) -> SoarResult<()> {
        let conn = self.db.lock()?;
        let package = &self.package;
        let mut stmt = conn.prepare(
            r#"
                UPDATE packages
                SET
                    bin_path = ?4,
                    checksum = ?5,
                    installed_date = ?6,
                    is_installed = ?7,
                    installed_with_family = ?8
                WHERE
                    family = ?1 AND pkg_name = ?2 AND checksum = ?3
                "#,
        )?;
        let now = Utc::now().timestamp_millis();
        stmt.execute(params![
            package.family,
            package.pkg_name,
            package.checksum,
            bin_path.as_ref().to_string_lossy(),
            final_checksum,
            now,
            true,
            self.installed_with_family
        ])?;

        Ok(())
    }
}
