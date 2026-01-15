//! Common package integration utilities.
//!
//! This module provides functions for desktop integration including
//! icon handling, desktop file creation, and portable directory setup.

use std::{
    env,
    ffi::OsStr,
    fs::{self, File},
    io::{BufReader, BufWriter, Write},
    path::{Path, PathBuf},
};

use image::{imageops::FilterType, DynamicImage, GenericImageView};
use regex::Regex;
use soar_config::config::{get_config, is_system_mode};
use soar_utils::{
    fs::{create_symlink, walk_dir},
    path::{desktop_dir, icons_dir},
};
use tracing::{debug, trace};

use super::{
    appimage::integrate_appimage, get_file_type, wrappe::setup_wrappe_portable_dir, PackageFormat,
};
use crate::{
    error::{ErrorContext, PackageError, Result},
    traits::PackageExt,
};

/// Supported icon dimensions for desktop integration.
const SUPPORTED_DIMENSIONS: &[(u32, u32)] = &[
    (16, 16),
    (24, 24),
    (32, 32),
    (48, 48),
    (64, 64),
    (72, 72),
    (80, 80),
    (96, 96),
    (128, 128),
    (192, 192),
    (256, 256),
    (512, 512),
];

fn find_nearest_supported_dimension(width: u32, height: u32) -> (u32, u32) {
    SUPPORTED_DIMENSIONS
        .iter()
        .min_by_key(|&&(w, h)| {
            let width_diff = (w as i32 - width as i32).abs();
            let height_diff = (h as i32 - height as i32).abs();
            width_diff + height_diff
        })
        .cloned()
        .unwrap_or((width, height))
}

fn normalize_image(image: DynamicImage) -> DynamicImage {
    let (width, height) = image.dimensions();
    let (new_width, new_height) = find_nearest_supported_dimension(width, height);

    if (width, height) != (new_width, new_height) {
        image.resize(new_width, new_height, FilterType::Lanczos3)
    } else {
        image
    }
}

/// Creates a symlink for an icon in the appropriate icons directory.
///
/// The icon is normalized to a supported dimension and symlinked to
/// `~/.local/share/icons/{WxH}/apps/{name}-soar.{ext}`.
///
/// # Arguments
///
/// * `real_path` - Path to the actual icon file
///
/// # Returns
///
/// The path to the created symlink.
///
/// # Errors
///
/// Returns [`PackageError`] if image processing or symlink creation fails.
pub fn symlink_icon<P: AsRef<Path>>(real_path: P) -> Result<PathBuf> {
    let real_path = real_path.as_ref();
    trace!(path = %real_path.display(), "creating icon symlink");
    let icon_name = real_path.file_stem().unwrap();
    let ext = real_path.extension();

    let (w, h) = if ext == Some(OsStr::new("svg")) {
        (128, 128)
    } else {
        let image = image::open(real_path)?;
        let (orig_w, orig_h) = image.dimensions();

        let normalized_image = normalize_image(image);
        let (w, h) = normalized_image.dimensions();

        if (w, h) != (orig_w, orig_h) {
            normalized_image.save(real_path)?;
        }

        (w, h)
    };

    let final_path = icons_dir(is_system_mode())
        .join(format!("{w}x{h}"))
        .join("apps")
        .join(format!(
            "{}-soar.{}",
            icon_name.to_string_lossy(),
            ext.unwrap_or_default().to_string_lossy()
        ));

    if final_path.is_symlink() {
        fs::remove_file(&final_path)
            .with_context(|| format!("removing existing symlink at {}", final_path.display()))?;
    }

    create_symlink(real_path, &final_path)?;
    debug!(icon = %final_path.display(), "icon symlink created");
    Ok(final_path)
}

