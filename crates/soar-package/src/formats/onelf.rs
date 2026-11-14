//! onelf format handling.
//!
//! onelf packs a directory into a self-extracting ELF binary with a trailing
//! footer, a zstd-compressed manifest, and a compressed payload section. Desktop
//! and icon resources follow the `.onelf/` convention, which makes them cheap to
//! locate and extract for desktop integration.

use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use onelf_format::{Entry, EntryKind, FOOTER_SIZE, Footer, Manifest};
use soar_utils::path::is_safe_component;

use super::common::{
    desktop_entry_name, managed_icon_name, symlink_desktop_with_config, symlink_icon_with_mode,
};
use crate::{
    error::{ErrorContext, PackageError, Result},
    traits::PackageExt,
};

fn invalid<S: Into<String>>(msg: S) -> PackageError {
    PackageError::Custom(msg.into())
}

/// Largest decompressed manifest accepted. A real one takes a few kilobytes
/// per thousand files.
const MAX_MANIFEST_SIZE: u64 = 64 * 1024 * 1024;

/// Largest resource extracted for desktop integration. Desktop files and
/// icons are kilobytes.
const MAX_RESOURCE_SIZE: u64 = 16 * 1024 * 1024;

/// Checks that `len` bytes at `offset` lie inside a file of `file_size`
/// bytes, so a size read from the file cannot drive an allocation the file
/// itself could not fill.
fn span(offset: u64, len: u64, file_size: u64, what: &str) -> Result<usize> {
    offset
        .checked_add(len)
        .filter(|end| *end <= file_size)
        .and_then(|_| usize::try_from(len).ok())
        .ok_or_else(|| invalid(format!("onelf {what} lies outside the file")))
}

fn file_size(file: &File) -> Result<u64> {
    Ok(file
        .metadata()
        .with_context(|| "reading onelf file metadata".to_string())?
        .len())
}

/// Reads and verifies the footer and manifest from an onelf binary.
fn read_footer_and_manifest(file: &mut File) -> Result<(Footer, Manifest)> {
    let file_size = file_size(file)?;
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

    let compressed_len = span(
        footer.manifest_offset,
        footer.manifest_compressed,
        file_size,
        "manifest",
    )?;
    if footer.manifest_original > MAX_MANIFEST_SIZE {
        return Err(invalid("onelf manifest is too large"));
    }
    file.seek(SeekFrom::Start(footer.manifest_offset))
        .with_context(|| "seeking to onelf manifest".to_string())?;
    let mut manifest_compressed = vec![0u8; compressed_len];
    file.read_exact(&mut manifest_compressed)
        .with_context(|| "reading onelf manifest".to_string())?;

    let manifest_bytes =
        zstd::bulk::decompress(&manifest_compressed, footer.manifest_original as usize)
            .map_err(|e| invalid(format!("onelf manifest decompression failed: {e}")))?;

    let checksum = onelf_format::manifest::manifest_checksum(&manifest_bytes)
        .map_err(|e| invalid(e.to_string()))?;
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
    let len = span(
        footer.dict_offset,
        footer.dict_size.into(),
        file_size(file)?,
        "dictionary",
    )?;
    file.seek(SeekFrom::Start(footer.dict_offset))
        .with_context(|| "seeking to onelf dictionary".to_string())?;
    let mut buf = vec![0u8; len];
    file.read_exact(&mut buf)
        .with_context(|| "reading onelf dictionary".to_string())?;
    Ok(Some(buf))
}

