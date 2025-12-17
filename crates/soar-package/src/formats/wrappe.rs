//! Wrappe format handling.

use std::path::{Path, PathBuf};

use super::common::create_portable_link;
use crate::error::Result;

/// Sets up portable directory for a Wrappe package.
///
/// Wrappe packages use a special `.wrappe` extension for their portable
/// data directories.
///
/// # Arguments
///
/// * `bin_path` - Path to the binary
/// * `pkg_name` - Package name
/// * `portable` - Optional portable directory path
///
/// # Errors
///
/// Returns [`PackageError`] if directory creation or symlink fails.
pub fn setup_wrappe_portable_dir<P: AsRef<Path>>(
    bin_path: P,
    pkg_name: &str,
    portable: Option<&str>,
) -> Result<()> {
    let bin_path = bin_path.as_ref();
    let package_path = &bin_path.parent().unwrap();
    let real_path = package_path.join(format!(".{pkg_name}.wrappe"));

    if let Some(portable) = portable {
        if !portable.is_empty() {
            let portable = PathBuf::from(portable);
            create_portable_link(&portable, &real_path, pkg_name, "wrappe")?;
        }
    }

    Ok(())
}