/// Creates a symlink for a desktop file with modified fields.
///
/// Updates the Icon, Exec, and TryExec fields in the desktop file to point
/// to the installed package, then creates a symlink in the applications
/// directory.
///
/// # Arguments
///
/// * `real_path` - Path to the desktop file
/// * `package` - Package metadata
///
/// # Returns
///
/// The path to the created symlink.
///
/// # Errors
///
/// Returns [`PackageError`] if file operations fail.
pub fn symlink_desktop<P: AsRef<Path>, T: PackageExt>(
    real_path: P,
    package: &T,
) -> Result<PathBuf> {
    let pkg_name = package.pkg_name();
    let real_path = real_path.as_ref();
    trace!(path = %real_path.display(), pkg_name = pkg_name, "creating desktop file symlink");
    let content = fs::read_to_string(real_path)
        .with_context(|| format!("reading content of desktop file: {}", real_path.display()))?;
    let file_name = real_path.file_stem().unwrap();

    let bin_path = get_config().get_bin_path()?;

    let final_content = {
        let re = Regex::new(r"(?m)^(Icon|Exec|TryExec)=(.*)").unwrap();

        re.replace_all(&content, |caps: &regex::Captures| {
            match &caps[1] {
                "Icon" => format!("Icon={}-soar", file_name.to_string_lossy()),
                "Exec" | "TryExec" => {
                    let value = &caps[0];
                    let new_value = format!("{}/{}", &bin_path.display(), pkg_name);

                    if value.contains("{{pkg_path}}") {
                        value.replace("{{pkg_path}}", &new_value)
                    } else {
                        format!("{}={}", &caps[1], new_value)
                    }
                }
                _ => unreachable!(),
            }
        })
        .to_string()
    };

    let mut writer = BufWriter::new(
        File::create(real_path)
            .with_context(|| format!("creating desktop file {}", real_path.display()))?,
    );
    writer
        .write_all(final_content.as_bytes())
        .with_context(|| format!("writing desktop file to {}", real_path.display()))?;

    let final_path =
        desktop_dir(is_system_mode()).join(format!("{}-soar.desktop", file_name.to_string_lossy()));

    if final_path.is_symlink() {
        fs::remove_file(&final_path)
            .with_context(|| format!("removing existing symlink at {}", final_path.display()))?;
    }

    create_symlink(real_path, &final_path)?;
    debug!(desktop = %final_path.display(), "desktop file symlink created");
    Ok(final_path)
}

/// Creates a portable link for package data directories.
///
/// # Arguments
///
/// * `portable_path` - Base path for portable data
/// * `real_path` - Path to link to
/// * `pkg_name` - Package name
/// * `extension` - Extension for the portable directory (e.g., "home", "config")
///
/// # Errors
///
/// Returns [`PackageError`] if directory creation or symlink fails.
pub fn create_portable_link<P: AsRef<Path>>(
    portable_path: P,
    real_path: P,
    pkg_name: &str,
    extension: &str,
) -> Result<()> {
    let base_dir = env::current_dir()
        .map_err(|_| PackageError::Custom("Error retrieving current directory".into()))?;
    let portable_path = portable_path.as_ref();
    let portable_path = if portable_path.is_absolute() {
        portable_path
    } else {
        &base_dir.join(portable_path)
    };
    let portable_path = portable_path.join(pkg_name).with_extension(extension);

    fs::create_dir_all(&portable_path)
        .with_context(|| format!("creating directory {}", portable_path.display()))?;
    create_symlink(&portable_path, real_path)?;
    Ok(())
}

/// Sets up portable directories for a package.
///
/// Creates symlinks for home, config, share, and cache directories based
/// on the provided portable path options.
///
/// # Arguments
///
/// * `bin_path` - Path to the package binary
/// * `package` - Package metadata
/// * `portable` - Base portable path (overrides all individual paths)
/// * `portable_home` - Path for home directory
/// * `portable_config` - Path for config directory
/// * `portable_share` - Path for share directory
/// * `portable_cache` - Path for cache directory
///
/// # Errors
///
/// Returns [`PackageError`] if directory creation or symlink fails.
pub fn setup_portable_dir<P: AsRef<Path>, T: PackageExt>(
    bin_path: P,
    package: &T,
    portable: Option<&str>,
    portable_home: Option<&str>,
    portable_config: Option<&str>,
    portable_share: Option<&str>,
    portable_cache: Option<&str>,
) -> Result<()> {
    let portable_dir_base = get_config().get_portable_dirs()?.join(format!(
        "{}-{}",
        package.pkg_name(),
        package.pkg_id()
    ));
    let bin_path = bin_path.as_ref();

    let pkg_name = package.pkg_name();
    let pkg_config = bin_path.with_extension("config");
    let pkg_home = bin_path.with_extension("home");
    let pkg_share = bin_path.with_extension("share");
    let pkg_cache = bin_path.with_extension("cache");

    let (portable_home, portable_config, portable_share, portable_cache) =
        if let Some(portable) = portable {
            (
                Some(portable),
                Some(portable),
                Some(portable),
                Some(portable),
            )
        } else {
            (
                portable_home,
                portable_config,
                portable_share,
                portable_cache,
            )
        };

    for (opt, target, kind) in [
        (portable_home, &pkg_home, "home"),
        (portable_config, &pkg_config, "config"),
        (portable_share, &pkg_share, "share"),
        (portable_cache, &pkg_cache, "cache"),
    ] {
        if let Some(val) = opt {
            let base = if val.is_empty() {
                &portable_dir_base
            } else {
                Path::new(val)
            };
            create_portable_link(base, target, pkg_name, kind)?;
        }
    }

    Ok(())
}

