//! onelf format handling.
//!
//! onelf packs a directory into a self-extracting ELF binary with a trailing
//! footer, a zstd-compressed manifest, and a compressed payload section. Desktop
//! and icon resources follow the `.onelf/` convention, which makes them cheap to
//! locate and extract for desktop integration.

use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::Path,
};

use onelf_format::{Entry, EntryKind, Footer, Manifest, FOOTER_SIZE};

use super::common::{symlink_desktop_with_config, symlink_icon_with_mode};
use crate::{
    error::{ErrorContext, PackageError, Result},
    traits::PackageExt,
};

fn invalid<S: Into<String>>(msg: S) -> PackageError {
    PackageError::Custom(msg.into())
}

/// Reads and verifies the footer and manifest from an onelf binary.
fn read_footer_and_manifest(file: &mut File) -> Result<(Footer, Manifest)> {
    let file_size = file
        .metadata()
        .with_context(|| "reading onelf file metadata".to_string())?
        .len();
    if file_size < FOOTER_SIZE as u64 {
        return Err(invalid("file too small for onelf footer"));
    }

    file.seek(SeekFrom::End(-(FOOTER_SIZE as i64)))
        .with_context(|| "seeking to onelf footer".to_string())?;
    let mut footer_buf = [0u8; FOOTER_SIZE];
    file.read_exact(&mut footer_buf)
        .with_context(|| "reading onelf footer".to_string())?;
    let footer = Footer::from_bytes(&footer_buf)
        .map_err(|e| invalid(format!("invalid onelf footer: {e}")))?;

    file.seek(SeekFrom::Start(footer.manifest_offset))
        .with_context(|| "seeking to onelf manifest".to_string())?;
    let mut manifest_compressed = vec![0u8; footer.manifest_compressed as usize];
    file.read_exact(&mut manifest_compressed)
        .with_context(|| "reading onelf manifest".to_string())?;

    let manifest_bytes =
        zstd::bulk::decompress(&manifest_compressed, footer.manifest_original as usize)
            .map_err(|e| invalid(format!("onelf manifest decompression failed: {e}")))?;

    let checksum = xxhash_rust::xxh32::xxh32(&manifest_bytes, 0).to_le_bytes();
    if checksum != footer.manifest_checksum {
        return Err(invalid("onelf manifest checksum mismatch"));
    }

    let manifest = Manifest::deserialize(&manifest_bytes)
        .map_err(|e| invalid(format!("invalid onelf manifest: {e}")))?;
    Ok((footer, manifest))
}

/// Reads the optional zstd dictionary described by `footer`.
fn read_dict(file: &mut File, footer: &Footer) -> Result<Option<Vec<u8>>> {
    if footer.dict_size == 0 {
        return Ok(None);
    }
    file.seek(SeekFrom::Start(footer.dict_offset))
        .with_context(|| "seeking to onelf dictionary".to_string())?;
    let mut buf = vec![0u8; footer.dict_size as usize];
    file.read_exact(&mut buf)
        .with_context(|| "reading onelf dictionary".to_string())?;
    Ok(Some(buf))
}

/// Decompresses a single file entry from the payload section.
///
/// Every block is capped at its recorded `original_size`, so a malformed or
/// hostile block cannot expand without bound.
fn decompress_entry(
    file: &mut File,
    footer: &Footer,
    entry: &Entry,
    dict: Option<&[u8]>,
) -> Result<Vec<u8>> {
    let mut result = Vec::new();
    for block in &entry.blocks {
        file.seek(SeekFrom::Start(
            footer.payload_offset + block.payload_offset,
        ))
        .with_context(|| "seeking to onelf payload block".to_string())?;
        let mut compressed = vec![0u8; block.compressed_size as usize];
        file.read_exact(&mut compressed)
            .with_context(|| "reading onelf payload block".to_string())?;

        if footer.is_stored() {
            result.extend_from_slice(&compressed);
            continue;
        }

        let cap = block.original_size as usize;
        let decompressed = if let Some(d) = dict {
            let mut dec = zstd::bulk::Decompressor::with_dictionary(d)
                .with_context(|| "initializing onelf decompressor".to_string())?;
            dec.decompress(&compressed, cap)
                .map_err(|e| invalid(format!("onelf block decompression failed: {e}")))?
        } else {
            zstd::bulk::decompress(&compressed, cap)
                .map_err(|e| invalid(format!("onelf block decompression failed: {e}")))?
        };
        if decompressed.len() != cap {
            return Err(invalid("onelf block size mismatch"));
        }
        result.extend_from_slice(&decompressed);
    }
    Ok(result)
}

