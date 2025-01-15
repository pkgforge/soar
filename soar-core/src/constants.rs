use std::{path::PathBuf, sync::OnceLock};

use include_dir::{include_dir, Dir};

use crate::{config::Config, utils::build_path};

pub fn root_path() -> &'static PathBuf {
    static ROOT_PATH: OnceLock<PathBuf> = OnceLock::new();
    ROOT_PATH.get_or_init(|| build_path(&Config::get().unwrap().soar_root).unwrap())
}

pub fn bin_path() -> &'static PathBuf {
    static BIN_PATH: OnceLock<PathBuf> = OnceLock::new();
    BIN_PATH.get_or_init(|| build_path(&Config::get().unwrap().soar_bin.clone().unwrap()).unwrap())
}

pub fn cache_path() -> &'static PathBuf {
    static CACHE_PATH: OnceLock<PathBuf> = OnceLock::new();
    CACHE_PATH
        .get_or_init(|| build_path(&Config::get().unwrap().soar_cache.clone().unwrap()).unwrap())
}

pub fn db_path() -> &'static PathBuf {
    static DB_PATH: OnceLock<PathBuf> = OnceLock::new();
    DB_PATH.get_or_init(|| build_path(&Config::get().unwrap().soar_db.clone().unwrap()).unwrap())
}

pub fn repositories_path() -> &'static PathBuf {
    static REPOS_PATH: OnceLock<PathBuf> = OnceLock::new();
    REPOS_PATH.get_or_init(|| {
        build_path(&Config::get().unwrap().soar_repositories.clone().unwrap()).unwrap()
    })
}

pub fn packages_path() -> &'static PathBuf {
    static PACKAGES_PATH: OnceLock<PathBuf> = OnceLock::new();
    PACKAGES_PATH
        .get_or_init(|| build_path(&Config::get().unwrap().soar_packages.clone().unwrap()).unwrap())
}

pub const ELF_MAGIC_BYTES: [u8; 4] = [0x7f, 0x45, 0x4c, 0x46];
pub const APPIMAGE_MAGIC_BYTES: [u8; 4] = [0x41, 0x49, 0x02, 0x00];
pub const FLATIMAGE_MAGIC_BYTES: [u8; 4] = [0x46, 0x49, 0x01, 0x00];

pub const PNG_MAGIC_BYTES: [u8; 8] = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
pub const SVG_MAGIC_BYTES: [u8; 4] = [0x3c, 0x73, 0x76, 0x67];
pub const XML_MAGIC_BYTES: [u8; 5] = [0x3c, 0x3f, 0x78, 0x6d, 0x6c];

pub const CAP_SYS_ADMIN: i32 = 21;
pub const CAP_MKNOD: i32 = 27;

pub const METADATA_MIGRATIONS: Dir = include_dir!("$CARGO_MANIFEST_DIR/migrations/metadata");
pub const CORE_MIGRATIONS: Dir = include_dir!("$CARGO_MANIFEST_DIR/migrations/core");
