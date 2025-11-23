use std::{
    fs::{self, File},
    io::{BufReader, Read},
    os,
    path::Path,
};

use crate::error::{FileSystemError, FileSystemResult, IoOperation, IoResultExt};

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
/// use soar_utils::fs::safe_remove;
///
/// fn main() -> FileSystemResult<()> {
///     safe_remove("/tmp/some_path")?;
///     Ok(())
/// }
/// ```
pub fn safe_remove<P: AsRef<Path>>(path: P) -> FileSystemResult<()> {
    let path = path.as_ref();

    if !path.exists() {
        return Ok(());
    }

    let result = if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };

    result.with_path(path, IoOperation::RemoveFile)?;

    Ok(())
}

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
/// use soar_utils::fs::ensure_dir_exists;
///
/// fn main() -> FileSystemResult<()> {
///     let dir = "/tmp/soar-doc/internal/dir";
///     ensure_dir_exists(dir)?;
///     Ok(())
/// }
/// ```
pub fn ensure_dir_exists<P: AsRef<Path>>(path: P) -> FileSystemResult<()> {
    let path = path.as_ref();
    if !path.exists() {
        std::fs::create_dir_all(path).with_path(path, IoOperation::CreateDirectory)?;
    } else if !path.is_dir() {
        return Err(FileSystemError::NotADirectory {
            path: path.to_path_buf(),
        });
    }

    Ok(())
}

/// Creates symlink from `source` to `target`
/// If `target` is a file, it will be removed before creating the symlink.
///
/// # Arguments
///
/// * `source` - The path to the file or directory to symlink
/// * `target` - The path to the symlink
///
/// # Errors
///
/// Returns a [`FileSystemError::Symlink`] if the symlink could not be created.
/// Returns a [`FileSystemError::File`] if the symlink could not be removed.
///
/// # Example
///
/// ```no_run
/// use soar_utils::error::FileSystemResult;
/// use soar_utils::fs::create_symlink;
///
/// fn main() -> FileSystemResult<()> {
///     create_symlink("/tmp/source", "/tmp/target")?;
///     Ok(())
/// }
/// ```
pub fn create_symlink<P: AsRef<Path>, Q: AsRef<Path>>(
    source: P,
    target: Q,
) -> FileSystemResult<()> {
    let source = source.as_ref();
    let target = target.as_ref();

    if let Some(parent) = target.parent() {
        ensure_dir_exists(parent)?;
    }

    if target.is_file() {
        fs::remove_file(target).with_path(target, IoOperation::RemoveFile)?;
    }

    os::unix::fs::symlink(source, target).with_path(
        source,
        IoOperation::CreateSymlink {
            target: target.into(),
        },
    )
}

/// Walks a directory recursively and calls the provided function on each file or directory.
///
/// # Arguments
///
/// * `dir` - The directory to walk
/// * `action` - The function to call on each file or directory
///
/// # Errors
///
/// Returns a [`FileSystemError::Directory`] if the directory could not be read.
/// Returns a [`FileSystemError::NotADirectory`] if the path is not a directory.
///
/// # Example
///
/// ```no_run
/// use std::path::Path;
///
/// use soar_utils::error::FileSystemResult;
/// use soar_utils::fs::walk_dir;
///
/// fn main() -> FileSystemResult<()> {
///     let _ = walk_dir("/tmp/dir", &mut |path: &Path| -> FileSystemResult<()> {
///         println!("Found file or directory: {}", path.display());
///         Ok(())
///     })?;
///     Ok(())
/// }
/// ```
pub fn walk_dir<P, F, E>(dir: P, action: &mut F) -> Result<(), E>
where
    P: AsRef<Path>,
    F: FnMut(&Path) -> Result<(), E>,
    FileSystemError: Into<E>,
{
    let dir = dir.as_ref();

    if !dir.is_dir() {
        return Err(FileSystemError::NotADirectory {
            path: dir.to_path_buf(),
        }
        .into());
    }

    for entry in fs::read_dir(dir)
        .with_path(dir, IoOperation::ReadDirectory)
        .map_err(|e| e.into())?
    {
        let Ok(entry) = entry else {
            continue;
        };

        let path = entry.path();

        if path.is_dir() {
            walk_dir(&path, action)?;
            continue;
        }

        action(&path)?;
    }

    Ok(())
}

