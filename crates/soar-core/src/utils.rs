//! Utility functions for soar-core.

use std::{
    fs,
    path::{Path, PathBuf},
};

use soar_config::config::{get_config, is_system_mode};
use soar_utils::{
    error::FileSystemResult,
    fs::{safe_remove, walk_dir},
    path::icons_dir,
};
use tracing::info;

use crate::error::{ErrorContext, SoarError};

type Result<T> = std::result::Result<T, SoarError>;

/// Sets up required directories for soar operation.
pub fn setup_required_paths() -> Result<()> {
    let config = get_config();
    let bin_path = config.get_bin_path()?;
    if !bin_path.exists() {
        fs::create_dir_all(&bin_path)
            .with_context(|| format!("creating bin directory {}", bin_path.display()))?;
    }

    let db_path = config.get_db_path()?;
    if !db_path.exists() {
        fs::create_dir_all(&db_path)
            .with_context(|| format!("creating database directory {}", db_path.display()))?;
    }

    for profile in config.profile.values() {
        let packages_path = profile.get_packages_path()?;
        if !packages_path.exists() {
            fs::create_dir_all(&packages_path).with_context(|| {
                format!("creating packages directory {}", packages_path.display())
            })?;
        }
    }

    Ok(())
}

/// Cleans up the cache directory.
pub fn cleanup_cache() -> Result<()> {
    let cache_path = get_config().get_cache_path()?;
    if cache_path.exists() {
        fs::remove_dir_all(&cache_path)
            .with_context(|| format!("removing directory {}", cache_path.display()))?;
        info!("Nuked cache directory: {}", cache_path.display());
    } else {
        info!("Cache directory is clean.");
    }

    Ok(())
}

fn remove_action(path: &Path) -> FileSystemResult<()> {
    if path.is_symlink() && !path.exists() {
        safe_remove(path)?;
        info!("Removed broken symlink: {}", path.display());
    }
    Ok(())
}

/// Removes broken symlinks from bin, desktop, and icons directories.
pub fn remove_broken_symlinks() -> Result<()> {
    let mut soar_files_action = |path: &Path| -> FileSystemResult<()> {
        if let Some(filename) = path.file_stem().and_then(|s| s.to_str()) {
            if filename.ends_with("-soar") {
                return remove_action(path);
            }
        }
        Ok(())
    };

    walk_dir(&get_config().get_bin_path()?, &mut remove_action)?;
    walk_dir(&get_config().get_desktop_path()?, &mut soar_files_action)?;
    walk_dir(icons_dir(is_system_mode()), &mut soar_files_action)?;

    Ok(())
}

/// Gets the extract directory path for a given base directory.
pub fn get_extract_dir<P: AsRef<Path>>(base_dir: P) -> PathBuf {
    let base_dir = base_dir.as_ref();
    base_dir.join("SOAR_AUTOEXTRACT")
}

/// Substitute placeholders in a string with system/package metadata.
///
/// Supported placeholders:
/// - `{arch}` - System architecture (e.g., "x86_64", "aarch64")
/// - `{os}` - Operating system (e.g., "linux", "macos")
/// - `{version}` - Package version (if provided)
pub fn substitute_placeholders(template: &str, version: Option<&str>) -> String {
    let result = template
        .replace("{arch}", std::env::consts::ARCH)
        .replace("{os}", std::env::consts::OS);

    match version {
        Some(v) => {
            let normalized_version = v.strip_prefix('v').unwrap_or(v);
            result.replace("{version}", normalized_version)
        }
        None => result,
    }
}
