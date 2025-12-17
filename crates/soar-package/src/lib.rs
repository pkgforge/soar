//! Package format handling for the soar package manager.
//!
//! This crate provides functionality for detecting package formats and
//! handling format-specific operations like desktop integration.
//!
//! # Supported Formats
//!
//! - **AppImage**: Self-contained Linux applications with embedded resources
//! - **FlatImage**: Similar to AppImage with different internal structure
//! - **RunImage**: Another AppImage-like format
//! - **Wrappe**: Windows PE wrapper format
//! - **ELF**: Standard Linux executables
//!
//! # Example
//!
//! ```no_run
//! use std::fs::File;
//! use std::io::BufReader;
//! use soar_package::{get_file_type, PackageFormat, PackageError};
//!
//! fn detect_format(path: &str) -> Result<PackageFormat, PackageError> {
//!     let file = File::open(path).map_err(|e| PackageError::IoError {
//!         action: "opening file".to_string(),
//!         source: e,
//!     })?;
//!     let mut reader = BufReader::new(file);
//!     get_file_type(&mut reader)
//! }
//! ```

pub mod error;
pub mod formats;
pub mod traits;

pub use error::{ErrorContext, PackageError, Result};
pub use formats::{
    common::integrate_package, get_file_type, PackageFormat, APPIMAGE_MAGIC_BYTES, ELF_MAGIC_BYTES,
    FLATIMAGE_MAGIC_BYTES, PNG_MAGIC_BYTES, RUNIMAGE_MAGIC_BYTES, SVG_MAGIC_BYTES,
    WRAPPE_MAGIC_BYTES,
};
pub use traits::PackageExt;