/// Finds the entry index of a file at the given package-relative path.
fn find_entry_by_path(manifest: &Manifest, path: &str) -> Option<usize> {
    manifest
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.kind == EntryKind::File)
        .find(|(i, _)| manifest.entry_path(*i) == path)
        .map(|(i, _)| i)
}

/// Resolves the icon entry for `entrypoint`, falling back to the package default.
fn resolve_icon(manifest: &Manifest, entrypoint: &str) -> Option<usize> {
    let candidates = [
        format!(".onelf/icons/{entrypoint}.svg"),
        format!(".onelf/icons/{entrypoint}.png"),
        ".onelf/icons/default.svg".to_string(),
        ".onelf/icons/default.png".to_string(),
    ];
    candidates
        .iter()
        .find_map(|path| find_entry_by_path(manifest, path))
}

/// Resolves the desktop entry for `entrypoint`, falling back to the package default.
fn resolve_desktop(manifest: &Manifest, entrypoint: &str) -> Option<usize> {
    let candidates = [
        format!(".onelf/desktop/{entrypoint}.desktop"),
        ".onelf/desktop/default.desktop".to_string(),
    ];
    candidates
        .iter()
        .find_map(|path| find_entry_by_path(manifest, path))
}

/// Returns the name of the manifest's default entrypoint, or an empty string.
fn default_entrypoint_name(manifest: &Manifest) -> String {
    manifest
        .entrypoints
        .get(manifest.header.default_entrypoint as usize)
        .map(|ep| manifest.get_string(ep.name).to_string())
        .unwrap_or_default()
}

/// Integrates an onelf package by extracting its embedded resources.
///
/// This function extracts the icon and desktop file that onelf stores under the
/// `.onelf/` convention and sets up the appropriate symlinks for desktop
/// integration, reusing soar's shared symlink helpers.
///
/// # Arguments
///
/// * `install_dir` - Directory where the package is installed
/// * `file_path` - Path to the onelf binary
/// * `package` - Package metadata
/// * `has_icon` - Whether an icon was already found in the install directory
/// * `has_desktop` - Whether a desktop file was already found
///
/// # Errors
///
/// Returns [`PackageError`] if extraction or symlink creation fails.
pub async fn integrate_onelf<P: AsRef<Path>, T: PackageExt>(
    install_dir: P,
    file_path: P,
    package: &T,
    has_icon: bool,
    has_desktop: bool,
    config: &soar_config::config::Config,
) -> Result<()> {
    if has_icon && has_desktop {
        return Ok(());
    }

    let install_dir = install_dir.as_ref();
    let pkg_name = package.pkg_name();
    let file_path = file_path.as_ref();

    let mut file =
        File::open(file_path).with_context(|| format!("opening {}", file_path.display()))?;
    let (footer, manifest) = read_footer_and_manifest(&mut file)?;
    let dict = read_dict(&mut file, &footer)?;
    let ep_name = default_entrypoint_name(&manifest);

    // Both the extracted icon and desktop file are named after `pkg_name`, so
    // their stems match and the desktop file can reference the soar-managed icon.
    let mut icon_available = has_icon;
    if !has_icon {
        if let Some(idx) = resolve_icon(&manifest, &ep_name) {
            let ext = if manifest.entry_path(idx).ends_with(".svg") {
                "svg"
            } else {
                "png"
            };
            let icon_data =
                decompress_entry(&mut file, &footer, &manifest.entries[idx], dict.as_deref())?;
            let dest = install_dir.join(format!("{pkg_name}.{ext}"));
            fs::write(&dest, &icon_data)
                .with_context(|| format!("writing icon to {}", dest.display()))?;
            symlink_icon_with_mode(&dest, config.is_system())?;
            icon_available = true;
        }
    }

    if !has_desktop {
        if let Some(idx) = resolve_desktop(&manifest, &ep_name) {
            let desktop_data =
                decompress_entry(&mut file, &footer, &manifest.entries[idx], dict.as_deref())?;
            let dest = install_dir.join(format!("{pkg_name}.desktop"));
            fs::write(&dest, &desktop_data)
                .with_context(|| format!("writing desktop file to {}", dest.display()))?;
            symlink_desktop_with_config(&dest, package, icon_available, config)?;
        }
    }

    Ok(())
}
