use std::{fs, path::Path};

use squishy::{appimage::AppImage, EntryKind};

use crate::{
    constants::PNG_MAGIC_BYTES, database::models::Package, utils::calc_magic_bytes, SoarResult,
};

use super::common::{symlink_desktop, symlink_icon};

pub async fn integrate_appimage<P: AsRef<Path>>(file_path: P, package: &Package) -> SoarResult<()> {
    let appimage = AppImage::new(None, &file_path, None)?;
    let squashfs = &appimage.squashfs;

    if let Some(entry) = appimage.find_icon() {
        if let EntryKind::File(basic_file) = entry.kind {
            let dest = format!("{}.DirIcon", package.pkg_name);
            let _ = squashfs.write_file(basic_file, &dest);

            let magic_bytes = calc_magic_bytes(&dest, 8)?;
            let ext = if magic_bytes == PNG_MAGIC_BYTES {
                "png"
            } else {
                "svg"
            };
            let final_path = format!("{}.{ext}", package.pkg_name);
            fs::rename(&dest, &final_path)?;

            symlink_icon(final_path, &package.pkg_name).await?;
        }
    }

    if let Some(entry) = appimage.find_desktop() {
        if let EntryKind::File(basic_file) = entry.kind {
            let dest = format!("{}.desktop", package.pkg_name);
            let _ = squashfs.write_file(basic_file, &dest);

            symlink_desktop(dest, &package).await?;
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
            let dest = format!("{}.{file_name}.xml", package.pkg_name);
            let _ = squashfs.write_file(basic_file, &dest);
        }
    }
    Ok(())
}
