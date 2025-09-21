use std::{fs, path::Path};

use crate::error::{FileSystemError, FileSystemResult};

pub trait FileSystemProvider {
    /// Removes the specified file or directory safely.
    ///
    /// If the path does not exist, this function returns `Ok(())` without error. If the path
    /// points to a directory, it and all of its contents are removed recursively, equivalent to
    /// [`std::fs::remove_dir_all`]. If the path points to a file, it is removed with
    /// [`std::fs::remove_file`].
    ///
    /// # Errors
    ///
    /// Returns a [`FileSystemError::File`] if the removal fails for any reason other than
    /// the path not existing (e.g., permission denied, path is in use, etc.).
    ///
    /// # Example
    ///
    /// ```no_run
    /// use soar_utils::error::FileSystemResult;
    /// use soar_utils::fs::{FileSystemProvider, StandardFileSystemProvider};
    ///
    /// fn main() -> FileSystemResult<()> {
    ///     let fs = StandardFileSystemProvider;
    ///     // Remove a file or directory, ignoring if it doesn't exist
    ///     fs.safe_remove("/tmp/some_path")?;
    ///     Ok(())
    /// }
    /// ```
    fn safe_remove<P: AsRef<Path>>(&self, path: P) -> FileSystemResult<()>;

    /// Creates a directory structure if it doesn't exist.
    ///
    /// If the directory already exists, this function does nothing. If the directory structure
    /// exists but is not a directory, this function returns an error.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to create.
    ///
    /// # Errors
    ///
    /// * [`FileSystemError::Directory`] if the directory could not be created.
    /// * [`FileSystemError::NotADirectory`] if the path exists but is not a directory.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use soar_utils::error::FileSystemResult;
    /// use soar_utils::fs::{FileSystemProvider, StandardFileSystemProvider};
    ///
    /// fn main() -> FileSystemResult<()> {
    ///     let fs = StandardFileSystemProvider;
    ///     let dir = "/tmp/soar-doc/internal/dir";
    ///     fs.ensure_dir_exists(dir)?;
    ///     Ok(())
    /// }
    /// ```
    fn ensure_dir_exists<P: AsRef<Path>>(&self, path: P) -> FileSystemResult<()>;
}

#[derive(Default, Clone)]
pub struct StandardFileSystemProvider;

impl FileSystemProvider for StandardFileSystemProvider {
    fn safe_remove<P: AsRef<Path>>(&self, path: P) -> FileSystemResult<()> {
        let path = path.as_ref();

        if !path.exists() {
            return Ok(());
        }

        let result = if path.is_dir() {
            fs::remove_dir_all(path)
        } else {
            fs::remove_file(path)
        };

        result.map_err(|err| FileSystemError::File {
            path: path.to_path_buf(),
            action: "remove",
            source: err,
        })
    }

    fn ensure_dir_exists<P: AsRef<Path>>(&self, path: P) -> FileSystemResult<()> {
        let path = path.as_ref();
        if !path.exists() {
            std::fs::create_dir_all(path).map_err(|err| FileSystemError::Directory {
                path: path.to_path_buf(),
                action: "create",
                source: err,
            })?;
        } else if !path.is_dir() {
            return Err(FileSystemError::NotADirectory {
                path: path.to_path_buf(),
            });
        }

        Ok(())
    }
}

/// Creates a directory structure if it doesn't exist.
///
/// This is a convenience function that creates a [`StandardFileSystemProvider`] and calls
/// [`FileSystemProvider::ensure_dir_exists`] on it.
///
/// See [`FileSystemProvider::ensure_dir_exists`] for detailed documentation.
pub fn ensure_dir_exists<P: AsRef<Path>>(path: P) -> FileSystemResult<()> {
    StandardFileSystemProvider.ensure_dir_exists(path)
}

