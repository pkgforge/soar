//! Common package integration utilities.
//!
//! This module provides functions for desktop integration including
//! icon handling, desktop file creation, and portable directory setup.

use std::{
    collections::{HashMap, HashSet},
    env,
    ffi::OsStr,
    fs::{self, File},
    io::{BufReader, BufWriter, Write},
    path::{Path, PathBuf},
};

use image::{imageops::FilterType, DynamicImage, GenericImageView};
use regex::Regex;
use soar_config::config::get_config;
use soar_utils::{
    fs::{create_symlink, walk_dir},
    path::{icons_dir, is_safe_component},
};
use tracing::{debug, trace};

use super::{
    appimage::integrate_appimage, get_file_type, onelf::integrate_onelf,
    wrappe::setup_wrappe_portable_dir, PackageFormat,
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
    let icon_name = real_path.as_ref().file_stem().unwrap().to_string_lossy();
    symlink_icon_with_mode(&real_path, &icon_name, false)
}

/// Creates a symlink for an icon in the appropriate icons directory.
///
/// The symlink is named `{icon_name}-soar`, and the `-soar` suffix is what marks
/// the link as soar-managed. Uses the provided `system_mode` flag to determine
/// the icons directory.
pub fn symlink_icon_with_mode<P: AsRef<Path>>(
    real_path: P,
    icon_name: &str,
    system_mode: bool,
) -> Result<PathBuf> {
    let real_path = real_path.as_ref();
    trace!(path = %real_path.display(), icon_name = icon_name, "creating icon symlink");
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

    let final_path = icons_dir(system_mode)
        .join(format!("{w}x{h}"))
        .join("apps")
        .join(format!(
            "{icon_name}-soar.{}",
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

/// The `Name` of the `[Desktop Entry]` group, if the file declares one.
///
/// Localized keys (`Name[de]`) and the `Name` of any other group, such as a
/// `[Desktop Action ...]`, are not the application's name and are skipped.
pub(crate) fn desktop_entry_name(content: &str) -> Option<&str> {
    let mut in_entry = false;
    for line in content.lines() {
        let line = line.trim();
        if let Some(group) = line.strip_prefix('[') {
            in_entry = group.trim_end_matches(']') == "Desktop Entry";
            continue;
        }
        if !in_entry {
            continue;
        }
        if let Some(value) = line.strip_prefix("Name=") {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

/// The name soar gives the icon it manages for a desktop entry.
///
/// Derived from the entry's `Name` so that packages whose file name is a
/// generic word, such as an AppImage that calls itself `desktop`, don't land in
/// the namespace the icon theme already uses for its own generic icons.
/// Falls back to `fallback` when the name yields nothing usable.
pub(crate) fn managed_icon_name(desktop_name: &str, fallback: &str) -> String {
    let mut name = String::with_capacity(desktop_name.len());
    for ch in desktop_name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' {
            name.extend(ch.to_lowercase());
        } else if !name.ends_with('-') {
            name.push('-');
        }
    }

    let name = name.trim_matches(['-', '.']);
    if name.is_empty() || !is_safe_component(name) {
        return fallback.to_string();
    }
    name.to_string()
}

/// An `Exec` or `TryExec` line pointed at the command as installed.
///
/// The arguments the entry carried are kept after it. With none to keep,
/// nothing follows the command: a launcher reading the line whole would take
/// a trailing space as part of the command and fail to find it.
fn exec_line(field: &str, old: &str, command: &str) -> String {
    if old.contains("{{pkg_path}}") {
        return format!("{field}={}", old.replace("{{pkg_path}}", command));
    }

    let args: Vec<&str> = old.split_whitespace().skip(1).collect();
    if args.is_empty() {
        format!("{field}={command}")
    } else {
        format!("{field}={command} {}", args.join(" "))
    }
}

/// Creates a symlink for a desktop file with modified fields.
///
/// Updates the Exec and TryExec fields in the desktop file to point to the
/// installed package, then creates a symlink in the applications directory.
/// The Icon field is rewritten to the soar-managed icon only when the package
/// ships a matching icon (`icon_name`); otherwise it is left untouched so that
/// references to generic system icons keep working.
///
/// # Arguments
///
/// * `real_path` - Path to the desktop file
/// * `package` - Package metadata
/// * `icon_name` - Name of the soar-managed icon for this desktop file, if the
///   package ships one
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
    icon_name: Option<&str>,
) -> Result<PathBuf> {
    symlink_desktop_with_config(real_path, package, icon_name, &get_config())
}

/// Creates a symlink for a desktop file using the provided config.
///
/// Uses the provided `config` to determine bin and desktop paths
/// instead of the global config.
pub fn symlink_desktop_with_config<P: AsRef<Path>, T: PackageExt>(
    real_path: P,
    package: &T,
    icon_name: Option<&str>,
    config: &soar_config::config::Config,
) -> Result<PathBuf> {
    let pkg_name = package.pkg_name();
    let real_path = real_path.as_ref();
    trace!(path = %real_path.display(), pkg_name = pkg_name, "creating desktop file symlink");
    let content = fs::read_to_string(real_path)
        .with_context(|| format!("reading content of desktop file: {}", real_path.display()))?;
    let file_name = real_path.file_stem().unwrap();

    let bin_path = config.get_bin_path()?;

    let final_content = {
        let re = Regex::new(r"(?m)^(Icon|Exec|TryExec)=(.*)").unwrap();

        re.replace_all(&content, |caps: &regex::Captures| {
            match &caps[1] {
                "Icon" => {
                    match icon_name {
                        Some(icon_name) => format!("Icon={icon_name}-soar"),
                        None => caps[0].to_string(),
                    }
                }
                "Exec" | "TryExec" => {
                    exec_line(
                        &caps[1],
                        &caps[2],
                        &format!("{}/{}", bin_path.display(), pkg_name),
                    )
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

    let final_path = config
        .get_desktop_path()?
        .join(format!("{}-soar.desktop", file_name.to_string_lossy()));

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
    // Packages that carry an id keep their existing directory name. Without
    // one the family has to stand in, or two packages sharing a name would
    // share a portable directory. Neither is trusted to be a single path
    // component: both come from metadata, and one holding `..` would put the
    // directory outside the portable root.
    let family = package.pkg_family().filter(|f| is_safe_component(f));
    let portable_dir_base = get_config().get_portable_dirs()?.join(
        match (package.pkg_id().filter(|id| is_safe_component(id)), family) {
            (Some(pkg_id), _) => format!("{}-{}", package.pkg_name(), pkg_id),
            (None, Some(family)) => format!("{}-{}", package.pkg_name(), family),
            (None, None) => package.pkg_name().to_string(),
        },
    );
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
    config: &soar_config::config::Config,
) -> Result<()> {
    let install_dir = install_dir.as_ref();
    let pkg_name = package.pkg_name();
    debug!(pkg_name = pkg_name, install_dir = %install_dir.display(), "integrating package with desktop environment");
    let bin_path = bin_path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| install_dir.join(pkg_name));

    let system_mode = config.is_system();

    let mut icon_paths: Vec<PathBuf> = Vec::new();
    let mut desktop_paths: Vec<PathBuf> = Vec::new();
    let mut collect_action = |path: &Path| -> Result<()> {
        // Never treat the package binary itself as a desktop file. Its name can
        // legitimately end in `.desktop`, but its contents are the executable.
        if path == bin_path.as_path() {
            return Ok(());
        }
        let ext = path.extension();
        if ext == Some(OsStr::new("png")) || ext == Some(OsStr::new("svg")) {
            icon_paths.push(path.to_path_buf());
        } else if ext == Some(OsStr::new("desktop")) {
            desktop_paths.push(path.to_path_buf());
        }
        Ok(())
    };
    walk_dir(install_dir, &mut collect_action)?;

    // An icon is named after the app rather than the file it came from, so the
    // desktop files have to be read before any icon is linked.
    let mut icon_names: HashMap<String, String> = HashMap::new();
    for path in &desktop_paths {
        let Some(stem) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
            continue;
        };
        let name = fs::read_to_string(path)
            .ok()
            .and_then(|content| desktop_entry_name(&content).map(|name| name.to_string()))
            .map(|name| managed_icon_name(&name, &stem))
            .unwrap_or_else(|| stem.clone());
        icon_names.insert(stem, name);
    }

    let mut icon_stems: HashSet<String> = HashSet::new();
    for path in &icon_paths {
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        let icon_name = icon_names.get(&*stem).map(String::as_str).unwrap_or(&stem);
        symlink_icon_with_mode(path, icon_name, system_mode)?;
        icon_stems.insert(stem.into_owned());
    }

    for path in &desktop_paths {
        // Only rewrite the Icon field when this desktop file has a matching
        // icon shipped by the package (matched by file stem).
        let icon_name = path
            .file_stem()
            .map(|stem| stem.to_string_lossy())
            .filter(|stem| icon_stems.contains(&**stem))
            .and_then(|stem| icon_names.get(&*stem))
            .map(String::as_str);
        symlink_desktop_with_config(path, package, icon_name, config)?;
    }

    let has_icon = !icon_paths.is_empty();
    let has_desktop = !desktop_paths.is_empty();
    // The formats below extract their desktop file as `{pkg_name}.desktop`, so
    // only an icon under that stem can be the one it refers to.
    let pkg_icon_name = icon_stems.contains(pkg_name).then(|| {
        icon_names
            .get(pkg_name)
            .map(String::as_str)
            .unwrap_or(pkg_name)
    });

    let mut reader = BufReader::new(
        File::open(&bin_path).with_context(|| format!("opening {}", bin_path.display()))?,
    );
    let file_type = get_file_type(&mut reader)?;

    trace!(file_type = ?file_type, "detected package format");
    match file_type {
        PackageFormat::AppImage | PackageFormat::RunImage => {
            if matches!(file_type, PackageFormat::AppImage) {
                trace!("integrating AppImage resources");
                let _ = integrate_appimage(
                    install_dir,
                    &bin_path,
                    package,
                    has_icon,
                    pkg_icon_name,
                    has_desktop,
                    config,
                )
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
        PackageFormat::Onelf => {
            trace!("integrating onelf resources");
            let _ = integrate_onelf(
                install_dir,
                &bin_path,
                package,
                has_icon,
                pkg_icon_name,
                has_desktop,
                config,
            )
            .await;
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

#[cfg(test)]
mod tests {
    use std::fs;

    use soar_config::config::Config;
    use tempfile::TempDir;

    use super::{desktop_entry_name, exec_line, managed_icon_name, symlink_desktop_with_config};
    use crate::traits::PackageExt;

    struct TestPackage;

    impl PackageExt for TestPackage {
        fn pkg_name(&self) -> &str {
            "desktop"
        }

        fn pkg_id(&self) -> Option<&str> {
            None
        }

        fn pkg_family(&self) -> Option<&str> {
            None
        }

        fn version(&self) -> &str {
            "3.5"
        }

        fn repo_name(&self) -> &str {
            "local"
        }
    }

    /// Writes `ENTRY` into a temporary install dir and integrates it, then
    /// hands back what the rewritten desktop file ended up saying.
    fn integrated_desktop(icon_name: Option<&str>) -> (TempDir, String) {
        let dir = TempDir::new().unwrap();
        let mut config = Config::default_config::<&str>(&[]);
        config.bin_path = Some(dir.path().join("bin").to_string_lossy().into_owned());
        config.desktop_path = Some(
            dir.path()
                .join("applications")
                .to_string_lossy()
                .into_owned(),
        );
        fs::create_dir_all(dir.path().join("applications")).unwrap();

        let real_path = dir.path().join("desktop.desktop");
        fs::write(&real_path, ENTRY).unwrap();
        symlink_desktop_with_config(&real_path, &TestPackage, icon_name, &config).unwrap();

        let content = fs::read_to_string(&real_path).unwrap();
        (dir, content)
    }

    #[test]
    fn a_shipped_icon_is_referenced_by_its_managed_name() {
        let (_dir, content) = integrated_desktop(Some("noteboard"));
        assert!(content.contains("Icon=noteboard-soar"), "{content}");
    }

    #[test]
    fn an_entry_with_no_shipped_icon_keeps_its_own_icon() {
        let (_dir, content) = integrated_desktop(None);
        assert!(content.contains("Icon=desktop"), "{content}");
        assert!(!content.contains("-soar"), "{content}");
    }

    /// An entry shaped like the ones that motivated naming icons after the app:
    /// the file it ships as is called `desktop`, but the app is not.
    const ENTRY: &str = "\
[Desktop Entry]
Name[de]=NoteBoard Schreibtisch
Name=NoteBoard
Icon=desktop

[Desktop Action new-note]
Name=New Note
";

    #[test]
    fn the_app_name_wins_over_localized_and_action_names() {
        assert_eq!(desktop_entry_name(ENTRY), Some("NoteBoard"));
        assert_eq!(desktop_entry_name("[Desktop Entry]\nIcon=desktop\n"), None);
        assert_eq!(desktop_entry_name("Name=Stray\n"), None);
        assert_eq!(
            desktop_entry_name("[Desktop Action open]\nName=Open\n"),
            None
        );
    }

    #[test]
    fn an_icon_name_is_a_lowercase_slug_of_the_app_name() {
        assert_eq!(managed_icon_name("NoteBoard", "desktop"), "noteboard");
        assert_eq!(
            managed_icon_name("Note Board Studio", "desktop"),
            "note-board-studio"
        );
        assert_eq!(
            managed_icon_name("Note  Board / Studio", "desktop"),
            "note-board-studio"
        );
    }

    #[test]
    fn an_unusable_app_name_falls_back_to_the_file_stem() {
        assert_eq!(managed_icon_name("", "desktop"), "desktop");
        assert_eq!(managed_icon_name("///", "desktop"), "desktop");
        assert_eq!(managed_icon_name("...", "desktop"), "desktop");
    }

    #[test]
    fn an_app_name_cannot_escape_the_icons_directory() {
        assert_eq!(managed_icon_name("../../evil", "desktop"), "evil");
        assert_eq!(managed_icon_name("a/b", "desktop"), "a-b");
    }

    #[test]
    fn a_command_with_no_arguments_ends_the_line() {
        assert_eq!(exec_line("Exec", "/old/bin", "/new/bin"), "Exec=/new/bin");
        assert_eq!(
            exec_line("TryExec", "/old/bin", "/new/bin"),
            "TryExec=/new/bin"
        );
    }

    #[test]
    fn the_arguments_an_entry_carried_are_kept() {
        assert_eq!(
            exec_line("Exec", "/old/bin --flag %U", "/new/bin"),
            "Exec=/new/bin --flag %U"
        );
    }

    #[test]
    fn a_placeholder_is_filled_where_it_stands() {
        assert_eq!(
            exec_line("Exec", "env FOO=1 {{pkg_path}} %F", "/new/bin"),
            "Exec=env FOO=1 /new/bin %F"
        );
    }
}
