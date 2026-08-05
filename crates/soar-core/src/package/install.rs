use std::{
    collections::HashMap,
    env, fs,
    io::{Read, Write},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
    thread::sleep,
    time::Duration,
};

use chrono::Utc;
use serde_json::json;
use soar_config::{
    config::Config,
    packages::{BinaryMapping, BuildConfig, PackageHooks, SandboxConfig},
};
use soar_db::{
    models::types::PackageFile,
    repository::core::{CoreRepository, InstalledPackageWithPortable, NewInstalledPackage},
};
use soar_dl::{
    download::Download,
    error::DownloadError,
    filter::Filter,
    oci::OciDownload,
    types::{OverwriteMode, Progress},
};
use soar_events::{BuildStage, EventSinkHandle, InstallStage, OperationId, SoarEvent};
use soar_utils::{
    error::FileSystemResult,
    fs::{safe_remove, walk_dir},
    hash::calculate_checksum,
    path::is_safe_component,
};
use tracing::{debug, trace, warn};

use crate::{
    constants::INSTALL_MARKER_FILE,
    database::{connection::DieselDatabase, models::Package},
    error::{ErrorContext, SoarError},
    package::{
        local::local_path_from_url, remove::remove_provide_symlinks, update_info::UpdateInfo,
    },
    utils::get_extract_dir,
    SoarResult,
};

/// Fetch the side files an artifact does not carry itself.
///
/// One that published a hash is verified against it, on the same footing as
/// the artifact. A licence publishes none, because it is served from a branch
/// and is documentation rather than something that runs: pinning it would turn
/// an upstream copyright-year edit into a failed download.
async fn install_extras(package: &Package, install_dir: &Path) -> SoarResult<()> {
    let Some(extras) = &package.extra else {
        return Ok(());
    };
    for e in extras {
        if !is_safe_component(&e.to) {
            warn!(to = e.to, "skipping side file with unsafe name");
            continue;
        }
        let dest = install_dir.join(&e.to);
        if dest.exists() {
            continue;
        }
        let mut dl = Download::new(&e.url)
            .output(dest.to_string_lossy())
            .overwrite(OverwriteMode::Skip);
        if let Some(sum) = e.blake3.as_ref() {
            dl = dl.checksum(sum);
        }
        match dl.execute() {
            Ok(_) => {
                // A side file is usually a licence, but it can be a binary an
                // upstream ships separately, and that has to be runnable to be
                // worth linking.
                if is_elf(&dest) {
                    fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755)).ok();
                }
                debug!(file = %dest.display(), "installed side file")
            }
            Err(err) => {
                // A missing licence should not abandon a working install, but
                // it must not pass unnoticed either.
                fs::remove_file(&dest).ok();
                warn!(url = e.url, error = %err, "could not install side file");
            }
        }
    }
    Ok(())
}

/// Lay the package out as its recipe describes: each listed file at its own
/// path, aliases beside it, and nothing else kept.
///
/// Built in a staging directory and swapped in at the end. Resolving a source
/// can fail, and pruning first would leave a package with its binary deleted
/// and nothing to put back.
pub fn apply_file_layout(
    files: &[PackageFile],
    install_dir: &Path,
    artifact: &Path,
) -> SoarResult<()> {
    let staging = install_dir.join(".soar-layout");
    fs::remove_dir_all(&staging).ok();
    fs::create_dir_all(&staging)
        .with_context(|| format!("creating staging directory {}", staging.display()))?;

    // An archive may ship its directories read-only, and moving a file out of
    // one needs write permission on the directory itself.
    crate::package::remove::make_tree_writable(install_dir);

    // What each file was moved from, so a failure part-way can put it back.
    let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
    let placed = match place_files(files, install_dir, artifact, &staging, &mut moved) {
        Ok(placed) => placed,
        Err(err) => {
            // Leave the package as it was found rather than half emptied: the
            // caller may retry, and the sources are only in the staging
            // directory this call is about to remove.
            for (source, dest) in moved {
                fs::rename(&dest, &source).ok();
            }
            fs::remove_dir_all(&staging).ok();
            return Err(err);
        }
    };

    // Nothing resolved means the recipe and the artifact disagree, which most
    // often means the download never extracted. Keeping what is there would
    // record a package with no commands in it and still report success, so the
    // install fails here instead.
    if placed == 0 {
        fs::remove_dir_all(&staging).ok();
        return Err(SoarError::Custom(format!(
            "none of the {} files listed by {} were found in the artifact",
            files.len(),
            install_dir.display()
        )));
    }

    for entry in fs::read_dir(install_dir)
        .with_context(|| format!("reading {}", install_dir.display()))?
        .flatten()
    {
        let path = entry.path();
        // The install marker is soar's own bookkeeping, not package content:
        // removing it here loses the record an interrupted install resumes from.
        if path == staging || entry.file_name() == INSTALL_MARKER_FILE {
            continue;
        }
        if path.is_dir() && !path.is_symlink() {
            fs::remove_dir_all(&path).ok();
        } else {
            fs::remove_file(&path).ok();
        }
    }
    for entry in fs::read_dir(&staging)
        .with_context(|| format!("reading {}", staging.display()))?
        .flatten()
    {
        let to = install_dir.join(entry.file_name());
        fs::rename(entry.path(), &to)
            .with_context(|| format!("moving {} into place", to.display()))?;
    }
    fs::remove_dir_all(&staging).ok();
    Ok(())
}

