use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread::sleep,
    time::Duration,
};

use rusqlite::{params, prepare_and_bind, Connection};
use soar_config::config::get_config;
use soar_dl::{
    download::Download,
    error::DownloadError,
    filter::Filter,
    oci::OciDownload,
    types::{OverwriteMode, Progress},
};
use soar_utils::{
    error::FileSystemResult,
    fs::{safe_remove, walk_dir},
    hash::calculate_checksum,
    path::{desktop_dir, icons_dir},
};

use crate::{
    database::{
        models::{InstalledPackage, Package},
        packages::{FilterCondition, PackageQueryBuilder, ProvideStrategy},
    },
    error::{ErrorContext, SoarError},
    utils::get_extract_dir,
    SoarResult,
};

pub struct PackageInstaller {
    package: Package,
    install_dir: PathBuf,
    progress_callback: Option<Arc<dyn Fn(Progress) + Send + Sync>>,
    db: Arc<Mutex<Connection>>,
    with_pkg_id: bool,
    globs: Vec<String>,
}

#[derive(Clone, Default)]
pub struct InstallTarget {
    pub package: Package,
    pub existing_install: Option<InstalledPackage>,
    pub with_pkg_id: bool,
    pub profile: Option<String>,
    pub portable: Option<String>,
    pub portable_home: Option<String>,
    pub portable_config: Option<String>,
    pub portable_share: Option<String>,
    pub portable_cache: Option<String>,
}

