use std::{
    env::{
        self,
        consts::{ARCH, OS},
    },
    fs::{self, File},
    io::{self, BufReader, Read, Seek},
    os,
    path::{Path, PathBuf},
};

use nix::unistd::{geteuid, User};
use tracing::info;

use crate::{
    config::get_config,
    error::{ErrorContext, SoarError},
    SoarResult,
};

type Result<T> = std::result::Result<T, SoarError>;

fn get_username() -> Result<String> {
    let uid = geteuid();
    User::from_uid(uid)?
        .ok_or_else(|| panic!("Failed to get user"))
        .map(|user| user.name)
}

pub fn home_path() -> String {
    env::var("HOME").unwrap_or_else(|_| {
        let username = env::var("USER")
            .or_else(|_| env::var("LOGNAME"))
            .or_else(|_| get_username().map_err(|_| ()))
            .unwrap_or_else(|_| panic!("Couldn't determine username. Please fix the system."));
        format!("/home/{username}")
    })
}

pub fn home_config_path() -> String {
    env::var("XDG_CONFIG_HOME").unwrap_or(format!("{}/.config", home_path()))
}

pub fn home_cache_path() -> String {
    env::var("XDG_CACHE_HOME").unwrap_or(format!("{}/.cache", home_path()))
}

pub fn home_data_path() -> String {
    env::var("XDG_DATA_HOME").unwrap_or(format!("{}/.local/share", home_path()))
}

/// Expands the environment variables and user home directory in a given path.
pub fn build_path(path: &str) -> Result<PathBuf> {
    let mut result = String::new();
    let mut chars = path.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '$' {
            let mut var_name = String::new();
            while let Some(&c) = chars.peek() {
                if !c.is_alphanumeric() && c != '_' {
                    break;
                }
                var_name.push(chars.next().unwrap());
            }
            if !var_name.is_empty() {
                let expanded = if var_name == "HOME" {
                    home_path()
                } else {
                    env::var(&var_name)?
                };
                result.push_str(&expanded);
            } else {
                result.push('$');
            }
        } else if c == '~' && result.is_empty() {
            result.push_str(&home_path())
        } else {
            result.push(c);
        }
    }

    Ok(PathBuf::from(result))
}

pub fn format_bytes(bytes: u64) -> String {
    let kb = 1024u64;
    let mb = kb * 1024;
    let gb = mb * 1024;

    match bytes {
        b if b >= gb => format!("{:.2} GiB", b as f64 / gb as f64),
        b if b >= mb => format!("{:.2} MiB", b as f64 / mb as f64),
        b if b >= kb => format!("{:.2} KiB", b as f64 / kb as f64),
        _ => format!("{bytes} B"),
    }
}

pub fn parse_size(size_str: &str) -> Option<u64> {
    let size_str = size_str.trim();
    let units = [
        ("B", 1u64),
        ("KB", 1000u64),
        ("MB", 1000u64 * 1000),
        ("GB", 1000u64 * 1000 * 1000),
        ("KiB", 1024u64),
        ("MiB", 1024u64 * 1024),
        ("GiB", 1024u64 * 1024 * 1024),
    ];

    for (unit, multiplier) in &units {
        let size_str = size_str.to_uppercase();
        if size_str.ends_with(unit) {
            let number_part = size_str.trim_end_matches(unit).trim();
            if let Ok(num) = number_part.parse::<f64>() {
                return Some((num * (*multiplier as f64)) as u64);
            }
        }
    }

    None
}

pub fn calculate_checksum<P: AsRef<Path>>(file_path: P) -> Result<String> {
    let file_path = file_path.as_ref();
    let mut hasher = blake3::Hasher::new();
    hasher
        .update_mmap(file_path)
        .with_context(|| format!("reading {} using memory mapping", file_path.display()))?;
    Ok(hasher.finalize().to_hex().to_string())
}

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

