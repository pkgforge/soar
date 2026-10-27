//! AppImage format handling.

use std::{fs, path::Path};

use soar_utils::fs::read_file_signature;
use squishy::appimage::{AppImage, AppImageEntry, AppImageEntryKind};

use super::{
    PNG_MAGIC_BYTES,
    common::{
        desktop_entry_name, managed_icon_name, symlink_desktop_with_config, symlink_icon_with_mode,
    },
};
use crate::{
    error::{ErrorContext, Result},
    traits::PackageExt,
};

/// Upper bound on the size of a candidate desktop file, used to skip large
/// `.desktop`-suffixed binaries without reading them.
const MAX_DESKTOP_SIZE: u64 = 1 << 20;

fn is_desktop_entry(bytes: &[u8]) -> bool {
    if bytes.starts_with(b"\x7fELF") {
        return false;
    }
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .any(|line| line == "[Desktop Entry]")
}

fn declares_no_display(bytes: &[u8]) -> bool {
    String::from_utf8_lossy(bytes).lines().any(|line| {
        line.trim()
            .strip_prefix("NoDisplay=")
            .is_some_and(|value| value.trim().eq_ignore_ascii_case("true"))
    })
}

/// How canonical a desktop file's location is, lower being preferred.
///
/// An AppImage ships its entry at the root, while `/usr/share/applications/`
/// holds both the app's own entry and helper entries nested in subdirectories
/// such as `kde4/`. An unpacked runtime may carry dozens of those helpers.
fn location_rank(path: &Path) -> u8 {
    const APPS: &str = "/usr/share/applications/";
    let path = path.to_string_lossy().to_ascii_lowercase();
    if let Some(rest) = path.strip_prefix(APPS) {
        if rest.contains('/') { 2 } else { 1 }
    } else if path.trim_start_matches('/').contains('/') {
        3
    } else {
        0
    }
}

fn entry_stem_matches(path: &Path, pkg_name: &str) -> bool {
    path.file_stem()
        .is_some_and(|stem| stem.to_string_lossy().eq_ignore_ascii_case(pkg_name))
}

fn resolve_symlink<'a>(
    entries: &'a [AppImageEntry],
    entry: &'a AppImageEntry,
) -> Option<&'a AppImageEntry> {
    let mut entry = entry;
    for _ in 0..40 {
        match &entry.kind {
            AppImageEntryKind::Symlink(target) => {
                entry = entries.iter().find(|candidate| candidate.path == *target)?;
            }
            _ => return Some(entry),
        }
    }
    None
}

/// Picks the desktop file that belongs to the application.
///
/// `squishy`'s `find_desktop` prefers any path under `/usr/share/applications/`
/// and breaks ties by path order, which lets a helper entry nested in a
/// subdirectory win over the application's own entry. Ranking by location,
/// visibility, and name keeps the canonical one first.
fn find_desktop_entry(appimage: &mut AppImage, pkg_name: &str) -> Option<AppImageEntry> {
    let entries: Vec<AppImageEntry> = appimage.entries().collect();
    let mut candidates: Vec<(&AppImageEntry, bool)> = Vec::new();

    for entry in &entries {
        let Some(entry) = resolve_symlink(&entries, entry) else {
            continue;
        };
        if entry.kind != AppImageEntryKind::File
            || entry.size > MAX_DESKTOP_SIZE
            || !entry
                .path
                .to_string_lossy()
                .to_ascii_lowercase()
                .ends_with(".desktop")
        {
            continue;
        }
        let Ok(bytes) = appimage.read_entry(entry) else {
            continue;
        };
        if !is_desktop_entry(&bytes) {
            continue;
        }
        candidates.push((entry, declares_no_display(&bytes)));
    }

    candidates.sort_by_key(|(entry, no_display)| {
        (
            location_rank(&entry.path),
            *no_display,
            !entry_stem_matches(&entry.path, pkg_name),
            entry.path.to_string_lossy().to_ascii_lowercase(),
        )
    });

    candidates.first().map(|(entry, _)| AppImageEntry {
        path: entry.path.clone(),
        size: entry.size,
        kind: entry.kind.clone(),
    })
}

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
/// Returns [`crate::error::PackageError`] if extraction or symlink creation fails.
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
    let desktop_path = if has_desktop {
        None
    } else if let Some(entry) = find_desktop_entry(&mut appimage, pkg_name) {
        let dest = format!("{}/{}.desktop", install_dir.display(), pkg_name);
        appimage.write_entry(&entry, &dest)?;
        Some(dest)
    } else {
        None
    };

    let desktop_icon_name = desktop_path
        .as_ref()
        .and_then(|dest| fs::read_to_string(dest).ok())
        .and_then(|content| desktop_entry_name(&content).map(str::to_string))
        .map_or_else(
            || pkg_name.to_string(),
            |name| managed_icon_name(&name, pkg_name),
        );

    let mut icon_name = pkg_icon_name.map(str::to_string);
    if !has_icon
        && let Some(entry) = appimage.find_icon()
        && entry.kind == AppImageEntryKind::File
    {
        let dest = format!("{}/{}.DirIcon", install_dir.display(), pkg_name);
        appimage.write_entry(&entry, &dest)?;

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

    if let Some(dest) = desktop_path {
        symlink_desktop_with_config(dest, package, icon_name.as_deref(), config)?;
    }

    if let Some(entry) = appimage.find_appstream()
        && entry.kind == AppImageEntryKind::File
    {
        let file_name = if entry
            .path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().contains("appdata"))
        {
            "appdata"
        } else {
            "metainfo"
        };
        let dest = format!("{}/{}.{file_name}.xml", install_dir.display(), pkg_name);
        appimage.write_entry(&entry, &dest)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{declares_no_display, entry_stem_matches, is_desktop_entry, location_rank};

    #[test]
    fn a_nested_helper_entry_ranks_below_the_canonical_entries() {
        let root = Path::new("/onlyoffice-desktopeditors.desktop");
        let direct = Path::new("/usr/share/applications/onlyoffice-desktopeditors.desktop");
        let nested = Path::new("/usr/share/applications/kde4/keditbookmarks.desktop");
        let elsewhere = Path::new("/usr/share/kde4/services/bookmarks.desktop");

        assert!(location_rank(root) < location_rank(direct));
        assert!(location_rank(direct) < location_rank(nested));
        assert!(location_rank(nested) < location_rank(elsewhere));
    }

    #[test]
    fn an_entry_stem_matches_the_package_name_without_case() {
        assert!(entry_stem_matches(
            Path::new("/usr/share/applications/OnlyOffice.desktop"),
            "onlyoffice"
        ));
        assert!(!entry_stem_matches(
            Path::new("/usr/share/applications/other.desktop"),
            "onlyoffice"
        ));
    }

    #[test]
    fn a_hidden_entry_is_recognized() {
        assert!(declares_no_display(b"[Desktop Entry]\nNoDisplay=true\n"));
        assert!(declares_no_display(b"[Desktop Entry]\nNoDisplay=TRUE\n"));
        assert!(!declares_no_display(b"[Desktop Entry]\nName=Visible\n"));
    }

    #[test]
    fn only_real_desktop_entries_are_accepted() {
        assert!(is_desktop_entry(
            b"[Desktop Entry]\nName=DesktopEditors\n"
        ));
        assert!(!is_desktop_entry(b"\x7fELF some binary calling itself .desktop"));
    }
}
