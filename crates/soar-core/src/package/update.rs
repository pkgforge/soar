use std::{fs, path::Path};

use soar_db::repository::core::CoreRepository;

use crate::{
    database::{connection::DieselDatabase, models::Package},
    error::ErrorContext,
    SoarResult,
};

/// Removes old versions of a package after a successful update.
///
/// This function finds all installed versions of the package (by pkg_id, pkg_name, repo_name)
/// that are older than the current version and removes them from disk and database.
pub fn remove_old_versions(package: &Package, db: &DieselDatabase) -> SoarResult<()> {
    let Package {
        pkg_id,
        pkg_name,
        repo_name,
        ..
    } = package;

    let old_packages =
        db.with_conn(|conn| CoreRepository::get_old_package_paths(conn, pkg_id, pkg_name, repo_name))?;

    for (_id, installed_path) in &old_packages {
        let path = Path::new(installed_path);
        if path.exists() {
            fs::remove_dir_all(path)
                .with_context(|| format!("removing old package directory {}", path.display()))?;
        }
    }

    db.with_conn(|conn| CoreRepository::delete_old_packages(conn, pkg_id, pkg_name, repo_name))?;

    Ok(())
}