/// Removes the specified file or directory safely.
///
/// This is a convenience function that creates a [`StandardFileSystemProvider`] and calls
/// [`FileSystemProvider::safe_remove`] on it.
///
/// See [`FileSystemProvider::safe_remove`] for detailed documentation.
pub fn safe_remove<P: AsRef<Path>>(path: P) -> FileSystemResult<()> {
    StandardFileSystemProvider.safe_remove(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_safe_remove_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test_file.txt");
        fs::write(&file_path, "hello").unwrap();
        safe_remove(&file_path).unwrap();
        assert!(!file_path.exists());
    }

    #[test]
    fn test_safe_remove_dir() {
        let dir = tempdir().unwrap();
        let sub_dir = dir.path().join("sub");
        fs::create_dir(&sub_dir).unwrap();
        safe_remove(&sub_dir).unwrap();
        assert!(!sub_dir.exists());
    }

    #[test]
    fn test_safe_remove_non_existent() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("non_existent.txt");
        safe_remove(&file_path).unwrap();
    }

    #[test]
    fn test_ensure_dir_exists() {
        let dir = tempdir().unwrap();
        let new_dir = dir.path().join("new_dir");
        ensure_dir_exists(&new_dir).unwrap();
        assert!(new_dir.is_dir());
    }

    #[test]
    fn test_ensure_dir_exists_already_exists() {
        let dir = tempdir().unwrap();
        ensure_dir_exists(dir.path()).unwrap();
        assert!(dir.path().is_dir());
    }

    #[test]
    fn test_ensure_dir_exists_file_collision() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("file.txt");
        fs::write(&file_path, "hello").unwrap();
        assert!(ensure_dir_exists(&file_path).is_err());
    }

    #[test]
    fn test_ensure_dir_exists_permission_denied() {
        let dir = tempdir().unwrap();
        let read_only_dir = dir.path().join("read_only");
        fs::create_dir(&read_only_dir).unwrap();

        // Set read-only permissions on the directory.
        let mut perms = fs::metadata(&read_only_dir).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&read_only_dir, perms).unwrap();

        let new_dir = read_only_dir.join("new_dir");
        let result = ensure_dir_exists(&new_dir);
        assert!(result.is_err());

        // Cleanup: Set back to writable to allow tempdir to be removed.
        let mut perms = fs::metadata(&read_only_dir).unwrap().permissions();
        perms.set_readonly(false);
        fs::set_permissions(&read_only_dir, perms).unwrap();
    }

    #[test]
    fn test_standard_safe_remove_permission_denied() {
        let dir = tempdir().unwrap();
        let sub_dir = dir.path().join("read_only_dir");
        fs::create_dir(&sub_dir).unwrap();
        let file_path = sub_dir.join("file.txt");
        fs::write(&file_path, "content").unwrap();

        // Set read-only permissions on the parent directory.
        let mut perms = fs::metadata(&sub_dir).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&sub_dir, perms).unwrap();

        let result = safe_remove(&file_path);
        assert!(result.is_err());

        // Cleanup: Set back to writable to allow tempdir to be removed.
        let mut perms = fs::metadata(&sub_dir).unwrap().permissions();
        perms.set_readonly(false);
        fs::set_permissions(&sub_dir, perms).unwrap();
    }

    #[test]
    fn test_safe_remove_dir_permission_denied() {
        let dir = tempdir().unwrap();
        let sub_dir = dir.path().join("read_only_dir");
        fs::create_dir(&sub_dir).unwrap();
        let file_path = sub_dir.join("file.txt");
        fs::write(&file_path, "content").unwrap();

        // Set read-only permissions on the parent directory.
        let mut perms = fs::metadata(&sub_dir).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&sub_dir, perms).unwrap();

        let result = safe_remove(&sub_dir);
        assert!(result.is_err());

        // Cleanup: Set back to writable to allow tempdir to be removed.
        let mut perms = fs::metadata(&sub_dir).unwrap().permissions();
        perms.set_readonly(false);
        fs::set_permissions(&sub_dir, perms).unwrap();
    }
}
