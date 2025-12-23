//! Constants used throughout soar-core.

/// Magic bytes for XML files.
pub const XML_MAGIC_BYTES: [u8; 5] = [0x3c, 0x3f, 0x78, 0x6d, 0x6c];

/// Linux capability for CAP_SYS_ADMIN.
pub const CAP_SYS_ADMIN: i32 = 21;

/// Linux capability for CAP_MKNOD.
pub const CAP_MKNOD: i32 = 27;

/// Marker file stored in install directory to track partial installs
pub const INSTALL_MARKER_FILE: &str = ".soar_install";
