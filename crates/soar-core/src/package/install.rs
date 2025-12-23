use std::{
    env, fs,
    path::{Path, PathBuf},
    thread::sleep,
    time::Duration,
};

use chrono::Utc;
use serde_json::json;
use soar_config::config::get_config;
use soar_db::{
    models::types::ProvideStrategy,
    repository::core::{CoreRepository, InstalledPackageWithPortable, NewInstalledPackage},
};
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
    database::{connection::DieselDatabase, models::Package},
    error::{ErrorContext, SoarError},
    utils::get_extract_dir,
    SoarResult,
};

pub struct PackageInstaller {
    package: Package,
    install_dir: PathBuf,
    progress_callback: Option<std::sync::Arc<dyn Fn(Progress) + Send + Sync>>,
    db: DieselDatabase,
    with_pkg_id: bool,
    globs: Vec<String>,
}

#[derive(Clone, Default)]
pub struct InstallTarget {
    pub package: Package,
    pub existing_install: Option<crate::database::models::InstalledPackage>,
    pub with_pkg_id: bool,
    pub pinned: bool,
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
        progress_callback: Option<std::sync::Arc<dyn Fn(Progress) + Send + Sync>>,
        db: DieselDatabase,
        with_pkg_id: bool,
        globs: Vec<String>,
    ) -> SoarResult<Self> {
        let install_dir = install_dir.as_ref().to_path_buf();
        let package = &target.package;
        let profile = get_config().default_profile.clone();

        if target.existing_install.is_none() {
            let repo_name = &package.repo_name;
            let pkg_id = &package.pkg_id;
            let pkg_name = &package.pkg_name;
            let pkg_type = package.pkg_type.as_deref();
            let version = &package.version;
            let size = package.ghcr_size.unwrap_or(package.size.unwrap_or(0)) as i64;
            let installed_path = install_dir.to_string_lossy();
            let installed_date = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

            let new_package = NewInstalledPackage {
                repo_name,
                pkg_id,
                pkg_name,
                pkg_type,
                version,
                size,
                checksum: None,
                installed_path: &installed_path,
                installed_date: &installed_date,
                profile: &profile,
                pinned: target.pinned,
                is_installed: false,
                with_pkg_id,
                detached: false,
                unlinked: false,
                provides: None,
                install_patterns: Some(json!(globs)),
            };

            db.with_conn(|conn| CoreRepository::insert(conn, &new_package))?;
        }

        Ok(Self {
            package: package.clone(),
            install_dir,
            progress_callback,
            db,
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
            let mut last_error: Option<DownloadError> = None;
            loop {
                if retries > 5 {
                    if let Some(ref callback) = self.progress_callback {
                        callback(Progress::Aborted);
                    }
                    // Return error after max retries
                    return Err(last_error.unwrap_or_else(|| {
                        DownloadError::Multiple {
                            errors: vec!["Download failed after 5 retries".into()],
                        }
                    }))?;
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
                            last_error = Some(err);
                        } else {
                            return Err(err)?;
                        }
                    }
                }
            }

            Ok(None)
        } else {
            let extract_dir = get_extract_dir(&self.install_dir);

            // Only extract if it's an archive type
            let should_extract = self
                .package
                .pkg_type
                .as_deref()
                .is_some_and(|t| t == "archive");

            let mut dl = Download::new(url.as_str())
                .output(output_path.to_string_lossy())
                .overwrite(OverwriteMode::Skip)
                .extract(should_extract)
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
        let package = &self.package;
        let repo_name = &package.repo_name;
        let pkg_name = &package.pkg_name;
        let pkg_id = &package.pkg_id;
        let version = &package.version;
        let size = package.ghcr_size.unwrap_or(package.size.unwrap_or(0)) as i64;
        let checksum = package.bsum.as_deref();
        let provides = package.provides.clone();

        let with_pkg_id = self.with_pkg_id;
        let installed_date = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let record_id: Option<i32> = self.db.with_conn(|conn| {
            CoreRepository::record_installation(
                conn,
                repo_name,
                pkg_name,
                pkg_id,
                version,
                size,
                provides,
                with_pkg_id,
                checksum,
                &installed_date,
            )
        })?;

        let record_id = record_id.ok_or_else(|| {
            SoarError::Custom(format!(
                "Failed to record installation for {}#{}: package not found in database",
                pkg_name, pkg_id
            ))
        })?;

        if portable.is_some()
            || portable_home.is_some()
            || portable_config.is_some()
            || portable_share.is_some()
            || portable_cache.is_some()
        {
            let base_dir = env::current_dir()
                .map_err(|_| SoarError::Custom("Error retrieving current directory".into()))?;

            let resolve_path = |opt: Option<&str>| -> Option<String> {
                opt.map(|p| {
                    if p.is_empty() {
                        String::new()
                    } else {
                        let path = PathBuf::from(p);
                        let absolute = if path.is_absolute() {
                            path
                        } else {
                            base_dir.join(path)
                        };
                        absolute.to_string_lossy().into_owned()
                    }
                })
            };

            let portable_path = resolve_path(portable);
            let portable_home = resolve_path(portable_home);
            let portable_config = resolve_path(portable_config);
            let portable_share = resolve_path(portable_share);
            let portable_cache = resolve_path(portable_cache);

            self.db.with_conn(|conn| {
                CoreRepository::upsert_portable(
                    conn,
                    record_id,
                    portable_path.as_deref(),
                    portable_home.as_deref(),
                    portable_config.as_deref(),
                    portable_share.as_deref(),
                    portable_cache.as_deref(),
                )
            })?;
        }

        if !unlinked {
            self.db
                .with_conn(|conn| CoreRepository::unlink_others(conn, pkg_name, pkg_id, version))?;

            let alternate_packages: Vec<InstalledPackageWithPortable> =
                self.db.with_conn(|conn| {
                    CoreRepository::find_alternates(conn, pkg_name, pkg_id, version)
                })?;

            for alt_pkg in alternate_packages {
                let installed_path = PathBuf::from(&alt_pkg.installed_path);

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

                if let Some(ref provides) = alt_pkg.provides {
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