/// Move each listed file into the staging directory and record where it came
/// from. Returns how many were placed.
fn place_files(
    files: &[PackageFile],
    install_dir: &Path,
    artifact: &Path,
    staging: &Path,
    moved: &mut Vec<(PathBuf, PathBuf)>,
) -> SoarResult<usize> {
    let present = walk_dir_files(install_dir, staging);
    // Where a source ended up, for the second entry that names the same file.
    let mut taken: HashMap<PathBuf, PathBuf> = HashMap::new();
    let mut placed = 0usize;
    for file in files {
        if !is_safe_relative(&file.to) {
            warn!(to = file.to, "skipping file with an unsafe target");
            continue;
        }
        let Some(source) = resolve_source(&file.source, install_dir, &present, artifact) else {
            warn!(
                source = file.source,
                to = file.to,
                "file not found in the artifact"
            );
            continue;
        };
        let dest = staging.join(&file.to);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        // A file listed twice under different names was consumed by the first
        // move, so the second is a copy of where it landed.
        match taken.get(&source) {
            Some(already) => {
                fs::copy(already, &dest).with_context(|| format!("copying to {}", file.to))?;
            }
            None => {
                fs::rename(&source, &dest).with_context(|| format!("placing {}", file.to))?;
                moved.push((source.clone(), dest.clone()));
                taken.insert(source, dest.clone());
            }
        }
        placed += 1;

        for alias in &file.alias {
            if !is_safe_relative(alias) {
                warn!(alias, "skipping alias with an unsafe target");
                continue;
            }
            let link = staging.join(alias);
            if let Some(parent) = link.parent() {
                fs::create_dir_all(parent).ok();
            }
            // relative, so the package directory stays movable
            if let Some(target) = relative_to(alias, &file.to) {
                std::os::unix::fs::symlink(&target, &link).ok();
            }
        }
    }
    Ok(placed)
}

/// The path `target` has when read from the directory holding `link`.
///
/// Nearly every alias is a sibling of what it points at, which is just the
/// file's own name, but nothing in the format requires that.
fn relative_to(link: &str, target: &str) -> Option<PathBuf> {
    let link_dir: Vec<&str> = link.split('/').collect();
    let link_dir = &link_dir[..link_dir.len().saturating_sub(1)];
    let target_parts: Vec<&str> = target.split('/').collect();
    let shared = link_dir
        .iter()
        .zip(&target_parts)
        .take_while(|(a, b)| a == b)
        .count();
    let mut out = PathBuf::new();
    for _ in shared..link_dir.len() {
        out.push("..");
    }
    for part in &target_parts[shared..] {
        out.push(part);
    }
    (!out.as_os_str().is_empty()).then_some(out)
}

/// Whether a path stays inside the directory it is joined to.
fn is_safe_relative(path: &str) -> bool {
    !path.is_empty()
        && !Path::new(path).is_absolute()
        && Path::new(path)
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
}

