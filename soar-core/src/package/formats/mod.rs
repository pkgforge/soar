use std::io::{BufReader, Read, Seek, SeekFrom};

use crate::{
    constants::{
        APPIMAGE_MAGIC_BYTES, ELF_MAGIC_BYTES, FLATIMAGE_MAGIC_BYTES, RUNIMAGE_MAGIC_BYTES,
        WRAPPE_MAGIC_BYTES,
    },
    error::SoarError,
    SoarResult,
};

pub mod appimage;
pub mod common;
pub mod wrappe;

#[derive(Debug, PartialEq, Eq)]
pub enum PackageFormat {
    AppImage,
    FlatImage,
    RunImage,
    Wrappe,
    ELF,
    Unknown,
}

pub fn get_file_type<T>(file: &mut BufReader<T>) -> SoarResult<PackageFormat>
where
    T: Read + Seek,
{
    let mut magic_bytes = [0u8; 12];
    file.read_exact(&mut magic_bytes)
        .map_err(|_| SoarError::Custom("Error reading magic bytes".into()))?;

    if magic_bytes[8..] == APPIMAGE_MAGIC_BYTES {
        return Ok(PackageFormat::AppImage);
    }
    if magic_bytes[8..] == FLATIMAGE_MAGIC_BYTES {
        return Ok(PackageFormat::FlatImage);
    }
    if magic_bytes[8..] == RUNIMAGE_MAGIC_BYTES {
        return Ok(PackageFormat::RunImage);
    }

    let start = file
        .seek(SeekFrom::End(0))
        .map_err(|_| SoarError::Custom("Error seeking to end of file".into()))?
        .wrapping_sub(801);
    file.rewind()
        .map_err(|_| SoarError::Custom("Error rewinding file".into()))?;

    if file.seek(SeekFrom::Start(start)).is_ok() {
        let mut magic_bytes = [0u8; 8];
        file.read_exact(&mut magic_bytes)
            .map_err(|_| SoarError::Custom("Error reading magic bytes".into()))?;
        file.rewind()
            .map_err(|_| SoarError::Custom("Error rewinding file".into()))?;
        if magic_bytes[0..8] == WRAPPE_MAGIC_BYTES {
            return Ok(PackageFormat::Wrappe);
        }
    }

    if magic_bytes[..4] == ELF_MAGIC_BYTES {
        return Ok(PackageFormat::ELF);
    }

    Ok(PackageFormat::Unknown)
}
