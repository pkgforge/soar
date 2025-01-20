use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use rusqlite::{prepare_and_bind, Connection};
use soar_dl::downloader::{DownloadOptions, DownloadState, Downloader};

use crate::{
    config::get_config,
    database::models::{InstalledPackage, Package},
    utils::validate_checksum,
    SoarResult,
};

pub struct PackageInstaller {
    package: Package,
    install_dir: PathBuf,
    progress_callback: Option<Arc<dyn Fn(DownloadState) + Send + Sync>>,
    db: Arc<Mutex<Connection>>,
    installed_with_family: bool,
}

#[derive(Clone)]
pub struct InstallTarget {
    pub package: Package,
    pub existing_install: Option<InstalledPackage>,
}

impl PackageInstaller {
    pub async fn new<P: AsRef<Path>>(
        target: &InstallTarget,
        install_dir: P,
        progress_callback: Option<Arc<dyn Fn(DownloadState) + Send + Sync>>,
        db: Arc<Mutex<Connection>>,
        installed_with_family: bool,
    ) -> SoarResult<Self> {
        let install_dir = install_dir.as_ref().to_path_buf();
        let package = &target.package;
        let profile = get_config().default_profile.clone();

        if target.existing_install.is_none() {
            let conn = db.lock()?;
            let Package {
                ref repo_name,
                ref pkg,
                ref pkg_id,
                ref pkg_name,
                ref version,
                ref size,
                ref checksum,
                ..
            } = package;
            let installed_path = install_dir.to_string_lossy();
            let mut stmt = prepare_and_bind!(
                conn,
                "INSERT INTO packages (
                repo_name, pkg, pkg_id, pkg_name, version, size, checksum,
                installed_path, installed_with_family, profile
            )
            VALUES
            (
                $repo_name, $pkg, $pkg_id, $pkg_name, $version, $size, $checksum,
                $installed_path, $installed_with_family, $profile
            )"
            );
            stmt.raw_execute()?;
        }

        Ok(Self {
            package: package.clone(),
            install_dir,
            progress_callback,
            db: db.clone(),
            installed_with_family,
        })
    }

    pub async fn install(&self) -> SoarResult<()> {
        let package = &self.package;
        let output_path = self.install_dir.join(&package.pkg_name);

        self.download_package(&output_path).await?;

        validate_checksum(&package.checksum, &output_path)?;

        Ok(())
    }

    async fn download_package<P: AsRef<Path>>(&self, output_path: P) -> SoarResult<()> {
        let downloader = Downloader::default();
        let output_path = output_path.as_ref();

        // fallback to download_url for repositories without ghcr
        let (url, output_path) = if let Some(ref ghcr_pkg) = self.package.ghcr_pkg {
            (ghcr_pkg, &self.install_dir)
        } else {
            (&self.package.download_url, &output_path.to_path_buf())
        };

        let options = DownloadOptions {
            url: url.to_string(),
            output_path: Some(output_path.to_string_lossy().to_string()),
            progress_callback: self.progress_callback.clone(),
        };

        if self.package.ghcr_pkg.is_some() {
            downloader.download_oci(options).await?;
        } else {
            downloader.download(options).await?;
        }

        Ok(())
    }

    pub async fn record<P: AsRef<Path>>(
        &self,
        final_checksum: &str,
        bin_path: P,
        icon_path: Option<PathBuf>,
        desktop_path: Option<PathBuf>,
    ) -> SoarResult<()> {
        let conn = self.db.lock()?;
        let package = &self.package;
        let bin_path = bin_path.as_ref().to_string_lossy();
        let icon_path = icon_path.map(|path| path.to_string_lossy().into_owned());
        let desktop_path = desktop_path.map(|path| path.to_string_lossy().into_owned());
        let Package {
            pkg_name, checksum, ..
        } = package;
        let provides = serde_json::to_string(&package.provides).unwrap();

        let installed_with_family = self.installed_with_family;
        let mut stmt = prepare_and_bind!(
            conn,
            "UPDATE packages
            SET
                bin_path = $bin_path,
                icon_path = $icon_path,
                desktop_path = $desktop_path,
                checksum = $final_checksum,
                installed_date = datetime(),
                is_installed = true,
                provides = $provides,
                installed_with_family = $installed_with_family
            WHERE
                pkg_name = $pkg_name
                AND
                checksum = $checksum
            "
        );
        stmt.raw_execute()?;

        Ok(())
    }
}