/// Decompresses a single file entry from the payload section.
///
/// The entry's total size is checked against [`MAX_RESOURCE_SIZE`] and every
/// block against the file before anything is allocated, so a malformed or
/// hostile entry cannot claim more memory than it is worth.
fn decompress_entry(
    file: &mut File,
    footer: &Footer,
    entry: &Entry,
    dict: Option<&[u8]>,
) -> Result<Vec<u8>> {
    let file_size = file_size(file)?;
    let total = entry
        .blocks
        .iter()
        .map(|block| {
            if footer.is_stored() {
                block.compressed_size
            } else {
                block.original_size
            }
        })
        .try_fold(0u64, u64::checked_add)
        .filter(|total| *total <= MAX_RESOURCE_SIZE)
        .ok_or_else(|| invalid("onelf resource is too large"))?;

    let mut result = Vec::with_capacity(total as usize);
    for block in &entry.blocks {
        let offset = footer
            .payload_offset
            .checked_add(block.payload_offset)
            .ok_or_else(|| invalid("onelf payload block lies outside the file"))?;
        let compressed_len = span(offset, block.compressed_size, file_size, "payload block")?;
        file.seek(SeekFrom::Start(offset))
            .with_context(|| "seeking to onelf payload block".to_string())?;
        let mut compressed = vec![0u8; compressed_len];
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

/// Resolves the desktop entry for `entrypoint`.
///
/// Only the default entrypoint falls back to the package default, so the
/// other entrypoints do not each repeat it as a menu entry of their own.
fn resolve_desktop(manifest: &Manifest, entrypoint: &str, is_default: bool) -> Option<usize> {
    find_entry_by_path(manifest, &format!(".onelf/desktop/{entrypoint}.desktop")).or_else(|| {
        is_default
            .then(|| find_entry_by_path(manifest, ".onelf/desktop/default.desktop"))
            .flatten()
    })
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
/// This function extracts the icon and desktop files that onelf stores under
/// the `.onelf/` convention and sets up the appropriate symlinks for desktop
/// integration, reusing soar's shared symlink helpers. Every entrypoint with
/// a desktop entry of its own is linked, so a package can ship a hidden CLI
/// entry and a visible GUI one.
///
/// # Arguments
///
/// * `install_dir` - Directory where the package is installed
/// * `file_path` - Path to the onelf binary
/// * `package` - Package metadata
/// * `has_icon` - Whether an icon was already found in the install directory
/// * `pkg_icon_name` - Managed name of the already-linked icon that matches this
///   package, if there is one
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
    pkg_icon_name: Option<&str>,
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
    let default_ep = default_entrypoint_name(&manifest);

    // The default entrypoint's desktop keeps its `{pkg_name}.desktop` name,
    // the name the walk-based flow keys on; the rest are named after their
    // entrypoint. The first entrypoint to claim a stem wins.
    let mut desktops: Vec<(String, String, PathBuf)> = Vec::new();
    let mut seen_stems = std::collections::HashSet::new();
    if !has_desktop {
        for ep in &manifest.entrypoints {
            let ep_name = manifest.get_string(ep.name);
            let is_default = ep_name == default_ep;
            // Any other entrypoint names files in the install directory.
            if !is_default && !is_safe_component(ep_name) {
                continue;
            }
            let Some(idx) = resolve_desktop(&manifest, ep_name, is_default) else {
                continue;
            };
            let stem = if is_default {
                pkg_name.to_string()
            } else {
                ep_name.to_string()
            };
            if !seen_stems.insert(stem.clone()) {
                continue;
            }
            let dest = install_dir.join(format!("{stem}.desktop"));
            let desktop_data =
                decompress_entry(&mut file, &footer, &manifest.entries[idx], dict.as_deref())?;
            fs::write(&dest, &desktop_data)
                .with_context(|| format!("writing desktop file to {}", dest.display()))?;
            desktops.push((stem, ep_name.to_string(), dest));
        }
    }

    for (stem, ep_name, dest) in &desktops {
        // The icon is named after the desktop entry's `Name`, so entries
        // sharing the package default still keep distinct names.
        let content = fs::read_to_string(dest).ok();
        let managed = content
            .as_deref()
            .and_then(desktop_entry_name)
            .map_or_else(|| stem.clone(), |name| managed_icon_name(name, stem));

        // An icon the walk already linked under the package's own stem wins
        // for the default entry; otherwise the entrypoint's own icon, or the
        // package default, is extracted and linked.
        let icon = if stem == pkg_name && has_icon {
            pkg_icon_name.map(str::to_string)
        } else if let Some(idx) = resolve_icon(&manifest, ep_name) {
            let ext = if manifest.entry_path(idx).ends_with(".svg") {
                "svg"
            } else {
                "png"
            };
            let icon_data =
                decompress_entry(&mut file, &footer, &manifest.entries[idx], dict.as_deref())?;
            let icon_dest = install_dir.join(format!("{stem}.{ext}"));
            fs::write(&icon_dest, &icon_data)
                .with_context(|| format!("writing icon to {}", icon_dest.display()))?;
            symlink_icon_with_mode(&icon_dest, &managed, config.is_system())?;
            Some(managed)
        } else {
            None
        };

        symlink_desktop_with_config(install_dir, dest, package, icon.as_deref(), config)?;
    }

    // A package may ship an icon with no desktop entry at all; it is still
    // linked under the package's own name.
    if desktops.is_empty()
        && !has_icon
        && let Some(idx) = resolve_icon(&manifest, &default_ep)
    {
        let ext = if manifest.entry_path(idx).ends_with(".svg") {
            "svg"
        } else {
            "png"
        };
        let icon_data =
            decompress_entry(&mut file, &footer, &manifest.entries[idx], dict.as_deref())?;
        let icon_dest = install_dir.join(format!("{pkg_name}.{ext}"));
        fs::write(&icon_dest, &icon_data)
            .with_context(|| format!("writing icon to {}", icon_dest.display()))?;
        symlink_icon_with_mode(&icon_dest, pkg_name, config.is_system())?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use onelf_format::{Block, Flags, ManifestHeader};

    use super::*;

    const BODY: usize = 64;

    fn footer() -> Footer {
        Footer {
            format_version: 1,
            flags: Flags::from_bits_retain(0),
            manifest_offset: 0,
            manifest_compressed: 8,
            manifest_original: 8,
            payload_offset: 0,
            payload_size: 0,
            dict_offset: 0,
            dict_size: 0,
            manifest_checksum: [0; 4],
        }
    }

    /// A file of `BODY` zero bytes followed by `footer`.
    fn onelf_file(footer: &Footer) -> File {
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(&[0; BODY]).unwrap();
        footer.write_to(&mut file).unwrap();
        file
    }

    fn entry(blocks: Vec<Block>) -> Entry {
        Entry {
            kind: EntryKind::File,
            parent: u32::MAX,
            name: 0,
            mode: 0o644,
            mtime_secs: 0,
            mtime_nsec: 0,
            content_hash: [0; 32],
            blocks,
            symlink_target: 0,
        }
    }

    fn block(payload_offset: u64, compressed_size: u64, original_size: u64) -> Block {
        Block {
            payload_offset,
            compressed_size,
            original_size,
            content_hash: [0; 32],
        }
    }

    fn message(err: PackageError) -> String {
        err.to_string()
    }

    #[test]
    fn a_manifest_reaching_past_the_file_is_refused() {
        for (offset, compressed) in [(0, u64::MAX), (u64::MAX, 1), (BODY as u64, 1 << 40)] {
            let footer = Footer {
                manifest_offset: offset,
                manifest_compressed: compressed,
                ..footer()
            };
            let err = read_footer_and_manifest(&mut onelf_file(&footer)).unwrap_err();
            assert!(
                message(err).contains("outside the file"),
                "{offset} {compressed}"
            );
        }
    }

    #[test]
    fn an_oversized_manifest_is_refused() {
        let footer = Footer {
            manifest_original: 1 << 40,
            ..footer()
        };
        let err = read_footer_and_manifest(&mut onelf_file(&footer)).unwrap_err();
        assert!(message(err).contains("too large"));
    }

    #[test]
    fn a_dictionary_reaching_past_the_file_is_refused() {
        let footer = Footer {
            dict_offset: BODY as u64,
            dict_size: u32::MAX,
            ..footer()
        };
        let err = read_dict(&mut onelf_file(&footer), &footer).unwrap_err();
        assert!(message(err).contains("outside the file"));
    }

    #[test]
    fn a_block_reaching_past_the_file_is_refused() {
        let footer = footer();
        let mut file = onelf_file(&footer);
        for blocks in [vec![block(0, 1 << 40, 16)], vec![block(u64::MAX, 1, 16)]] {
            let err = decompress_entry(&mut file, &footer, &entry(blocks), None).unwrap_err();
            assert!(message(err).contains("outside the file"));
        }
    }

    #[test]
    fn an_oversized_resource_is_refused_before_reading() {
        let footer = footer();
        let mut file = onelf_file(&footer);
        for blocks in [
            vec![block(0, 8, 1 << 40)],
            vec![block(0, 8, u64::MAX), block(0, 8, u64::MAX)],
        ] {
            let err = decompress_entry(&mut file, &footer, &entry(blocks), None).unwrap_err();
            assert!(message(err).contains("too large"));
        }
    }

    /// A manifest as current onelf writes it: version 3, nonzero footer_hash,
    /// checksum taken with that field zeroed. A plain hash of the bytes, the
    /// pre-v3 rule this reader used to apply, would not match.
    #[test]
    fn a_v3_manifest_checksums_its_zeroed_footer_hash() {
        let manifest = Manifest {
            header: ManifestHeader {
                version: 3,
                entry_count: 0,
                string_table_size: 1,
                entrypoint_count: 0,
                default_entrypoint: 0,
                lib_dir_count: 0,
                name_offset: 0,
                package_id: [1; 32],
                prefix_hash: [2; 32],
                footer_hash: [3; 32],
            },
            entrypoints: vec![],
            entries: vec![],
            lib_dir_offsets: vec![],
            string_table: b"\0".to_vec(),
        };
        let raw = manifest.serialize().unwrap();
        let checksum = onelf_format::manifest::manifest_checksum(&raw).unwrap();
        let compressed = zstd::bulk::compress(&raw, 3).unwrap();
        let footer = Footer {
            format_version: 1,
            flags: Flags::from_bits_retain(0),
            manifest_offset: 0,
            manifest_compressed: compressed.len() as u64,
            manifest_original: raw.len() as u64,
            payload_offset: 0,
            payload_size: 0,
            dict_offset: 0,
            dict_size: 0,
            manifest_checksum: checksum,
        };
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(&compressed).unwrap();
        footer.write_to(&mut file).unwrap();

        let (_, parsed) = read_footer_and_manifest(&mut file).unwrap();
        assert_eq!(parsed.header.version, 3);
    }

    /// Only the default entrypoint takes the package's `default.desktop`, so a
    /// package with several entrypoints does not list it once per command.
    #[test]
    fn only_the_default_entrypoint_falls_back_to_the_default_desktop() {
        let mut string_table = b"\0".to_vec();
        let mut entries = Vec::new();
        for path in [
            ".onelf/desktop/default.desktop",
            ".onelf/desktop/gui.desktop",
        ] {
            entries.push(Entry {
                name: string_table.len() as u32,
                ..entry(vec![])
            });
            string_table.extend_from_slice(path.as_bytes());
            string_table.push(0);
        }
        let manifest = Manifest {
            header: ManifestHeader {
                version: 3,
                entry_count: entries.len() as u32,
                string_table_size: string_table.len() as u32,
                entrypoint_count: 0,
                default_entrypoint: 0,
                lib_dir_count: 0,
                name_offset: 0,
                package_id: [0; 32],
                prefix_hash: [0; 32],
                footer_hash: [0; 32],
            },
            entrypoints: vec![],
            entries,
            lib_dir_offsets: vec![],
            string_table,
        };

        assert_eq!(resolve_desktop(&manifest, "cli", true), Some(0));
        assert_eq!(resolve_desktop(&manifest, "web", false), None);
        assert_eq!(resolve_desktop(&manifest, "gui", false), Some(1));
    }
}
