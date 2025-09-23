use std::{
    fs,
    path::{Path, PathBuf},
};

use soar_utils::{
    error::{FileSystemError, FileSystemResult},
    fs::walk_dir,
    path::{desktop_dir, icons_dir},
};
use tracing::info;

use crate::{
    config::get_config,
    error::{ErrorContext, SoarError},
};

type Result<T> = std::result::Result<T, SoarError>;

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
    if !path.exists() {
        fs::remove_file(path).map_err(|err| FileSystemError::File {
            path: path.to_path_buf(),
            action: "remove",
            source: err,
        })?;
        info!("Removed broken symlink: {}", path.display());
    }
    Ok(())
}

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
    walk_dir(desktop_dir(), &mut soar_files_action)?;
    walk_dir(icons_dir(), &mut soar_files_action)?;

    Ok(())
}

pub fn default_install_patterns() -> Vec<String> {
    ["!*.log", "!SBUILD", "!*.json", "!*.version"]
        .into_iter()
        .map(String::from)
        .collect::<Vec<String>>()
}

pub fn get_extract_dir<P: AsRef<Path>>(base_dir: P) -> PathBuf {
    let base_dir = base_dir.as_ref();
    base_dir.join("SOAR_AUTOEXTRACT")
}