pub fn calc_magic_bytes<P: AsRef<Path>>(file_path: P, size: usize) -> Result<Vec<u8>> {
    let file_path = file_path.as_ref();
    let file = File::open(file_path).with_context(|| format!("opening {}", file_path.display()))?;
    let mut file = BufReader::new(file);
    let mut magic_bytes = vec![0u8; size];
    file.read_exact(&mut magic_bytes)
        .with_context(|| format!("reading magic bytes from {}", file_path.display()))?;
    file.rewind().unwrap();
    Ok(magic_bytes)
}

pub fn create_symlink<P: AsRef<Path>>(from: P, to: P) -> SoarResult<()> {
    let from = from.as_ref();
    let to = to.as_ref();

    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating parent directory {}", parent.display()))?;
    }

    if to.is_symlink() {
        fs::remove_file(to).with_context(|| format!("removing symlink {}", to.display()))?;
    }
    os::unix::fs::symlink(from, to)
        .with_context(|| format!("creating symlink {} -> {}", from.display(), to.display()))?;
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

pub fn process_dir<P: AsRef<Path>, F>(dir: P, action: &mut F) -> Result<()>
where
    F: FnMut(&Path) -> Result<()>,
{
    let dir = dir.as_ref();
    if !dir.is_dir() {
        return Ok(());
    }

    for entry in
        fs::read_dir(dir).with_context(|| format!("reading directory {}", dir.display()))?
    {
        let path = entry
            .with_context(|| format!("reading entry from directory {}", dir.display()))?
            .path();

        if path.is_dir() {
            process_dir(&path, action)?;
            continue;
        }

        action(&path)?;
    }

    Ok(())
}

fn remove_action(path: &Path) -> Result<()> {
    if !path.exists() {
        fs::remove_file(path)
            .with_context(|| format!("removing broken symlink {}", path.display()))?;
        info!("Removed broken symlink: {}", path.display());
    }
    Ok(())
}

pub fn remove_broken_symlinks() -> Result<()> {
    let mut soar_files_action = |path: &Path| -> SoarResult<()> {
        if let Some(filename) = path.file_stem().and_then(|s| s.to_str()) {
            if filename.ends_with("-soar") {
                return remove_action(path);
            }
        }
        Ok(())
    };

    process_dir(&get_config().get_bin_path()?, &mut remove_action)?;
    process_dir(desktop_dir(), &mut soar_files_action)?;
    process_dir(icons_dir(), &mut soar_files_action)?;

    Ok(())
}

pub fn desktop_dir() -> String {
    format!("{}/applications", home_data_path())
}

pub fn icons_dir() -> String {
    format!("{}/icons/hicolor", home_data_path())
}

/// Retrieves the platform string in the format `ARCH-Os`.
///
/// This function combines the architecture (e.g., `x86_64`) and the operating
/// system (e.g., `Linux`) into a single string to identify the platform.
pub fn get_platform() -> String {
    format!("{}-{}{}", ARCH, &OS[..1].to_uppercase(), &OS[1..])
}

pub fn calculate_dir_size<P: AsRef<Path>>(path: P) -> io::Result<u64> {
    let mut total_size = 0;
    let path = path.as_ref();

    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let Ok(entry) = entry else {
                continue;
            };
            let Ok(metadata) = entry.metadata() else {
                continue;
            };

            if metadata.is_file() {
                total_size += metadata.len();
            } else if metadata.is_dir() {
                total_size += calculate_dir_size(entry.path())?;
            }
        }
    }

    Ok(total_size)
}

pub fn parse_duration(input: &str) -> Option<u128> {
    let (num_part, unit_part) = input
        .trim()
        .split_at(input.find(|c: char| !c.is_numeric()).unwrap_or(input.len()));
    let number: u128 = num_part.parse().ok()?;
    let multiplier = match unit_part {
        "s" => 1000,
        "m" => 60 * 1000,
        "h" => 60 * 60 * 1000,
        "d" => 24 * 60 * 60 * 1000,
        _ => return None,
    };

    Some(multiplier * number)
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