/// Reads the first `bytes` bytes from a file and returns the signature.
///
/// # Arguments
/// * `path` - The path to the file
/// * `bytes` - The number of bytes to read from the file
///
/// # Returns
/// Returns a byte array of the first `bytes` bytes from the file.
///
/// # Errors
/// Returns a [`FileSystemError::File`] if the file could not be opened or read.
///
/// # Example
/// ```no_run
/// use soar_utils::fs::read_file_signature;
/// use soar_utils::error::FileSystemResult;
///
/// fn main() -> FileSystemResult<()> {
///     let signature = read_file_signature("/tmp/file", 1024)?;
///     println!("File signature: {:?}", signature);
///     Ok(())
/// }
pub fn read_file_signature<P: AsRef<Path>>(path: P, bytes: usize) -> FileSystemResult<Vec<u8>> {
    let path = path.as_ref();
    let file = File::open(path).with_path(path, IoOperation::ReadFile)?;

    let mut reader = BufReader::new(file);
    let mut buffer = vec![0u8; bytes];
    reader
        .read_exact(&mut buffer)
        .with_path(path, IoOperation::ReadFile)?;
    Ok(buffer)
}

/// Calculate the total size in bytes of a directory and all files contained within it.
///
/// Skips entries whose directory entry or metadata cannot be read. Recurses into subdirectories
/// and accumulates file sizes.
///
/// # Returns
///
/// The total size in bytes of the directory and its contents.
///
/// # Errors
///
/// Returns a [`FileSystemError::Directory`] if the directory itself cannot be read.
///
/// # Examples
///
/// ```
/// use soar_utils::fs::dir_size;
///
/// let size = dir_size("/tmp/dir").unwrap_or(0);
/// println!("Directory size: {}", size);
/// ```
pub fn dir_size<P: AsRef<Path>>(path: P) -> FileSystemResult<u64> {
    let path = path.as_ref();
    let mut total_size = 0;

    for entry in fs::read_dir(path).with_path(path, IoOperation::ReadDirectory)? {
        let Ok(entry) = entry else {
            continue;
        };

        let Ok(metadata) = entry.metadata() else {
            continue;
        };

        if metadata.is_file() {
            total_size += metadata.len();
        } else if metadata.is_dir() {
            total_size += dir_size(entry.path())?;
        }
    }

    Ok(total_size)
}

