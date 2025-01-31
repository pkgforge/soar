use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use rusqlite::{prepare_and_bind, Connection};
use soar_dl::downloader::{DownloadOptions, DownloadState, Downloader, OciDownloadOptions};

use crate::{
    config::get_config,
    database::{
        models::{InstalledPackage, Package},
        packages::{FilterCondition, PackageQueryBuilder, ProvideStrategy},
    },
    SoarResult,
};

pub struct PackageInstaller {
    package: Package,
    install_dir: PathBuf,
    progress_callback: Option<Arc<dyn Fn(DownloadState) + Send + Sync>>,
    db: Arc<Mutex<Connection>>,
    with_pkg_id: bool,
}

#[derive(Clone)]
pub struct InstallTarget {
    pub package: Package,
    pub existing_install: Option<InstalledPackage>,
    pub with_pkg_id: bool,
    pub profile: Option<String>,
}

impl PackageInstaller {
    pub async fn new<P: AsRef<Path>>(
        target: &InstallTarget,
        install_dir: P,
        progress_callback: Option<Arc<dyn Fn(DownloadState) + Send + Sync>>,
        db: Arc<Mutex<Connection>>,
        with_pkg_id: bool,
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
                ref pkg_type,
                ref version,
                ref ghcr_size,
                ref size,
                bsum,
                ..
            } = package;
            let installed_path = install_dir.to_string_lossy();
            let size = ghcr_size.unwrap_or(*size);
            let mut stmt = prepare_and_bind!(
                conn,
                "INSERT INTO packages (
                repo_name, pkg, pkg_id, pkg_name, pkg_type, version, size,
                checksum, installed_path, installed_date, with_pkg_id, profile
            )
            VALUES
            (
                $repo_name, $pkg, $pkg_id, $pkg_name, $pkg_type, $version, $size,
                $bsum, $installed_path, datetime(), $with_pkg_id, $profile
            )"
            );
            stmt.raw_execute()?;
        }

        Ok(Self {
            package: package.clone(),
            install_dir,
            progress_callback,
            db: db.clone(),
            with_pkg_id,
        })
    }

    pub async fn install(&self) -> SoarResult<()> {
        let package = &self.package;
        let output_path = self.install_dir.join(&package.pkg_name);

        self.download_package(&output_path).await?;

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

        if self.package.ghcr_pkg.is_some() {
            let options = OciDownloadOptions {
                url: url.to_string(),
                output_path: Some(output_path.to_string_lossy().to_string()),
                progress_callback: self.progress_callback.clone(),
                api: None,
                concurrency: Some(8),
                regex_patterns: Vec::new(),
                exclude_keywords: Vec::new(),
                match_keywords: Vec::new(),
                exact_case: false,
            };
            downloader.download_oci(options).await?;
        } else {
            let options = DownloadOptions {
                url: url.to_string(),
                output_path: Some(output_path.to_string_lossy().to_string()),
                progress_callback: self.progress_callback.clone(),
            };
            downloader.download(options).await?;
        }

        Ok(())
    }

    pub async fn record<P: AsRef<Path>>(
        &self,
        bin_path: P,
        icon_path: Option<PathBuf>,
        desktop_path: Option<PathBuf>,
        unlinked: bool,
    ) -> SoarResult<()> {
        let mut conn = self.db.lock()?;
        let package = &self.package;
        let bin_path = bin_path.as_ref().to_string_lossy();
        let icon_path = icon_path.map(|path| path.to_string_lossy().into_owned());
        let desktop_path = desktop_path.map(|path| path.to_string_lossy().into_owned());
        let Package {
            repo_name,
            pkg_name,
            pkg_id,
            bsum: checksum,
            ..
        } = package;
        let provides = serde_json::to_string(&package.provides).unwrap();

        let with_pkg_id = self.with_pkg_id;
        let tx = conn.transaction()?;

        {
            let mut stmt = prepare_and_bind!(
                tx,
                "UPDATE packages
                SET
                    bin_path = $bin_path,
                    icon_path = $icon_path,
                    desktop_path = $desktop_path,
                    installed_date = datetime(),
                    is_installed = true,
                    provides = $provides,
                    with_pkg_id = $with_pkg_id
                WHERE
                    repo_name = $repo_name
                    AND pkg_name = $pkg_name
                    AND pkg_id = $pkg_id
                    AND checksum = $checksum
            "
            );
            stmt.raw_execute()?;
        }

        if !unlinked {
            let mut stmt = prepare_and_bind!(
                tx,
                "UPDATE packages
                SET
                    unlinked = true
                WHERE
                    pkg_name = $pkg_name
                    AND (
                        pkg_id != $pkg_id
                        OR
                        checksum != $checksum
                    )"
            );
            stmt.raw_execute()?;
        }

        tx.commit()?;
        drop(conn);

        if !unlinked {
            let alternate_packages = PackageQueryBuilder::new(self.db.clone())
                .where_and("pkg_name", FilterCondition::Eq(pkg_name.to_owned()))
                .where_and("pkg_id", FilterCondition::Ne(pkg_id.to_owned()))
                .where_and("checksum", FilterCondition::Ne(checksum.to_owned()))
                .load_installed()?
                .items;

            for package in alternate_packages {
                if let Some(alt_path) = package.desktop_path {
                    let alt_pathbuf = PathBuf::from(&alt_path);

                    let should_remove = desktop_path
                        .as_ref()
                        .map(|dp| dp != &alt_path)
                        .unwrap_or(true);

                    if should_remove && (alt_pathbuf.is_symlink() || alt_pathbuf.is_file()) {
                        fs::remove_file(&alt_path)?;
                    }
                }

                if let Some(alt_path) = package.icon_path {
                    let alt_pathbuf = PathBuf::from(&alt_path);

                    let should_remove =
                        icon_path.as_ref().map(|dp| dp != &alt_path).unwrap_or(true);

                    if should_remove && (alt_pathbuf.is_symlink() || alt_pathbuf.is_file()) {
                        fs::remove_file(&alt_path)?;
                    }
                }

                if let Some(provides) = package.provides {
                    for provide in provides {
                        if let Some(ref target) = provide.target {
                            let is_symlink = match provide.strategy {
                                Some(ProvideStrategy::KeepTargetOnly)
                                | Some(ProvideStrategy::KeepBoth) => true,
                                _ => false,
                            };
                            if is_symlink {
                                let target_name = get_config().get_bin_path()?.join(&target);
                                if target_name.is_symlink() || target_name.is_file() {
                                    std::fs::remove_file(&target_name)?;
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
