use include_dir::{include_dir, Dir};

pub const XML_MAGIC_BYTES: [u8; 5] = [0x3c, 0x3f, 0x78, 0x6d, 0x6c];

pub const CAP_SYS_ADMIN: i32 = 21;
pub const CAP_MKNOD: i32 = 27;

pub const METADATA_MIGRATIONS: Dir = include_dir!("$CARGO_MANIFEST_DIR/migrations/metadata");
pub const CORE_MIGRATIONS: Dir = include_dir!("$CARGO_MANIFEST_DIR/migrations/core");
pub const NESTS_MIGRATIONS_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/migrations/nests");
