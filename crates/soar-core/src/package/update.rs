use std::{
    fs,
    path::{Path, PathBuf},
};

use soar_config::config::Config;
use soar_db::repository::core::CoreRepository;
use tracing::warn;

use super::remove::path_inside_packages;
use crate::{
    database::{connection::DieselDatabase, models::Package},
    error::ErrorContext,
    SoarResult,
};

/// What `partition_old_versions` decides: `(clear, keep)`, where `clear`
/// pairs each record id with its directory and `keep` holds the refused paths
/// for the warning.
type PartitionedRows = (Vec<(i32, PathBuf)>, Vec<String>);

/// Splits old-version rows into records to clear and paths to keep.
///
/// Pure so the policy is testable without a database: `packages_root` maps a
/// row's profile to its packages directory. A missing directory needs no
/// filesystem work but its record still goes; a row outside its own tree is
/// kept with its record. A resolver failure aborts the whole cleanup rather
/// than guessing roots.
fn partition_old_versions(
    rows: &[(i32, String, String)],
    packages_root: &dyn Fn(&str) -> SoarResult<PathBuf>,
) -> SoarResult<PartitionedRows> {
    let mut clear = Vec::new();
    let mut keep = Vec::new();
    for (id, installed_path, profile) in rows {
        let path = Path::new(installed_path);
        if !path.exists() {
            clear.push((*id, PathBuf::from(installed_path)));
            continue;
        }
        let root = packages_root(profile)?;
        if path_inside_packages(path, &root) {
            clear.push((*id, PathBuf::from(installed_path)));
        } else {
            keep.push(installed_path.clone());
        }
    }
    Ok((clear, keep))
}

/// Removes old versions of a package after a successful update.
///
/// This function finds all installed versions of the package (by pkg_id, pkg_name, repo_name)
/// that are older than the current version and removes them from disk and database.
/// If `force` is true, removes pinned packages too. Otherwise only unpinned packages.
///
/// A row pointing outside its own tree is never chmodded, never deleted, and
/// keeps its record: it is skipped with a warning on every run until somebody
/// looks at it.
pub fn remove_old_versions(
    package: &Package,
    db: &DieselDatabase,
    force: bool,
    config: &Config,
) -> SoarResult<()> {
    let Package {
        pkg_id,
        pkg_family,
        pkg_name,
        repo_name,
        ..
    } = package;

    let old_packages = db.with_conn(|conn| {
        CoreRepository::get_old_package_paths(
            conn,
            pkg_id.as_deref(),
            pkg_family.as_deref(),
            pkg_name,
            repo_name,
            force,
        )
    })?;

    let (clear, keep) = partition_old_versions(&old_packages, &|profile| -> SoarResult<PathBuf> {
        Ok(config.get_packages_path(Some(profile.to_string()))?)
    })?;
    for installed_path in &keep {
        warn!(
            installed_path = installed_path,
            "refusing to remove package directory outside the packages tree; record kept"
        );
    }

    // Rows cleared from disk, in order. A filesystem failure still aborts
    // before any record is deleted, the way it always has.
    let mut cleared = Vec::new();
    for (id, installed_path) in &clear {
        if installed_path.exists() {
            // An archive may ship its directories read-only, and removing an
            // entry needs write permission on the directory holding it.
            crate::package::remove::make_tree_writable(installed_path);
            fs::remove_dir_all(installed_path).with_context(|| {
                format!(
                    "removing old package directory {}",
                    installed_path.display()
                )
            })?;
        }
        cleared.push(*id);
    }

    db.transaction(|conn| {
        for id in &cleared {
            CoreRepository::delete(conn, *id)?;
        }
        Ok::<_, diesel::result::Error>(())
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::partition_old_versions;
    use crate::SoarResult;

    fn roots(
        root1: &std::path::Path,
        root2: &std::path::Path,
    ) -> impl Fn(&str) -> SoarResult<std::path::PathBuf> {
        let root1 = root1.to_path_buf();
        let root2 = root2.to_path_buf();
        move |profile| {
            Ok(match profile {
                "p1" => root1.clone(),
                "p2" => root2.clone(),
                _ => return Err(crate::error::SoarError::Custom("unknown profile".into())),
            })
        }
    }

    #[test]
    fn contained_rows_clear_missing_rows_clear_and_outside_rows_keep() {
        let tmp = tempdir().unwrap();
        let root1 = tmp.path().join("root1");
        let root2 = tmp.path().join("root2");
        fs::create_dir_all(&root1).unwrap();
        fs::create_dir_all(&root2).unwrap();
        let inside = root1.join("foo-1.0-abc");
        fs::create_dir_all(&inside).unwrap();
        let outside = tmp.path().join("outside");
        fs::create_dir_all(&outside).unwrap();

        let rows = vec![
            (1, inside.to_string_lossy().into_owned(), "p1".to_string()),
            (
                2,
                root1.join("gone-1.0-abc").to_string_lossy().into_owned(),
                "p1".to_string(),
            ),
            (3, outside.to_string_lossy().into_owned(), "p1".to_string()),
        ];
        let (clear, keep) = partition_old_versions(&rows, &roots(&root1, &root2)).unwrap();

        assert_eq!(clear.len(), 2);
        assert!(clear.iter().any(|(id, _)| *id == 1));
        assert!(clear.iter().any(|(id, _)| *id == 2));
        assert_eq!(keep.len(), 1);
        assert!(keep[0].ends_with("outside"));
    }

    #[test]
    fn each_row_is_measured_against_its_own_profile_root() {
        let tmp = tempdir().unwrap();
        let root1 = tmp.path().join("root1");
        let root2 = tmp.path().join("root2");
        fs::create_dir_all(&root1).unwrap();
        fs::create_dir_all(&root2).unwrap();
        // Inside root1 but outside root2: the row's own profile decides.
        let dir = root1.join("foo-1.0-abc");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.to_string_lossy().into_owned();

        let rows = vec![(1, path.clone(), "p2".to_string())];
        let (clear, keep) = partition_old_versions(&rows, &roots(&root1, &root2)).unwrap();
        assert!(clear.is_empty());
        assert_eq!(keep, vec![path.clone()]);

        let rows = vec![(1, path, "p1".to_string())];
        let (clear, keep) = partition_old_versions(&rows, &roots(&root1, &root2)).unwrap();
        assert_eq!(clear.len(), 1);
        assert!(keep.is_empty());
    }

    #[test]
    fn a_resolver_failure_aborts_instead_of_guessing() {
        let tmp = tempdir().unwrap();
        let dir = tmp.path().join("foo-1.0-abc");
        fs::create_dir_all(&dir).unwrap();
        // Existing, so the resolver runs: an unknown profile must fail the
        // whole cleanup rather than clear against a guessed root.
        let rows = vec![(1, dir.to_string_lossy().into_owned(), "nope".to_string())];
        let root = std::path::PathBuf::from("/tmp/root");
        assert!(partition_old_versions(&rows, &roots(&root, &root)).is_err());
    }
}