/// Determine whether the file at the given path is an ELF binary.
///
/// Checks the file's first four bytes for the ELF magic sequence (0x7F, 'E', 'L', 'F') and
/// returns `true` if they match, `false` otherwise.
///
/// # Examples
///
/// ```
/// use std::fs::File;
/// use std::io::Write;
/// use tempfile::tempdir;
/// use soar_utils::fs::is_elf;
///
/// let dir = tempdir().unwrap();
/// let path = dir.path().join("example_elf");
/// let mut f = File::create(&path).unwrap();
/// f.write_all(&[0x7f, b'E', b'L', b'F', 0x00]).unwrap();
///
/// assert!(is_elf(&path));
/// ```
pub fn is_elf<P: AsRef<Path>>(path: P) -> bool {
    read_file_signature(path, 4)
        .ok()
        .map(|magic| magic == [0x7f, 0x45, 0x4c, 0x46])
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use std::{fs::Permissions, os::unix::fs::PermissionsExt};

    use tempfile::tempdir;

    use super::*;

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

    #[test]
    fn test_create_symlink() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("source");
        let target = dir.path().join("target");
        fs::write(&source, "content").unwrap();
        create_symlink(&source, &target).unwrap();
        assert!(target.is_symlink());
        assert_eq!(fs::read_link(&target).unwrap(), source);
    }

    #[test]
    fn test_create_symlink_already_exists() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("source");
        let target = dir.path().join("target");
        fs::write(&source, "content").unwrap();
        fs::write(&target, "content").unwrap();
        create_symlink(&source, &target).unwrap();
        assert!(target.is_symlink());
        assert_eq!(fs::read_link(&target).unwrap(), source);
    }

    #[test]
    fn test_create_symlink_permission_denied() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("source");
        let target = dir.path().join("target");
        fs::write(&source, "content").unwrap();

        // Set read-only permissions on the parent directory.
        let mut perms = fs::metadata(dir.path()).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(dir.path(), perms).unwrap();

        let result = create_symlink(&source, &target);
        assert!(result.is_err());

        // Cleanup: Set back to writable to allow tempdir to be removed.
        let mut perms = fs::metadata(dir.path()).unwrap().permissions();
        perms.set_readonly(false);
        fs::set_permissions(dir.path(), perms).unwrap();
    }

    #[test]
    fn test_walk_dir() {
        let tempdir = tempfile::tempdir().unwrap();
        let dir = tempdir.path().join("dir");
        fs::create_dir(&dir).unwrap();
        let file = dir.join("file");
        fs::File::create(&file).unwrap();

        let mut results = Vec::new();
        walk_dir(&dir, &mut |path| -> FileSystemResult<()> {
            results.push(path.to_path_buf());
            Ok(())
        })
        .unwrap();

        assert_eq!(results, vec![file]);
    }

    #[test]
    fn test_walk_dir_not_a_dir() {
        let tempdir = tempfile::tempdir().unwrap();
        let file = tempdir.path().join("file");
        fs::File::create(&file).unwrap();

        let result = walk_dir(&file, &mut |_| -> FileSystemResult<()> { Ok(()) });
        assert!(result.is_err());
    }

    #[test]
    fn test_walk_recursive_dir() {
        let tempdir = tempfile::tempdir().unwrap();
        let dir = tempdir.path().join("dir");
        fs::create_dir(&dir).unwrap();
        let file = dir.join("file");
        File::create(&file).unwrap();

        let nested_dir = dir.join("nested");
        fs::create_dir(&nested_dir).unwrap();
        let nested_file = nested_dir.join("file");
        File::create(&nested_file).unwrap();

        let mut results = Vec::new();
        walk_dir(&dir, &mut |path| -> FileSystemResult<()> {
            results.push(path.to_path_buf());
            Ok(())
        })
        .unwrap();

        assert_eq!(results, vec![file, nested_file]);
    }

    #[test]
    fn test_walk_failing_entry() {
        let tempdir = tempfile::tempdir().unwrap();
        let dir = tempdir.path().join("dir");
        fs::create_dir(&dir).unwrap();
        let file = dir.join("file");
        File::create(&file).unwrap();

        let mut results = Vec::new();
        walk_dir(&dir, &mut |path| {
            results.push(path.to_path_buf());
            Err(FileSystemError::ReadFile {
                path: path.to_path_buf(),
                source: std::io::Error::from(std::io::ErrorKind::Other),
            })
        })
        .ok();

        assert_eq!(results, vec![file]);
    }

    #[test]
    fn test_walk_invalid_dir() {
        let result = walk_dir("/this/path/does/not/exist", &mut |_| -> FileSystemResult<
            (),
        > { Ok(()) });
        assert!(result.is_err());
    }

    #[test]
    fn test_walk_dir_permission_denied() {
        let tempdir = tempfile::tempdir().unwrap();
        let dir = tempdir.path();

        fs::set_permissions(dir, Permissions::from_mode(0o000)).unwrap();

        let result = walk_dir(dir, &mut |_| -> FileSystemResult<()> { Ok(()) });

        fs::set_permissions(dir, Permissions::from_mode(0o755)).unwrap();
        assert!(result.is_err());
    }

    #[test]
    fn test_walk_dir_permission_denied_recursive() {
        let tempdir = tempfile::tempdir().unwrap();
        let dir = tempdir.path();
        let nested_dir = dir.join("nested");
        fs::create_dir(&nested_dir).unwrap();

        fs::set_permissions(&nested_dir, Permissions::from_mode(0o000)).unwrap();

        let result = walk_dir(dir, &mut |_| -> FileSystemResult<()> { Ok(()) });

        fs::set_permissions(nested_dir, Permissions::from_mode(0o755)).unwrap();
        assert!(result.is_err());
    }

    #[test]
    fn test_read_file_signature() {
        let tempdir = tempfile::tempdir().unwrap();
        let file = tempdir.path().join("file");
        File::create(&file).unwrap();
        fs::write(&file, b"sample test content").unwrap();

        let signature = read_file_signature(&file, 8).unwrap();
        assert_eq!(signature.len(), 8);
        assert_eq!(signature, b"sample t");
    }

    #[test]
    fn test_read_file_signature_empty() {
        let tempdir = tempfile::tempdir().unwrap();
        let file = tempdir.path().join("file");
        File::create(&file).unwrap();

        let signature = read_file_signature(&file, 0).unwrap();
        assert!(signature.is_empty());
    }

    #[test]
    fn test_read_file_signature_invalid() {
        let tempdir = tempfile::tempdir().unwrap();
        let file = tempdir.path().join("file");
        File::create(&file).unwrap();

        let result = read_file_signature(&file, 1024);
        assert!(result.is_err());
    }

    #[test]
    fn test_read_file_signature_non_existent() {
        let result = read_file_signature("/this/path/does/not/exist", 1024);
        assert!(result.is_err());
    }

    #[test]
    fn test_calculate_directory_size() {
        let tempdir = tempfile::tempdir().unwrap();
        let dir = tempdir.path().join("dir");
        fs::create_dir(&dir).unwrap();

        let file = dir.join("file");
        File::create(&file).unwrap();
        fs::write(&file, b"sample test content").unwrap(); // 19 bytes

        let nested_dir = dir.join("nested");
        fs::create_dir(&nested_dir).unwrap();

        let nested_file = nested_dir.join("file");
        File::create(&nested_file).unwrap();
        fs::write(&nested_file, b"sample test content").unwrap();

        let size = dir_size(&dir).unwrap();
        assert_eq!(size, 38);
    }

    #[test]
    fn test_calculate_directory_size_empty() {
        let tempdir = tempfile::tempdir().unwrap();
        let dir = tempdir.path().join("dir");
        fs::create_dir(&dir).unwrap();

        let size = dir_size(&dir).unwrap();
        assert_eq!(size, 0);
    }

    #[test]
    fn test_calculate_directory_size_invalid() {
        let result = dir_size("/this/path/does/not/exist");
        assert!(result.is_err());
    }

    #[test]
    fn test_calculate_directory_size_inner_permission_denied() {
        let tempdir = tempfile::tempdir().unwrap();
        let dir = tempdir.path();
        let inner_dir = dir.join("inner");
        ensure_dir_exists(&inner_dir).unwrap();

        fs::set_permissions(&inner_dir, Permissions::from_mode(0o000)).unwrap();

        let result = dir_size(dir);
        assert!(result.is_err());

        // Cleanup: Set back to writable to allow tempdir to be removed.
        fs::set_permissions(inner_dir, Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn test_create_symlink_inner_target() {
        let tempdir = tempfile::tempdir().unwrap();
        let source = tempdir.path().join("source");
        let target = tempdir.path().join("inner").join("target");

        let result = create_symlink(&source, &target);
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_symlink_target_invalid_parent() {
        let tempdir = tempfile::tempdir().unwrap();
        let source = tempdir.path().join("source");

        let file = tempdir.path().join("file");
        File::create(&file).unwrap();
        let target = tempdir.path().join("file").join("target");

        let result = create_symlink(&source, &target);
        assert!(result.is_err());
    }

    #[test]
    fn test_create_symlink_target_no_permissions() {
        let tempdir = tempfile::tempdir().unwrap();
        let source = tempdir.path().join("source");
        let target = tempdir.path().join("target");
        File::create(&target).unwrap();

        // Set read-only permissions on the parent directory.
        let mut perms = fs::metadata(tempdir.path()).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(tempdir.path(), perms).unwrap();

        let result = create_symlink(&source, &target);
        assert!(result.is_err());

        // Cleanup: Set back to writable to allow tempdir to be removed.
        let mut perms = fs::metadata(tempdir.path()).unwrap().permissions();
        perms.set_readonly(false);
        fs::set_permissions(tempdir.path(), perms).unwrap();
    }
}