/// Every regular file under `dir`, skipping the staging directory.
fn walk_dir_files(dir: &Path, skip: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path == skip || path.is_symlink() {
            continue;
        }
        if path.is_dir() {
            out.extend(walk_dir_files(&path, skip));
        } else {
            out.push(path);
        }
    }
    out
}

/// Find a listed source among the extracted files.
///
/// An empty source means the artifact is itself the file, which is the case for
/// a bare binary. Otherwise the recorded path is tried as written, then without
/// its leading component, since an archive with a single top-level directory
/// has it promoted away, and finally by file name alone.
fn resolve_source(
    source: &str,
    install_dir: &Path,
    present: &[PathBuf],
    artifact: &Path,
) -> Option<PathBuf> {
    // An empty source means the download itself, which is how a bare binary or
    // an AppImage says "the artifact is the file".
    if source.is_empty() {
        return artifact.exists().then(|| artifact.to_path_buf());
    }
    let rel_of = |p: &PathBuf| {
        p.strip_prefix(install_dir)
            .unwrap_or(p)
            .to_string_lossy()
            .to_string()
    };
    // As written first, then without the leading component, since an archive
    // with a single top-level directory has it promoted away.
    let mut patterns = vec![source.to_string()];
    if let Some((_, rest)) = source.split_once('/') {
        patterns.push(rest.to_string());
    }
    for pattern in patterns {
        if let Some(hit) = present.iter().find(|p| rel_of(p) == pattern) {
            return Some(hit.clone());
        }
    }
    // Last resort, the file name alone: an archive that was extracted but not
    // promoted still has everything under the extraction directory.
    let name = source.rsplit('/').next().unwrap_or(source);
    present
        .iter()
        .find(|p| p.file_name().is_some_and(|n| n == name))
        .cloned()
}

/// Give every ELF under `dir` the executable bit.
///
/// Archive members carry whatever permissions the upstream tarball recorded,
/// and some ship binaries as 0644. The downloaded file is chmodded on the way
/// in, but files that only appear after extraction were being left
/// unexecutable.
fn mark_elfs_executable(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_symlink() {
            continue;
        }
        if path.is_dir() {
            mark_elfs_executable(&path);
        } else if is_elf(&path) {
            fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).ok();
        }
    }
}

/// Returns `true` if the file at `path` starts with the ELF magic bytes.
///
/// AppImages and plain binaries are ELF and need the executable bit; archives
/// are not and are extracted instead.
fn is_elf(path: &Path) -> bool {
    let mut magic = [0u8; 4];
    fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut magic))
        .is_ok()
        && magic == *b"\x7fELF"
}

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

use crate::utils::substitute_placeholders;

/// Marker content to verify partial install matches current package
#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct InstallMarker {
    #[serde(default)]
    pub pkg_id: Option<String>,
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
    config: Config,
    globs: Vec<String>,
    nested_extract: Option<String>,
    extract_root: Option<String>,
    hooks: Option<PackageHooks>,
    build: Option<BuildConfig>,
    sandbox: Option<SandboxConfig>,
    arch_map: Option<std::collections::HashMap<String, String>>,
    zsync: Option<ZsyncSeed>,
    events: EventSinkHandle,
    op_id: OperationId,
}

/// A zsync feed and the installed copy to rebuild the new artifact from.
///
/// An AppImage release changes a fraction of a file measured in tens of
/// megabytes, so the blocks the old copy already holds are worth reusing.
#[derive(Clone, Debug)]
pub struct ZsyncSeed {
    /// URL of the zsync control file describing the new artifact.
    pub url: String,
    /// The installed artifact to take unchanged blocks from.
    pub seed: PathBuf,
}

#[derive(Clone, Default, Debug)]
pub struct InstallTarget {
    pub package: Package,
    pub existing_install: Option<crate::database::models::InstalledPackage>,
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
    pub arch_map: Option<std::collections::HashMap<String, String>>,
    /// Set when the new artifact can be rebuilt from the installed one.
    pub zsync: Option<ZsyncSeed>,
}

