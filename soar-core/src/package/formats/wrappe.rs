use std::path::{Path, PathBuf};

use crate::SoarResult;

use super::common::create_portable_link;

pub fn setup_wrappe_portable_dir<P: AsRef<Path>>(
    bin_path: P,
    pkg_name: &str,
    portable: Option<&str>,
) -> SoarResult<()> {
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
