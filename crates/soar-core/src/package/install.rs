use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    thread::sleep,
    time::Duration,
};

use chrono::Utc;
use serde_json::json;
use soar_config::{
    config::get_config,
    packages::{BinaryMapping, BuildConfig, PackageHooks, SandboxConfig},
};
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
use tracing::{debug, trace, warn};

use crate::{
    constants::INSTALL_MARKER_FILE,
    database::{connection::DieselDatabase, models::Package},
    error::{ErrorContext, SoarError},
    utils::get_extract_dir,
    SoarResult,
};

/// Early validation of relative paths before download.
/// Rejects paths containing `..` or absolute paths.
fn validate_relative_path(relative_path: &str, path_type: &str) -> SoarResult<()> {
    if Path::new(relative_path).is_absolute() {
        return Err(SoarError::Custom(format!(
            "{} '{}' must be a relative path, not absolute",
            path_type, relative_path
        )));
    }

    if relative_path.contains("..") {
        return Err(SoarError::Custom(format!(
            "{} '{}' contains path traversal components",
            path_type, relative_path
        )));
    }

    Ok(())
}

/// Validate that a path is contained within a base directory (post-extraction check).
/// Returns the canonicalized path if valid, or an error if the path escapes the base.
fn validate_path_containment(
    base_dir: &Path,
    relative_path: &str,
    path_type: &str,
) -> SoarResult<PathBuf> {
    let joined_path = base_dir.join(relative_path);

    let canonical_base = base_dir
        .canonicalize()
        .with_context(|| format!("canonicalizing base directory {}", base_dir.display()))?;

    let canonical_path = joined_path.canonicalize().with_context(|| {
        format!(
            "canonicalizing {} path {}",
            path_type,
            joined_path.display()
        )
    })?;

    if !canonical_path.starts_with(&canonical_base) {
        return Err(SoarError::Custom(format!(
            "{} '{}' escapes install directory (path traversal)",
            path_type, relative_path
        )));
    }

    Ok(canonical_path)
}

/// Marker content to verify partial install matches current package
#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct InstallMarker {
    pub pkg_id: String,
    pub version: String,
    pub bsum: Option<String>,
}

impl InstallMarker {
    pub fn read_from_dir(install_dir: &Path) -> Option<Self> {
        let marker_path = install_dir.join(INSTALL_MARKER_FILE);
        let content = fs::read_to_string(&marker_path).ok()?;
        serde_json::from_str(&content).ok()
    }

    pub fn matches_package(&self, package: &Package) -> bool {
        self.pkg_id == package.pkg_id
            && self.version == package.version
            && self.bsum == package.bsum
    }
}

pub struct PackageInstaller {
    package: Package,
    install_dir: PathBuf,
    progress_callback: Option<std::sync::Arc<dyn Fn(Progress) + Send + Sync>>,
    db: DieselDatabase,
    with_pkg_id: bool,
    globs: Vec<String>,
    nested_extract: Option<String>,
    extract_root: Option<String>,
    hooks: Option<PackageHooks>,
    build: Option<BuildConfig>,
    sandbox: Option<SandboxConfig>,
}