impl PackageInstaller {
    #[allow(clippy::too_many_arguments)]
    pub async fn new<P: AsRef<Path>>(
        target: &InstallTarget,
        install_dir: P,
        progress_callback: Option<std::sync::Arc<dyn Fn(Progress) + Send + Sync>>,
        db: DieselDatabase,
        globs: Vec<String>,
        config: Config,
        events: EventSinkHandle,
        op_id: OperationId,
    ) -> SoarResult<Self> {
        let install_dir = install_dir.as_ref().to_path_buf();
        let package = &target.package;
        trace!(
            pkg_name = package.pkg_name,
            pkg_id = package.pkg_id,
            install_dir = %install_dir.display(),
            "creating package installer"
        );
        let profile = config.default_profile.clone();

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
                package.pkg_id.as_deref(),
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
            let pkg_id = package.pkg_id.as_deref();
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
                pkg_family: package.pkg_family.as_deref(),
                pkg_type,
                version,
                size,
                checksum: None,
                installed_path: &installed_path,
                installed_date: &installed_date,
                profile: &profile,
                pinned: target.pinned,
                is_installed: false,
                detached: false,
                unlinked: false,
                provides: None,
                install_patterns: Some(json!(globs)),
                download_url: None,
                update_info: None,
            };

            db.with_conn(|conn| CoreRepository::insert(conn, &new_package))?;
        }

        Ok(Self {
            package: package.clone(),
            install_dir,
            progress_callback,
            db,
            config,
            globs,
            nested_extract: target.nested_extract.clone(),
            extract_root: target.extract_root.clone(),
            hooks: target.hooks.clone(),
            build: target.build.clone(),
            sandbox: target.sandbox.clone(),
            arch_map: target.arch_map.clone(),
            zsync: target.zsync.clone(),
            events,
            op_id,
        })
    }

    /// Run a hook command with environment variables set.
    fn run_hook(&self, hook_name: &str, command: &str) -> SoarResult<()> {
        use super::hooks::{run_hook, HookEnv};

        let env = HookEnv {
            pkg_id: self.package.pkg_id.as_deref().unwrap_or_default(),
            install_dir: &self.install_dir,
            pkg_name: &self.package.pkg_name,
            pkg_version: &self.package.version,
        };

        self.events.emit(SoarEvent::Installing {
            op_id: self.op_id,
            pkg_name: self.package.pkg_name.clone(),
            stage: InstallStage::RunningHook(hook_name.to_string()),
        });

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

        let bin_dir = self.config.get_bin_path()?;
        let nproc = std::thread::available_parallelism()
            .map(|p| p.get().to_string())
            .unwrap_or_else(|_| "1".to_string());

        let sandbox_enabled = self.sandbox.as_ref().is_none_or(|s| s.is_enabled());
        let use_sandbox = sandbox_enabled && sandbox::is_landlock_supported();

        if use_sandbox {
            debug!("running build with Landlock sandbox");
        } else if !sandbox_enabled {
            debug!(
                "sandbox explicitly disabled, running build without sandbox ({} commands)",
                build_config.commands.len()
            );
        } else {
            if self.sandbox.as_ref().is_some_and(|s| s.is_required()) {
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

        let total_commands = build_config.commands.len();

        if use_sandbox {
            self.events.emit(SoarEvent::Building {
                op_id: self.op_id,
                pkg_name: self.package.pkg_name.clone(),
                stage: BuildStage::Sandboxing,
            });
        }

        for (i, cmd) in build_config.commands.iter().enumerate() {
            debug!(
                "running build command {}/{}: {}",
                i + 1,
                total_commands,
                cmd
            );

            self.events.emit(SoarEvent::Building {
                op_id: self.op_id,
                pkg_name: self.package.pkg_name.clone(),
                stage: BuildStage::Running {
                    command_index: i,
                    total_commands,
                },
            });

            let status = if use_sandbox {
                let env_vars: Vec<(&str, String)> = vec![
                    (
                        "INSTALL_DIR",
                        self.install_dir.to_string_lossy().to_string(),
                    ),
                    ("BIN_DIR", bin_dir.to_string_lossy().to_string()),
                    ("PKG_NAME", self.package.pkg_name.clone()),
                    ("PKG_ID", self.package.pkg_id.clone().unwrap_or_default()),
                    ("PKG_VERSION", self.package.version.clone()),
                    ("NPROC", nproc.clone()),
                ];

                let mut sandbox_cmd = sandbox::SandboxedCommand::new(cmd)
                    .working_dir(&self.install_dir)
                    .read_path(&bin_dir)
                    .envs(env_vars);

                if let Some(s) = &self.sandbox {
                    let config =
                        sandbox::SandboxConfig::new().with_network(if s.allows_network() {
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
                    .env("PKG_ID", self.package.pkg_id.as_deref().unwrap_or_default())
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

            self.events.emit(SoarEvent::Building {
                op_id: self.op_id,
                pkg_name: self.package.pkg_name.clone(),
                stage: BuildStage::CommandComplete {
                    command_index: i,
                },
            });
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

    /// Install a package from a local file by copying it into the install
    /// directory (and extracting it when it is an archive), mirroring the
    /// relevant post-download steps of [`Download::execute`].
    fn copy_local_source(
        &self,
        src: &Path,
        dest: &Path,
        extract: bool,
        extract_dir: &Path,
    ) -> SoarResult<PathBuf> {
        if !src.is_file() {
            return Err(SoarError::Custom(format!(
                "Local source is not a file: {}",
                src.display()
            )));
        }

        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating directory {}", parent.display()))?;
        }

        fs::copy(src, dest)
            .with_context(|| format!("copying {} to {}", src.display(), dest.display()))?;

        // Honor checksum pinning the same way the direct-download path does.
        if let Some(ref bsum) = self.package.bsum {
            let actual = calculate_checksum(dest)?;
            if &actual != bsum {
                fs::remove_file(dest).ok();
                return Err(SoarError::Custom(format!(
                    "Checksum mismatch for {}: expected {}, got {}",
                    src.display(),
                    bsum,
                    actual
                )));
            }
        }

        // ELF binaries (including AppImages) need the executable bit; archives
        // are extracted below instead of being run directly.
        if is_elf(dest) {
            fs::set_permissions(dest, std::fs::Permissions::from_mode(0o755))
                .with_context(|| format!("setting permissions on {}", dest.display()))?;
        }

        // Extraction is offered for every install, so what the file actually is
        // decides: a local AppImage or bare binary is not an archive and is
        // left alone rather than failing.
        if extract && compak::detect_from_file(dest).is_ok() {
            debug!(archive = %dest.display(), dest = %extract_dir.display(), "extracting local archive");
            compak::extract_archive(dest, extract_dir).map_err(|e| {
                SoarError::Custom(format!(
                    "Failed to extract archive {}: {}",
                    dest.display(),
                    e
                ))
            })?;
        }

        Ok(dest.to_path_buf())
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
                .parallel(self.config.ghcr_concurrency.unwrap_or(8))
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
                    return Err(last_error
                        .unwrap_or_else(|| {
                            DownloadError::Multiple {
                                errors: vec!["Download failed after 5 retries".into()],
                            }
                        })
                        .into());
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
                            warn!(
                                retry = retries,
                                "download failed, retrying after delay: {err}"
                            );
                            sleep(Duration::from_secs(5));
                            retries += 1;
                            if retries > 1 {
                                if let Some(ref callback) = self.progress_callback {
                                    callback(Progress::Error);
                                }
                            }
                            last_error = Some(err);
                        } else {
                            return Err(err.into());
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
            let extract_dir = get_extract_dir(&self.install_dir);

            // Offer extraction unconditionally: the downloader detects the
            // format by magic number and leaves non-archives alone. Relying
            // on pkg_type meant an archive published as "static" installed
            // as an unusable compressed file.
            let should_extract = true;

            let file_path = if let Some(seed) = self.zsync.clone() {
                trace!(
                    url = seed.url,
                    "rebuilding from the installed copy over zsync"
                );
                let callback = self.progress_callback.clone();
                soar_dl::zsync::download(
                    &seed.url,
                    &seed.seed,
                    output_path,
                    callback.map(|cb| move |p| cb(p)),
                )?;
                output_path.to_path_buf()
            } else if let Some(local_src) = local_path_from_url(url) {
                trace!(source = %local_src.display(), "installing from local file");
                self.copy_local_source(local_src, output_path, should_extract, &extract_dir)?
            } else {
                trace!(url = url.as_str(), "using direct download");
                let mut dl = Download::new(url.as_str())
                    .output(output_path.to_string_lossy())
                    .overwrite(OverwriteMode::Skip)
                    .extract(should_extract)
                    .extract_to(&extract_dir);

                if let Some(ref bsum) = self.package.bsum {
                    dl = dl.checksum(bsum);
                }

                if let Some(ref cb) = self.progress_callback {
                    let cb = cb.clone();
                    dl = dl.progress(move |p| {
                        cb(p);
                    });
                }

                dl.execute()?
            };

            self.run_post_download_hook()?;

            let checksum = if PathBuf::from(&file_path).exists() {
                Some(calculate_checksum(&file_path)?)
            } else {
                None
            };

            let extract_path = PathBuf::from(&extract_dir);
            let extracted = extract_path.exists();
            if extracted {
                fs::remove_file(file_path).ok();

                for entry in fs::read_dir(&extract_path)
                    .with_context(|| format!("reading {} directory", extract_path.display()))?
                {
                    let entry = entry.with_context(|| {
                        format!("reading entry from directory {}", extract_path.display())
                    })?;
                    let from = entry.path();
                    let to = self.install_dir.join(entry.file_name());
                    // Renaming a directory rewrites its `..`, so the directory
                    // itself needs the write bit; archives shipping 0555 dirs
                    // would otherwise fail to promote.
                    if let Ok(meta) = fs::metadata(&from) {
                        let mode = meta.permissions().mode();
                        if meta.is_dir() && mode & 0o200 == 0 {
                            fs::set_permissions(
                                &from,
                                std::fs::Permissions::from_mode(mode | 0o200),
                            )
                            .ok();
                        }
                    }
                    // A leftover from an interrupted install would make rename
                    // fail, the same way the other promotion paths treat it.
                    if to.exists() {
                        if to.is_dir() {
                            fs::remove_dir_all(&to).ok();
                        } else {
                            fs::remove_file(&to).ok();
                        }
                    }
                    fs::rename(&from, &to).with_context(|| {
                        format!("renaming {} to {}", from.display(), to.display())
                    })?;
                }

                fs::remove_dir_all(&extract_path).ok();
            }

            // Archives conventionally wrap everything in one versioned
            // directory (foo-1.2.3-x86_64/). Nothing downstream can guess that
            // name, so when extraction leaves exactly one directory behind and
            // no extract_root was given, treat it as the root.
            let auto_root = if self.extract_root.is_none() && extracted {
                let mut dirs = Vec::new();
                let mut files = 0usize;
                if let Ok(rd) = fs::read_dir(&self.install_dir) {
                    for entry in rd.flatten() {
                        let name = entry.file_name();
                        if name.to_string_lossy().starts_with('.') {
                            continue;
                        }
                        if entry.path().is_dir() {
                            dirs.push(name.to_string_lossy().to_string());
                        } else {
                            files += 1;
                        }
                    }
                }
                if files == 0 && dirs.len() == 1 {
                    debug!(root = %dirs[0], "auto-detected single extract root");
                    Some(dirs.remove(0))
                } else {
                    None
                }
            } else {
                None
            };

            // Handle extract_root: move contents from subdirectory to install root
            if let Some(ref root_dir) = self.extract_root.clone().or(auto_root) {
                let root_dir = substitute_placeholders(
                    root_dir,
                    Some(&self.package.version),
                    self.arch_map.as_ref(),
                );
                let root_path =
                    validate_path_containment(&self.install_dir, &root_dir, "extract_root")?;

                if root_path.is_dir() {
                    debug!(
                        "applying extract_root: moving contents from {} to {}",
                        root_path.display(),
                        self.install_dir.display()
                    );

                    // A file inside the root can share the root's own name
                    // (age/age). Promoting it would target the directory
                    // currently being drained, and the clobber below would
                    // delete the rest of the package. Move the root aside
                    // first so source and destination can never collide.
                    let staged = self.install_dir.join(".soar_extract_root");
                    fs::remove_dir_all(&staged).ok();
                    fs::rename(&root_path, &staged).with_context(|| {
                        format!("staging {} for promotion", root_path.display())
                    })?;
                    let root_path = staged;

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

            if let Some(files) = self.package.files.as_deref().filter(|f| !f.is_empty()) {
                apply_file_layout(files, &self.install_dir, output_path)?;
            }

            if extracted {
                mark_elfs_executable(&self.install_dir);
            }

            install_extras(&self.package, &self.install_dir).await?;

            // Handle nested_extract: extract an archive within the package
            if let Some(ref nested_archive) = self.nested_extract {
                let nested_archive = substitute_placeholders(
                    nested_archive,
                    Some(&self.package.version),
                    self.arch_map.as_ref(),
                );
                let archive_path = validate_path_containment(
                    &self.install_dir,
                    &nested_archive,
                    "nested_extract",
                )?;

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
        let pkg_id = package.pkg_id.as_deref();
        let version = &package.version;
        let size = package.ghcr_size.unwrap_or(package.size.unwrap_or(0)) as i64;
        let checksum = package.bsum.as_deref();
        let provides = package.provides.clone();

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
                checksum,
                &installed_date,
                &installed_path,
            )
        })?;

        let record_id = record_id.ok_or_else(|| {
            SoarError::Custom(format!(
                "Failed to record installation for {}: package not found in database",
                pkg_name
            ))
        })?;

        // Only a local or URL install needs its source recorded; a repository
        // package is found again through the index. The update feed lives in
        // the artifact, which is in place by the time this runs.
        if repo_name == "local" {
            let artifact = self.install_dir.join(pkg_name);
            let update_info = UpdateInfo::raw_from_artifact(&artifact);
            self.db.with_conn(|conn| {
                CoreRepository::set_install_source(
                    conn,
                    record_id,
                    Some(package.download_url.as_str()),
                    update_info.as_deref(),
                )
            })?;
        }

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
            self.db.with_conn(|conn| {
                CoreRepository::unlink_others(
                    conn,
                    pkg_name,
                    &self.package.repo_name,
                    pkg_id,
                    self.package.pkg_family.as_deref(),
                    Some(version),
                )
            })?;

            let alternate_packages: Vec<InstalledPackageWithPortable> =
                self.db.with_conn(|conn| {
                    CoreRepository::find_alternates(
                        conn,
                        pkg_name,
                        pkg_id,
                        self.package.pkg_family.as_deref(),
                        version,
                    )
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
                // A directory that was never created holds none of our links,
                // which is not a reason to fail an install that worked.
                let desktop_path = self.config.get_desktop_path()?;
                if desktop_path.is_dir() {
                    walk_dir(&desktop_path, &mut remove_action)?;
                }

                let mut remove_action = |path: &Path| -> FileSystemResult<()> {
                    if let Ok(real_path) = fs::read_link(path) {
                        if real_path.parent() == Some(&installed_path) {
                            safe_remove(path)?;
                        }
                    }
                    Ok(())
                };
                let icons_path = self.config.get_icons_path();
                if icons_path.is_dir() {
                    walk_dir(&icons_path, &mut remove_action)?;
                }

                if let Some(ref provides) = alt_pkg.provides {
                    let bin_path = self.config.get_bin_path()?;
                    remove_provide_symlinks(&bin_path, provides, &installed_path)?;
                }
            }
        }

        self.remove_marker()?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::relative_to;

    #[test]
    fn alias_beside_its_target_is_just_the_name() {
        assert_eq!(
            relative_to("bin/fdfind", "bin/fd").unwrap(),
            Path::new("fd")
        );
    }

    #[test]
    fn alias_in_another_directory_climbs_out() {
        assert_eq!(
            relative_to("share/man/man1/fdfind.1", "share/man/man1/fd.1").unwrap(),
            Path::new("fd.1")
        );
        assert_eq!(
            relative_to("bin/fd", "libexec/fd").unwrap(),
            Path::new("../libexec/fd")
        );
        assert_eq!(relative_to("fd", "bin/fd").unwrap(), Path::new("bin/fd"));
    }
}
