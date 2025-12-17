//! Package format detection and handling.
//!
//! This module provides functionality for detecting package formats based on
//! magic bytes and handling format-specific operations like desktop integration.

pub mod appimage;
pub mod common;
pub mod wrappe;

use std::io::{BufReader, Read, Seek, SeekFrom};

use crate::error::{PackageError, Result};

/// Magic bytes for ELF executables.
pub const ELF_MAGIC_BYTES: [u8; 4] = [0x7f, 0x45, 0x4c, 0x46];

/// Magic bytes for AppImage format (at offset 8).
pub const APPIMAGE_MAGIC_BYTES: [u8; 4] = [0x41, 0x49, 0x02, 0x00];

/// Magic bytes for FlatImage format (at offset 8).
pub const FLATIMAGE_MAGIC_BYTES: [u8; 4] = [0x46, 0x49, 0x01, 0x00];

/// Magic bytes for RunImage format (at offset 8).
pub const RUNIMAGE_MAGIC_BYTES: [u8; 4] = [0x52, 0x49, 0x02, 0x00];

/// Magic bytes for Wrappe format (at offset file_size - 801).
pub const WRAPPE_MAGIC_BYTES: [u8; 8] = [0x50, 0x45, 0x33, 0x44, 0x41, 0x54, 0x41, 0x00];

/// Magic bytes for PNG images.
pub const PNG_MAGIC_BYTES: [u8; 8] = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

/// Magic bytes for SVG images.
pub const SVG_MAGIC_BYTES: [u8; 4] = [0x3c, 0x73, 0x76, 0x67];

/// Supported package formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageFormat {
    /// AppImage format - self-contained Linux application.
    AppImage,
    /// FlatImage format.
    FlatImage,
    /// RunImage format.
    RunImage,
    /// Wrappe format - Windows PE wrapper.
    Wrappe,
    /// Standard ELF executable.
    ELF,
    /// Unknown or unsupported format.
    Unknown,
}

/// Detects the package format by reading magic bytes from the file.
///
/// # Arguments
///
/// * `file` - A buffered reader with seek capability
///
/// # Returns
///
/// The detected [`PackageFormat`], or [`PackageFormat::Unknown`] if the format
/// cannot be determined.
///
/// # Errors
///
/// Returns [`PackageError`] if reading or seeking fails.
pub fn get_file_type<T>(file: &mut BufReader<T>) -> Result<PackageFormat>
where
    T: Read + Seek,
{
    let mut magic_bytes = [0u8; 12];
    file.read_exact(&mut magic_bytes)
        .map_err(|_| PackageError::MagicBytesError)?;

    if magic_bytes[8..] == APPIMAGE_MAGIC_BYTES {
        return Ok(PackageFormat::AppImage);
    }
    if magic_bytes[8..] == FLATIMAGE_MAGIC_BYTES {
        return Ok(PackageFormat::FlatImage);
    }
    if magic_bytes[8..] == RUNIMAGE_MAGIC_BYTES {
        return Ok(PackageFormat::RunImage);
    }

    // Check for Wrappe format - magic bytes are at offset (file_size - 801)
    let file_size = file
        .seek(SeekFrom::End(0))
        .map_err(|_| PackageError::SeekError)?;

    // Wrappe magic bytes require at least 801 bytes (offset from end) + 8 bytes (magic)
    if file_size >= 801 {
        let start = file_size - 801;
        file.seek(SeekFrom::Start(start))
            .map_err(|_| PackageError::SeekError)?;

        let mut wrappe_magic = [0u8; 8];
        file.read_exact(&mut wrappe_magic)
            .map_err(|_| PackageError::MagicBytesError)?;

        if wrappe_magic == WRAPPE_MAGIC_BYTES {
            file.rewind().map_err(|_| PackageError::SeekError)?;
            return Ok(PackageFormat::Wrappe);
        }
    }

    file.rewind().map_err(|_| PackageError::SeekError)?;

    if magic_bytes[..4] == ELF_MAGIC_BYTES {
        return Ok(PackageFormat::ELF);
    }

    Ok(PackageFormat::Unknown)
}
