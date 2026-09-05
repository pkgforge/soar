//! AppImage format handling.

use std::{fs, path::Path};

use soar_utils::fs::read_file_signature;
use squishy::appimage::{AppImage, AppImageEntryKind};

use super::{
    common::{
        desktop_entry_name, managed_icon_name, symlink_desktop_with_config, symlink_icon_with_mode,
    },
    PNG_MAGIC_BYTES,
};
use crate::{
    error::{ErrorContext, Result},
    traits::PackageExt,
};

/// Integrates an AppImage by extracting its embedded resources.
///
/// This function extracts icons, desktop files, and AppStream metadata from
/// an AppImage and sets up the appropriate symlinks for desktop integration.
///
/// # Arguments
///
/// * `install_dir` - Directory where the package is installed
/// * `file_path` - Path to the AppImage file
/// * `package` - Package metadata
/// * `has_icon` - Whether an icon was already found in the install directory
/// * `pkg_icon_name` - Managed name of the already-linked icon that matches this
///   package, if there is one
/// * `has_desktop` - Whether a desktop file was already found
///
/// # Errors
///
/// Returns [`PackageError`] if extraction or symlink creation fails.
pub async fn integrate_appimage<P: AsRef<Path>, T: PackageExt>(
    install_dir: P,
    file_path: P,
    package: &T,
    has_icon: bool,
    pkg_icon_name: Option<&str>,
    has_desktop: bool,
    config: &soar_config::config::Config,
) -> Result<()> {
    if has_icon && has_desktop {
        return Ok(());
    }

    let install_dir = install_dir.as_ref();
    let pkg_name = package.pkg_name();
    let mut appimage = AppImage::new(None, &file_path, None)?;

    // The extracted icon is named after the app rather than after the AppImage,
    // whose own name is often a generic word the icon theme already claims, so
    // the desktop file has to be extracted and read first.
    let desktop_path = (!has_desktop)
        .then(|| appimage.find_desktop())
        .flatten()
        .filter(|entry| entry.kind == AppImageEntryKind::File)
        .map(|entry| {
            let dest = format!("{}/{}.desktop", install_dir.display(), pkg_name);
            let _ = appimage.write_entry(&entry, &dest);
            dest
        });

    let desktop_icon_name = desktop_path
        .as_ref()
        .and_then(|dest| fs::read_to_string(dest).ok())
        .and_then(|content| desktop_entry_name(&content).map(str::to_string))
        .map_or_else(
            || pkg_name.to_string(),
            |name| managed_icon_name(&name, pkg_name),
        );

    let mut icon_name = pkg_icon_name.map(str::to_string);
    if !has_icon {
        if let Some(entry) = appimage.find_icon() {
            if entry.kind == AppImageEntryKind::File {
                let dest = format!("{}/{}.DirIcon", install_dir.display(), pkg_name);
                let _ = appimage.write_entry(&entry, &dest);

                let magic_bytes = read_file_signature(&dest, 8)?;
                let ext = if magic_bytes == PNG_MAGIC_BYTES {
                    "png"
                } else {
                    "svg"
                };
                let final_path = format!("{}/{}.{ext}", install_dir.display(), pkg_name);
                fs::rename(&dest, &final_path)
                    .with_context(|| format!("renaming from {dest} to {final_path}"))?;

                symlink_icon_with_mode(final_path, &desktop_icon_name, config.is_system())?;
                icon_name = Some(desktop_icon_name);
            }
        }
    }

    if let Some(dest) = desktop_path {
        symlink_desktop_with_config(dest, package, icon_name.as_deref(), config)?;
    }

    if let Some(entry) = appimage.find_appstream() {
        if entry.kind == AppImageEntryKind::File {
            let file_name = if entry
                .path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .contains("appdata")
            {
                "appdata"
            } else {
                "metainfo"
            };
            let dest = format!("{}/{}.{file_name}.xml", install_dir.display(), pkg_name);
            let _ = appimage.write_entry(&entry, &dest);
        }
    }
    Ok(())
}