impl PackageInstaller {
    pub async fn new<P: AsRef<Path>>(
        target: &InstallTarget,
        install_dir: P,
        progress_callback: Option<Arc<dyn Fn(Progress) + Send + Sync>>,
        db: Arc<Mutex<Connection>>,
        with_pkg_id: bool,
        globs: Vec<String>,
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
                ..
            } = package;
            let installed_path = install_dir.to_string_lossy();
            let size = ghcr_size.unwrap_or(size.unwrap_or(0));
            let install_patterns = serde_json::to_string(&globs).unwrap();
            let mut stmt = prepare_and_bind!(
                conn,
                "INSERT INTO packages (
                    repo_name, pkg, pkg_id, pkg_name, pkg_type, version, size,
                    installed_path, installed_date, with_pkg_id, profile, install_patterns
                )
                VALUES
                (
                    $repo_name, $pkg, $pkg_id, $pkg_name, $pkg_type, $version, $size,
                    $installed_path, datetime(), $with_pkg_id, $profile, $install_patterns
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
            globs,
        })
    }

    pub async fn download_package(&self) -> SoarResult<Option<String>> {
        let package = &self.package;
        let output_path = self.install_dir.join(&package.pkg_name);

        // fallback to download_url for repositories without ghcr
        let (url, output_path) = if let Some(ref ghcr_pkg) = self.package.ghcr_pkg {
            (ghcr_pkg, &self.install_dir)
        } else {
            (&self.package.download_url, &output_path.to_path_buf())
        };

        if self.package.ghcr_pkg.is_some() {
            let mut dl = OciDownload::new(url.as_str())
                .output(output_path.to_string_lossy())
                .parallel(get_config().ghcr_concurrency.unwrap_or(8))
                .overwrite(OverwriteMode::Skip);

            if let Some(ref cb) = self.progress_callback {
                let cb = cb.clone();
                dl = dl.progress(move |p| {
                    cb(p);
                });
            }

            if !self.globs.is_empty() {
                dl = dl.filter(Filter {
                    globs: self.globs.clone(),
                    ..Default::default()
                });
            }

            let mut retries = 0;
            loop {
                if retries > 5 {
                    if let Some(ref callback) = self.progress_callback {
                        callback(Progress::Aborted);
                    }
                    break;
                }
                match dl.clone().execute() {
                    Ok(_) => break,
                    Err(err) => {
                        if matches!(
                            err,
                            DownloadError::HttpError {
                                status: 429,
                                ..
                            } | DownloadError::Network(_)
                        ) {
                            sleep(Duration::from_secs(5));
                            retries += 1;
                            if retries > 1 {
                                if let Some(ref callback) = self.progress_callback {
                                    callback(Progress::Error);
                                }
                            }
                        } else {
                            return Err(err)?;
                        }
                    }
                }
            }

            Ok(None)
        } else {
            let extract_dir = get_extract_dir(&self.install_dir);

            let mut dl = Download::new(url.as_str())
                .output(output_path.to_string_lossy())
                .overwrite(OverwriteMode::Skip)
                .extract(true)
                .extract_to(&extract_dir);

            if let Some(ref cb) = self.progress_callback {
                let cb = cb.clone();
                dl = dl.progress(move |p| {
                    cb(p);
                });
            }

            let file_path = dl.execute()?;

            let checksum = if PathBuf::from(&file_path).exists() {
                Some(calculate_checksum(&file_path)?)
            } else {
                None
            };

            let extract_path = PathBuf::from(&extract_dir);
            if extract_path.exists() {
                fs::remove_file(file_path).ok();

                for entry in fs::read_dir(&extract_path)
                    .with_context(|| format!("reading {} directory", extract_path.display()))?
                {
                    let entry = entry.with_context(|| {
                        format!("reading entry from directory {}", extract_path.display())
                    })?;
                    let from = entry.path();
                    let to = self.install_dir.join(entry.file_name());
                    fs::rename(&from, &to).with_context(|| {
                        format!("renaming {} to {}", from.display(), to.display())
                    })?;
                }

                fs::remove_dir_all(&extract_path).ok();
            }

            Ok(checksum)
        }
    }

    pub async fn record(
        &self,
        unlinked: bool,
        portable: Option<&str>,
        portable_home: Option<&str>,
        portable_config: Option<&str>,
        portable_share: Option<&str>,
        portable_cache: Option<&str>,
    ) -> SoarResult<()> {
        let mut conn = self.db.lock()?;
        let package = &self.package;
        let Package {
            repo_name,
            pkg_name,
            pkg_id,
            version,
            ghcr_size,
            size,
            bsum,
            ..
        } = package;
        let provides = serde_json::to_string(&package.provides).unwrap();
        let size = ghcr_size.unwrap_or(size.unwrap_or(0));

        let with_pkg_id = self.with_pkg_id;
        let tx = conn.transaction()?;

        let record_id: u32 = {
            tx.query_row(
                r#"
                UPDATE packages
                SET
                    version = ?,
                    size = ?,
                    installed_date = datetime(),
                    is_installed = true,
                    provides = ?,
                    with_pkg_id = ?,
                    checksum = ?
                WHERE
                    repo_name = ?
                    AND pkg_name = ?
                    AND pkg_id = ?
                    AND pinned = false
                    AND version = ?
                RETURNING id
                "#,
                params![
                    version,
                    size,
                    provides,
                    with_pkg_id,
                    bsum,
                    repo_name,
                    pkg_name,
                    pkg_id,
                    version,
                ],
                |row| row.get(0),
            )
            .unwrap_or_default()
        };

        if portable.is_some()
            || portable_home.is_some()
            || portable_config.is_some()
            || portable_share.is_some()
            || portable_cache.is_some()
        {
            let base_dir = env::current_dir()
                .map_err(|_| SoarError::Custom("Error retrieving current directory".into()))?;

            let [portable, portable_home, portable_config, portable_share, portable_cache] = [
                portable,
                portable_home,
                portable_config,
                portable_share,
                portable_cache,
            ]
            .map(|opt| {
                opt.map(|p| {
                    if p.is_empty() {
                        String::new()
                    } else {
                        let path = PathBuf::from(&p);
                        let absolute = if path.is_absolute() {
                            path
                        } else {
                            base_dir.join(path)
                        };
                        absolute.to_string_lossy().into_owned()
                    }
                })
            });

            // try to update existing record first
            let mut stmt = prepare_and_bind!(
                tx,
                "UPDATE portable_package
                SET
                    portable_path = $portable,
                    portable_home = $portable_home,
                    portable_config = $portable_config,
                    portable_share = $portable_share,
                    portable_cache = $portable_cache
                WHERE
                    package_id = $record_id
                "
            );
            let updated = stmt.raw_execute()?;

            // if no record were updated, add a new record
            if updated == 0 {
                let mut stmt = prepare_and_bind!(
                    tx,
                    "INSERT INTO portable_package
                (
                    package_id, portable_path, portable_home, portable_config,
                    portable_share, portable_cache
                )
                VALUES
                (
                     $record_id, $portable, $portable_home, $portable_config,
                     $portable_share, $portable_cache
                )
                "
                );
                stmt.raw_execute()?;
            }
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
                        version != $version
                    )"
            );
            stmt.raw_execute()?;
        }

        tx.commit()?;
        drop(conn);

        if !unlinked {
            // FIXME: alternate package could be the same package but different version
            // or different package but same version
            //
            // this makes assumption that the pkg_id and version both are different
            let alternate_packages = PackageQueryBuilder::new(self.db.clone())
                .where_and("pkg_name", FilterCondition::Eq(pkg_name.to_owned()))
                .where_and("pkg_id", FilterCondition::Ne(pkg_id.to_owned()))
                .where_and("version", FilterCondition::Ne(version.to_owned()))
                .load_installed()?
                .items;

            for package in alternate_packages {
                let installed_path = PathBuf::from(&package.installed_path);

                let mut remove_action = |path: &Path| -> FileSystemResult<()> {
                    if let Ok(real_path) = fs::read_link(path) {
                        if real_path.parent() == Some(&installed_path) {
                            safe_remove(path)?;
                        }
                    }
                    Ok(())
                };
                walk_dir(desktop_dir(), &mut remove_action)?;

                let mut remove_action = |path: &Path| -> FileSystemResult<()> {
                    if let Ok(real_path) = fs::read_link(path) {
                        if real_path.parent() == Some(&installed_path) {
                            safe_remove(path)?;
                        }
                    }
                    Ok(())
                };
                walk_dir(icons_dir(), &mut remove_action)?;

                if let Some(provides) = package.provides {
                    for provide in provides {
                        if let Some(ref target) = provide.target {
                            let is_symlink = matches!(
                                provide.strategy,
                                Some(ProvideStrategy::KeepTargetOnly)
                                    | Some(ProvideStrategy::KeepBoth)
                            );
                            if is_symlink {
                                let target_name = get_config().get_bin_path()?.join(target);
                                if target_name.is_symlink() || target_name.is_file() {
                                    std::fs::remove_file(&target_name).with_context(|| {
                                        format!("removing provide {}", target_name.display())
                                    })?;
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
