use std::{
    env::consts::{ARCH, OS},
    fs,
    path::{Path, PathBuf},
};

use regex::Regex;
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

/// Retrieves the platform string in the format `ARCH-Os`.
///
/// This function combines the architecture (e.g., `x86_64`) and the operating
/// system (e.g., `Linux`) into a single string to identify the platform.
pub fn get_platform() -> String {
    format!("{}-{}{}", ARCH, &OS[..1].to_uppercase(), &OS[1..])
}

pub fn parse_duration(input: &str) -> Option<u128> {
    let re = Regex::new(r"(\d+)([smhd])").ok()?;
    let mut total: u128 = 0;

    for cap in re.captures_iter(input) {
        let number: u128 = cap[1].parse().ok()?;
        let multiplier = match &cap[2] {
            "s" => 1000,
            "m" => 60 * 1000,
            "h" => 60 * 60 * 1000,
            "d" => 24 * 60 * 60 * 1000,
            _ => return None,
        };
        total += number * multiplier;
    }

    Some(total)
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

pub fn apply_sig_variants(patterns: Vec<String>) -> Vec<String> {
    patterns
        .into_iter()
        .map(|pat| {
            let (negate, inner) = if let Some(rest) = pat.strip_prefix('!') {
                (true, rest)
            } else {
                (false, pat.as_str())
            };

            let sig_variant = format!("{inner}.sig");
            let brace_pattern = format!("{{{inner},{sig_variant}}}");

            if negate {
                format!("!{brace_pattern}")
            } else {
                brace_pattern
            }
        })
        .collect()
}