#[derive(Clone, Default, Debug)]
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
    pub entrypoint: Option<String>,
    pub binaries: Option<Vec<BinaryMapping>>,
    pub nested_extract: Option<String>,
    pub extract_root: Option<String>,
    pub hooks: Option<PackageHooks>,
    pub build: Option<BuildConfig>,
    pub sandbox: Option<SandboxConfig>,
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
        trace!(
            pkg_name = package.pkg_name,
            pkg_id = package.pkg_id,
            install_dir = %install_dir.display(),
            "creating package installer"
        );
        let profile = get_config().default_profile.clone();

        // Early validation of extract_root and nested_extract paths
        if let Some(ref extract_root) = target.extract_root {
            validate_relative_path(extract_root, "extract_root")?;
        }
        if let Some(ref nested_extract) = target.nested_extract {
            validate_relative_path(nested_extract, "nested_extract")?;
        }

        // Check if there's a pending install for this exact version we can resume
        let has_pending = db.with_conn(|conn| {
            CoreRepository::has_pending_install(
                conn,
                &package.pkg_id,
                &package.pkg_name,
                &package.repo_name,
                &package.version,
            )
        })?;

        trace!(
            pkg_id = package.pkg_id,
            pkg_name = package.pkg_name,
            repo_name = package.repo_name,
            version = package.version,
            has_pending = has_pending,
            "checking for pending install"
        );

        let needs_new_record = if has_pending {
            trace!("resuming existing pending install");
            false
        } else {
            match &target.existing_install {
                None => true,
                Some(existing) => existing.version != package.version || existing.is_installed,
            }
        };

        if needs_new_record {
            trace!(
                "inserting new package record for version {}",
                package.version
            );
            let repo_name = &package.repo_name;
            let pkg_id = &package.pkg_id;
            let pkg_name = &package.pkg_name;
            let pkg_type = package.pkg_type.as_deref();
            let version = &package.version;
            let size = package.ghcr_size.unwrap_or(package.size.unwrap_or(0)) as i64;
            let installed_path = install_dir.to_string_lossy();
            let installed_date = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

            // Clean up any orphaned pending installs (different versions) before creating new record
            let orphaned_paths = db.with_conn(|conn| {
                CoreRepository::delete_pending_installs(conn, pkg_id, pkg_name, repo_name)
            })?;
            for path in orphaned_paths {
                let path = std::path::Path::new(&path);
                if path.exists() {
                    fs::remove_dir_all(path).ok();
                }
            }

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
            nested_extract: target.nested_extract.clone(),
            extract_root: target.extract_root.clone(),
            hooks: target.hooks.clone(),
            build: target.build.clone(),
            sandbox: target.sandbox.clone(),
        })
    }

    /// Run a hook command with environment variables set.
    fn run_hook(&self, hook_name: &str, command: &str) -> SoarResult<()> {
        use super::hooks::{run_hook, HookEnv};

        let env = HookEnv {
            install_dir: &self.install_dir,
            pkg_name: &self.package.pkg_name,
            pkg_id: &self.package.pkg_id,
            pkg_version: &self.package.version,
        };

        run_hook(hook_name, command, &env, self.sandbox.as_ref())
    }

    /// Run post_download hook if configured.
    pub fn run_post_download_hook(&self) -> SoarResult<()> {
        if let Some(ref hooks) = self.hooks {
            if let Some(ref cmd) = hooks.post_download {
                self.run_hook("post_download", cmd)?;
            }
        }
        Ok(())
    }

    /// Run post_extract hook if configured.
    pub fn run_post_extract_hook(&self) -> SoarResult<()> {
        if let Some(ref hooks) = self.hooks {
            if let Some(ref cmd) = hooks.post_extract {
                self.run_hook("post_extract", cmd)?;
            }
        }
        Ok(())
    }

    /// Run post_install hook if configured.
    pub fn run_post_install_hook(&self) -> SoarResult<()> {
        if let Some(ref hooks) = self.hooks {
            if let Some(ref cmd) = hooks.post_install {
                self.run_hook("post_install", cmd)?;
            }
        }
        Ok(())
    }

    /// Check if build dependencies are available.
    fn check_build_dependencies(&self, deps: &[String]) -> SoarResult<()> {
        for dep in deps {
            let result = Command::new("which").arg(dep).output();

            match result {
                Ok(output) if !output.status.success() => {
                    warn!("Build dependency '{}' not found in PATH", dep);
                }
                Err(_) => {
                    warn!("Could not check for build dependency '{}'", dep);
                }
                _ => {
                    trace!("Build dependency '{}' found", dep);
                }
            }
        }
        Ok(())
    }

    /// Run build commands if configured.
    pub fn run_build(&self) -> SoarResult<()> {
        use crate::sandbox;

        let build_config = match &self.build {
            Some(config) if !config.commands.is_empty() => config,
            _ => return Ok(()),
        };

        debug!(
            "building package {} with {} commands",
            self.package.pkg_name,
            build_config.commands.len()
        );

        if !build_config.dependencies.is_empty() {
            self.check_build_dependencies(&build_config.dependencies)?;
        }

        let bin_dir = get_config().get_bin_path()?;
        let nproc = std::thread::available_parallelism()
            .map(|p| p.get().to_string())
            .unwrap_or_else(|_| "1".to_string());

        let use_sandbox = sandbox::is_landlock_supported();

        if use_sandbox {
            debug!("running build with Landlock sandbox");
        } else {
            if self.sandbox.as_ref().is_some_and(|s| s.require) {
                return Err(SoarError::Custom(
                    "Build requires sandbox but Landlock is not available on this system. \
                     Either upgrade to Linux 5.13+ or set sandbox.require = false."
                        .into(),
                ));
            }
            warn!(
                "Landlock not supported, running build without sandbox ({} commands)",
                build_config.commands.len()
            );
        }

        for (i, cmd) in build_config.commands.iter().enumerate() {
            debug!(
                "running build command {}/{}: {}",
                i + 1,
                build_config.commands.len(),
                cmd
            );

            let status = if use_sandbox {
                let env_vars: Vec<(&str, String)> = vec![
                    (
                        "INSTALL_DIR",
                        self.install_dir.to_string_lossy().to_string(),
                    ),
                    ("BIN_DIR", bin_dir.to_string_lossy().to_string()),
                    ("PKG_NAME", self.package.pkg_name.clone()),
                    ("PKG_ID", self.package.pkg_id.clone()),
                    ("PKG_VERSION", self.package.version.clone()),
                    ("NPROC", nproc.clone()),
                ];

                let mut sandbox_cmd = sandbox::SandboxedCommand::new(cmd)
                    .working_dir(&self.install_dir)
                    .read_path(&bin_dir)
                    .envs(env_vars);

                if let Some(s) = &self.sandbox {
                    let config = sandbox::SandboxConfig::new().with_network(if s.network {
                        sandbox::NetworkConfig::allow_all()
                    } else {
                        sandbox::NetworkConfig::default()
                    });
                    sandbox_cmd = sandbox_cmd.config(config);
                    for path in &s.fs_read {
                        sandbox_cmd = sandbox_cmd.read_path(path);
                    }
                    for path in &s.fs_write {
                        sandbox_cmd = sandbox_cmd.write_path(path);
                    }
                }
                sandbox_cmd.run()?
            } else {
                Command::new("sh")
                    .arg("-c")
                    .arg(cmd)
                    .env("INSTALL_DIR", &self.install_dir)
                    .env("BIN_DIR", &bin_dir)
                    .env("PKG_NAME", &self.package.pkg_name)
                    .env("PKG_ID", &self.package.pkg_id)
                    .env("PKG_VERSION", &self.package.version)
                    .env("NPROC", &nproc)
                    .current_dir(&self.install_dir)
                    .status()
                    .with_context(|| format!("executing build command {}", i + 1))?
            };

            if !status.success() {
                return Err(SoarError::Custom(format!(
                    "Build command {} failed with exit code: {}",
                    i + 1,
                    status.code().unwrap_or(-1)
                )));
            }
        }

        debug!("build completed successfully");
        Ok(())
    }

    fn write_marker(&self) -> SoarResult<()> {
        fs::create_dir_all(&self.install_dir).with_context(|| {
            format!("creating install directory {}", self.install_dir.display())
        })?;

        let marker = InstallMarker {
            pkg_id: self.package.pkg_id.clone(),
            version: self.package.version.clone(),
            bsum: self.package.bsum.clone(),
        };

        let marker_path = self.install_dir.join(INSTALL_MARKER_FILE);
        let mut file = fs::File::create(&marker_path)
            .with_context(|| format!("creating marker file {}", marker_path.display()))?;
        let content = serde_json::to_string(&marker)
            .map_err(|e| SoarError::Custom(format!("Failed to serialize marker: {e}")))?;
        file.write_all(content.as_bytes())
            .with_context(|| format!("writing marker file {}", marker_path.display()))?;

        Ok(())
    }

    fn remove_marker(&self) -> SoarResult<()> {
        let marker_path = self.install_dir.join(INSTALL_MARKER_FILE);
        if marker_path.exists() {
            fs::remove_file(&marker_path)
                .with_context(|| format!("removing marker file {}", marker_path.display()))?;
        }
        Ok(())
    }

    pub async fn download_package(&self) -> SoarResult<Option<String>> {
        debug!(
            pkg_name = self.package.pkg_name,
            pkg_id = self.package.pkg_id,
            "starting package download"
        );
        self.write_marker()?;

        let package = &self.package;
        let output_path = self.install_dir.join(&package.pkg_name);

        // fallback to download_url for repositories without ghcr
        let (url, output_path) = if let Some(ref ghcr_pkg) = self.package.ghcr_pkg {
            debug!("source: {} (OCI)", ghcr_pkg);
            (ghcr_pkg, &self.install_dir)
        } else {
            debug!("source: {}", self.package.download_url);
            (&self.package.download_url, &output_path.to_path_buf())
        };

        if self.package.ghcr_pkg.is_some() {
            trace!(url = url.as_str(), "using OCI/GHCR download");
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
                    Ok(_) => {
                        debug!("OCI download completed successfully");
                        break;
                    }
                    Err(err) => {
                        if matches!(
                            err,
                            DownloadError::HttpError {
                                status: 429,
                                ..
                            } | DownloadError::Network(_)
                        ) {
                            warn!(retry = retries, "download failed, retrying after delay");
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

            // Run post_download hook for OCI packages
            // For OCI packages, content is directly placed, so post_extract also applies
            self.run_post_download_hook()?;
            self.run_post_extract_hook()?;
            self.run_build()?;

            Ok(None)
        } else {
            trace!(url = url.as_str(), "using direct download");
            let extract_dir = get_extract_dir(&self.install_dir);

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

            self.run_post_download_hook()?;

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

            // Handle extract_root: move contents from subdirectory to install root
            if let Some(ref root_dir) = self.extract_root {
                let root_path =
                    validate_path_containment(&self.install_dir, root_dir, "extract_root")?;

                if root_path.is_dir() {
                    debug!(
                        "applying extract_root: moving contents from {} to {}",
                        root_path.display(),
                        self.install_dir.display()
                    );
                    // Move all contents from root_path to install_dir
                    for entry in fs::read_dir(&root_path).with_context(|| {
                        format!("reading extract_root directory {}", root_path.display())
                    })? {
                        let entry = entry.with_context(|| {
                            format!("reading entry from directory {}", root_path.display())
                        })?;
                        let from = entry.path();
                        let to = self.install_dir.join(entry.file_name());
                        if to.exists() {
                            if to.is_dir() {
                                fs::remove_dir_all(&to).ok();
                            } else {
                                fs::remove_file(&to).ok();
                            }
                        }
                        fs::rename(&from, &to).with_context(|| {
                            format!("moving {} to {}", from.display(), to.display())
                        })?;
                    }
                    fs::remove_dir_all(&root_path).ok();
                } else {
                    warn!("extract_root '{}' not found in package", root_dir);
                }
            }

            // Handle nested_extract: extract an archive within the package
            if let Some(ref nested_archive) = self.nested_extract {
                let archive_path =
                    validate_path_containment(&self.install_dir, nested_archive, "nested_extract")?;

                if archive_path.is_file() {
                    debug!("extracting nested archive: {}", archive_path.display());
                    let nested_extract_dir = get_extract_dir(&self.install_dir);

                    compak::extract_archive(&archive_path, &nested_extract_dir).map_err(|e| {
                        SoarError::Custom(format!(
                            "Failed to extract nested archive {}: {}",
                            archive_path.display(),
                            e
                        ))
                    })?;

                    fs::remove_file(&archive_path).ok();

                    // Move extracted contents to install_dir
                    let nested_extract_path = PathBuf::from(&nested_extract_dir);
                    if nested_extract_path.exists() {
                        for entry in fs::read_dir(&nested_extract_path).with_context(|| {
                            format!(
                                "reading nested extract directory {}",
                                nested_extract_path.display()
                            )
                        })? {
                            let entry = entry.with_context(|| {
                                format!(
                                    "reading entry from directory {}",
                                    nested_extract_path.display()
                                )
                            })?;
                            let from = entry.path();
                            let to = self.install_dir.join(entry.file_name());
                            if to.exists() {
                                if to.is_dir() {
                                    fs::remove_dir_all(&to).ok();
                                } else {
                                    fs::remove_file(&to).ok();
                                }
                            }
                            fs::rename(&from, &to).with_context(|| {
                                format!("moving {} to {}", from.display(), to.display())
                            })?;
                        }
                        fs::remove_dir_all(&nested_extract_path).ok();
                    }
                } else {
                    warn!(
                        "nested_extract archive '{}' not found in package",
                        nested_archive
                    );
                }
            }

            self.run_post_extract_hook()?;
            self.run_build()?;

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
        debug!(
            pkg_name = self.package.pkg_name,
            pkg_id = self.package.pkg_id,
            unlinked = unlinked,
            "recording installation"
        );
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

        let installed_path = self.install_dir.to_string_lossy();
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
                &installed_path,
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

        self.remove_marker()?;

        Ok(())
    }
}
