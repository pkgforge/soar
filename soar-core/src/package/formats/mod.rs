use std::io::{BufReader, Read};

use crate::constants::{APPIMAGE_MAGIC_BYTES, ELF_MAGIC_BYTES, FLATIMAGE_MAGIC_BYTES};

pub mod appimage;
pub mod common;

#[derive(PartialEq, Eq)]
pub enum PackageFormat {
    AppImage,
    FlatImage,
    ELF,
    Unknown,
}

pub fn get_file_type<T>(file: &mut BufReader<T>) -> PackageFormat
where
    T: Read,
{
    let mut magic_bytes = [0u8; 12];
    if file.read_exact(&mut magic_bytes).is_ok() {
        if magic_bytes[8..] == APPIMAGE_MAGIC_BYTES {
            return PackageFormat::AppImage;
        } else if magic_bytes[8..] == FLATIMAGE_MAGIC_BYTES {
            return PackageFormat::FlatImage;
        } else if magic_bytes[..4] == ELF_MAGIC_BYTES {
            return PackageFormat::ELF;
        }
    }
    PackageFormat::Unknown
}
