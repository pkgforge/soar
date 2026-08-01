//! Reading named sections out of an ELF file.
//!
//! An AppImage is an ELF, and the one section soar cares about is `.upd_info`,
//! which names where updates come from. Only 64-bit little-endian files are
//! read, which is every AppImage soar installs; anything else reports no
//! section rather than guessing at its layout.

use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];
const CLASS_64: u8 = 2;
const DATA_LE: u8 = 1;

/// The contents of the named section, with trailing NUL padding removed.
///
/// Returns `None` when the file is not a 64-bit little-endian ELF, has no such
/// section, or cannot be read.
pub fn section_data(path: impl AsRef<Path>, name: &str) -> Option<Vec<u8>> {
    let mut file = File::open(path).ok()?;

    let mut ident = [0u8; 16];
    file.read_exact(&mut ident).ok()?;
    if ident[..4] != ELF_MAGIC || ident[4] != CLASS_64 || ident[5] != DATA_LE {
        return None;
    }

    // Section header table offset, entry size, count, and which entry holds
    // the section names.
    let shoff = read_u64(&mut file, 0x28)?;
    let shentsize = read_u16(&mut file, 0x3A)? as u64;
    let shnum = read_u16(&mut file, 0x3C)? as u64;
    let shstrndx = read_u16(&mut file, 0x3E)? as u64;
    if shoff == 0 || shnum == 0 || shstrndx >= shnum {
        return None;
    }

    let names = {
        let entry = shoff.checked_add(shstrndx.checked_mul(shentsize)?)?;
        let offset = read_u64(&mut file, entry + 0x18)?;
        let size = read_u64(&mut file, entry + 0x20)?;
        read_at(&mut file, offset, size)?
    };

    for i in 0..shnum {
        let entry = shoff.checked_add(i.checked_mul(shentsize)?)?;
        let name_offset = read_u32(&mut file, entry)? as usize;
        if section_name(&names, name_offset)? != name {
            continue;
        }
        let offset = read_u64(&mut file, entry + 0x18)?;
        let size = read_u64(&mut file, entry + 0x20)?;
        let mut data = read_at(&mut file, offset, size)?;
        while data.last() == Some(&0) {
            data.pop();
        }
        return Some(data);
    }
    None
}

/// The NUL-terminated name starting at `offset` in the section name table.
fn section_name(names: &[u8], offset: usize) -> Option<&str> {
    let rest = names.get(offset..)?;
    let end = rest.iter().position(|b| *b == 0).unwrap_or(rest.len());
    std::str::from_utf8(&rest[..end]).ok()
}

/// Guards against a section header claiming a size no file could hold.
const MAX_SECTION: u64 = 1 << 20;

fn read_at(file: &mut File, offset: u64, size: u64) -> Option<Vec<u8>> {
    if size > MAX_SECTION {
        return None;
    }
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut buf = vec![0u8; size as usize];
    file.read_exact(&mut buf).ok()?;
    Some(buf)
}

fn read_u16(file: &mut File, offset: u64) -> Option<u16> {
    let b = read_at(file, offset, 2)?;
    Some(u16::from_le_bytes([b[0], b[1]]))
}

fn read_u32(file: &mut File, offset: u64) -> Option<u32> {
    let b = read_at(file, offset, 4)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn read_u64(file: &mut File, offset: u64) -> Option<u64> {
    let b = read_at(file, offset, 8)?;
    Some(u64::from_le_bytes([
        b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
    ]))
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::tempdir;

    use super::*;

    /// A minimal 64-bit little-endian ELF carrying two sections: the name
    /// table and one named section holding `payload`.
    fn elf_with_section(name: &str, payload: &[u8]) -> Vec<u8> {
        let shentsize: u64 = 64;
        let names = format!("\0{name}\0");
        let header_len: u64 = 64;
        let names_offset = header_len;
        let payload_offset = names_offset + names.len() as u64;
        let shoff = payload_offset + payload.len() as u64;

        let mut out = vec![0u8; header_len as usize];
        out[..4].copy_from_slice(&ELF_MAGIC);
        out[4] = CLASS_64;
        out[5] = DATA_LE;
        out[0x28..0x30].copy_from_slice(&shoff.to_le_bytes());
        out[0x3A..0x3C].copy_from_slice(&(shentsize as u16).to_le_bytes());
        out[0x3C..0x3E].copy_from_slice(&2u16.to_le_bytes());
        out[0x3E..0x40].copy_from_slice(&0u16.to_le_bytes());

        out.extend_from_slice(names.as_bytes());
        out.extend_from_slice(payload);

        let mut entry = |name_offset: u32, offset: u64, size: u64| {
            let mut e = vec![0u8; shentsize as usize];
            e[0..4].copy_from_slice(&name_offset.to_le_bytes());
            e[0x18..0x20].copy_from_slice(&offset.to_le_bytes());
            e[0x20..0x28].copy_from_slice(&size.to_le_bytes());
            out.extend_from_slice(&e);
        };
        entry(0, names_offset, names.len() as u64);
        entry(1, payload_offset, payload.len() as u64);
        out
    }

    fn write(bytes: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("app");
        File::create(&path).unwrap().write_all(bytes).unwrap();
        (dir, path)
    }

    #[test]
    fn reads_a_named_section() {
        let (_dir, path) = write(&elf_with_section(
            ".upd_info",
            b"zsync|https://e.test/a.zsync",
        ));
        assert_eq!(
            section_data(&path, ".upd_info").as_deref(),
            Some(&b"zsync|https://e.test/a.zsync"[..])
        );
    }

    #[test]
    fn strips_the_nul_padding_a_section_is_padded_with() {
        let (_dir, path) = write(&elf_with_section(".upd_info", b"zsync|u\0\0\0\0"));
        assert_eq!(
            section_data(&path, ".upd_info").as_deref(),
            Some(&b"zsync|u"[..])
        );
    }

    #[test]
    fn absent_section_and_non_elf_report_nothing() {
        let (_dir, path) = write(&elf_with_section(".upd_info", b"x"));
        assert_eq!(section_data(&path, ".missing"), None);

        let (_dir, plain) = write(b"#!/bin/sh\necho hi\n");
        assert_eq!(section_data(&plain, ".upd_info"), None);
    }
}
