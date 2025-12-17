//! AppImage format handling.

use std::{fs, path::Path};

use soar_utils::fs::read_file_signature;
use squishy::{appimage::AppImage, EntryKind};

use super::{
    common::{symlink_desktop, symlink_icon},
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
    has_desktop: bool,
) -> Result<()> {
    if has_icon && has_desktop {
        return Ok(());
    }

    let install_dir = install_dir.as_ref();
    let pkg_name = package.pkg_name();
    let appimage = AppImage::new(None, &file_path, None)?;
    let squashfs = &appimage.squashfs;

    if !has_icon {
        if let Some(entry) = appimage.find_icon() {
            if let EntryKind::File(basic_file) = entry.kind {
                let dest = format!("{}/{}.DirIcon", install_dir.display(), pkg_name);
                let _ = squashfs.write_file(basic_file, &dest);

                let magic_bytes = read_file_signature(&dest, 8)?;
                let ext = if magic_bytes == PNG_MAGIC_BYTES {
                    "png"
                } else {
                    "svg"
                };
                let final_path = format!("{}/{}.{ext}", install_dir.display(), pkg_name);
                fs::rename(&dest, &final_path)
                    .with_context(|| format!("renaming from {dest} to {final_path}"))?;

                symlink_icon(final_path)?;
            }
        }
    }

    if !has_desktop {
        if let Some(entry) = appimage.find_desktop() {
            if let EntryKind::File(basic_file) = entry.kind {
                let dest = format!("{}/{}.desktop", install_dir.display(), pkg_name);
                let _ = squashfs.write_file(basic_file, &dest);
                symlink_desktop(dest, package)?;
            }
        }
    }

    if let Some(entry) = appimage.find_appstream() {
        if let EntryKind::File(basic_file) = entry.kind {
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
            let _ = squashfs.write_file(basic_file, &dest);
        }
    }
    Ok(())
}