/// Integrates a package with the desktop environment.
///
/// This function handles format-specific integration including:
/// - Desktop file symlinking
/// - Icon symlinking with dimension normalization
/// - AppImage resource extraction
/// - Portable directory setup
///
/// # Arguments
///
/// * `install_dir` - Directory where the package is installed
/// * `package` - Package metadata
/// * `bin_path` - Optional path to the actual binary (if None, uses install_dir/pkg_name)
/// * `portable` - Base portable path
/// * `portable_home` - Path for home directory
/// * `portable_config` - Path for config directory
/// * `portable_share` - Path for share directory
/// * `portable_cache` - Path for cache directory
///
/// # Errors
///
/// Returns [`PackageError`] if integration fails.
#[allow(clippy::too_many_arguments)]
pub async fn integrate_package<P: AsRef<Path>, T: PackageExt>(
    install_dir: P,
    package: &T,
    bin_path: Option<&Path>,
    portable: Option<&str>,
    portable_home: Option<&str>,
    portable_config: Option<&str>,
    portable_share: Option<&str>,
    portable_cache: Option<&str>,
) -> Result<()> {
    let install_dir = install_dir.as_ref();
    let pkg_name = package.pkg_name();
    debug!(pkg_name = pkg_name, install_dir = %install_dir.display(), "integrating package with desktop environment");
    let bin_path = bin_path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| install_dir.join(pkg_name));

    let mut has_desktop = false;
    let mut has_icon = false;
    let mut symlink_action = |path: &Path| -> Result<()> {
        let ext = path.extension();
        if ext == Some(OsStr::new("desktop")) {
            has_desktop = true;
            symlink_desktop(path, package)?;
        }
        Ok(())
    };
    walk_dir(install_dir, &mut symlink_action)?;

    let mut symlink_action = |path: &Path| -> Result<()> {
        let ext = path.extension();
        if ext == Some(OsStr::new("png")) || ext == Some(OsStr::new("svg")) {
            has_icon = true;
            symlink_icon(path)?;
        }
        Ok(())
    };
    walk_dir(install_dir, &mut symlink_action)?;

    let mut reader = BufReader::new(
        File::open(&bin_path).with_context(|| format!("opening {}", bin_path.display()))?,
    );
    let file_type = get_file_type(&mut reader)?;

    trace!(file_type = ?file_type, "detected package format");
    match file_type {
        PackageFormat::AppImage | PackageFormat::RunImage => {
            if matches!(file_type, PackageFormat::AppImage) {
                trace!("integrating AppImage resources");
                let _ = integrate_appimage(install_dir, &bin_path, package, has_icon, has_desktop)
                    .await;
            }
            trace!("setting up portable directories");
            setup_portable_dir(
                bin_path,
                package,
                portable,
                portable_home,
                portable_config,
                portable_share,
                portable_cache,
            )?;
        }
        PackageFormat::FlatImage => {
            trace!("setting up FlatImage portable config");
            setup_portable_dir(
                format!("{}/.{}", bin_path.parent().unwrap().display(), pkg_name),
                package,
                None,
                None,
                portable_config,
                None,
                None,
            )?;
        }
        PackageFormat::Wrappe => {
            trace!("setting up Wrappe portable directory");
            setup_wrappe_portable_dir(&bin_path, pkg_name, portable)?;
        }
        _ => {}
    }

    debug!(
        pkg_name = pkg_name,
        has_desktop = has_desktop,
        has_icon = has_icon,
        "package integration completed"
    );
    Ok(())
}
