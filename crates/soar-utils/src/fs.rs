use std::{
    ffi::{OsStr, OsString},
    fs::{self, File},
    io::{BufReader, Read},
    os::{
        self,
        fd::{AsFd, OwnedFd},
        unix::ffi::OsStrExt,
    },
    path::Path,
};

use nix::{
    dir::Dir,
    errno::Errno,
    fcntl::{openat, OFlag},
    sys::stat::{fstat, Mode, SFlag},
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
/// Returns a [`FileSystemError::RemoveFile`] if the removal fails for any reason other than
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

    let metadata = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => {
            return Err(FileSystemError::RemoveFile {
                path: path.to_path_buf(),
                source: e,
            });
        }
    };

    let result = if metadata.is_dir() {
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
/// * [`FileSystemError::CreateDirectory`] if the directory could not be created.
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
/// If `target` is already a symlink, it is replaced. Anything else occupying
/// the path, a regular file or a directory, is never removed: the creation
/// fails and the caller decides whether skipping is acceptable.
///
/// # Arguments
///
/// * `source` - The path to the file or directory to symlink
/// * `target` - The path to the symlink
///
/// # Errors
///
/// Returns a [`FileSystemError::CreateSymlink`] if the symlink could not be created.
/// Returns a [`FileSystemError::RemoveFile`] if the symlink could not be removed.
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

    // A parentless target lives in the current directory: nothing to
    // create.
    if let Some(parent) = target.parent().filter(|p| !p.as_os_str().is_empty()) {
        ensure_dir_exists(parent)?;
    }

    // Only a link is ever cleared; anything else is refused by symlink()
    // with EEXIST for the caller to decide on.
    if fs::symlink_metadata(target).is_ok_and(|m| m.file_type().is_symlink()) {
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
/// Traversal is bound to open descriptors, not pathnames: entries are
/// classified and descended through `O_NOFOLLOW` opens, so a swap for a
/// link fails the open instead of redirecting the walk.
///
/// Links are reported, never descended into. Vanished entries skip; an
/// unreadable subdirectory fails, keeping the old contract.
///
/// # Arguments
///
/// * `dir` - The directory to walk
/// * `action` - The function to call on each file or directory
///
/// # Errors
///
/// Returns a [`FileSystemError::ReadDirectory`] if the directory could not be read.
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

    // Open without following: a symlinked root is refused rather than walked.
    let mut root = Dir::open(
        dir,
        OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map_err(|err| {
        let source = std::io::Error::from(err);
        if dir.is_dir() {
            FileSystemError::ReadDirectory {
                path: dir.to_path_buf(),
                source,
            }
            .into()
        } else {
            FileSystemError::NotADirectory {
                path: dir.to_path_buf(),
            }
            .into()
        }
    })?;

    walk_dir_fd(&mut root, dir, action)
}

/// Traverses an already-opened directory against its descriptor.
fn walk_dir_fd<F, E>(dir: &mut Dir, prefix: &Path, action: &mut F) -> Result<(), E>
where
    F: FnMut(&Path) -> Result<(), E>,
    FileSystemError: Into<E>,
{
    // Names snapshot first: entries borrow the directory buffer.
    let names: Vec<OsString> = dir
        .iter()
        .filter_map(|entry| {
            entry
                .ok()
                .map(|entry| OsStr::from_bytes(entry.file_name().to_bytes()).to_owned())
        })
        .collect();
    for name in &names {
        let name = name.as_os_str();
        if name == "." || name == ".." {
            continue;
        }
        let path = prefix.join(name);
        // Classify through a descriptor that follows nothing.
        let fd: OwnedFd = match openat(
            dir.as_fd(),
            name,
            OFlag::O_PATH | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        ) {
            Ok(fd) => fd,
            Err(_) => continue,
        };
        let kind = match fstat(&fd) {
            Ok(stat) => SFlag::from_bits_truncate(stat.st_mode),
            Err(_) => continue,
        };
        if kind.contains(SFlag::S_IFDIR) {
            let child = match openat(
                dir.as_fd(),
                name,
                OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                Mode::empty(),
            ) {
                Ok(fd) => fd,
                // Changed kind mid-walk (gone, link, or non-directory):
                // none can be descended into, so skip. Anything else keeps
                // the old contract and fails the walk.
                Err(Errno::ENOENT | Errno::ELOOP | Errno::ENOTDIR) => continue,
                Err(err) => {
                    return Err(FileSystemError::ReadDirectory {
                        path: path.clone(),
                        source: std::io::Error::from(err),
                    }
                    .into());
                }
            };
            let mut child = match Dir::from_fd(child) {
                Ok(child) => child,
                Err(_) => continue,
            };
            walk_dir_fd(&mut child, &path, action)?;
        } else {
            // Reported, never descended into.
            action(&path)?;
        }
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
/// Returns a [`FileSystemError::ReadFile`] if the file could not be opened or read.
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
    // Cap the allocation: the length comes from callers, and a bogus one
    // must not turn into a huge buffer. Short files already fail below in
    // `read_exact` instead of reading as zero-padded.
    const MAX_SIGNATURE: usize = 64 * 1024;
    if bytes > MAX_SIGNATURE {
        return Err(FileSystemError::ReadFile {
            path: path.to_path_buf(),
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("signature length {bytes} exceeds {MAX_SIGNATURE} bytes"),
            ),
        });
    }
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
/// Returns a [`FileSystemError::ReadDirectory`] if the directory itself cannot be read.
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

        // entry.metadata is lstat, so symlinks are never files
        // or dirs here: linked trees contribute nothing.
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
/// Unreadable files read as `false`: this is a probe for discovery filters,
/// not a validation. Callers that must tell "not ELF" apart from "cannot be
/// read" use [`read_file_signature`] directly.
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
        perms.set_mode(0o755);
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
        perms.set_mode(0o755);
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
        perms.set_mode(0o755);
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
    fn test_create_symlink_refuses_a_regular_file() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("source");
        let target = dir.path().join("target");
        fs::write(&source, "content").unwrap();
        fs::write(&target, "mine").unwrap();
        assert!(create_symlink(&source, &target).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"mine");
    }

    #[test]
    fn test_create_symlink_replaces_a_link() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("source");
        let target = dir.path().join("target");
        fs::write(&source, "content").unwrap();
        std::os::unix::fs::symlink("/nonexistent", &target).unwrap();
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
        perms.set_mode(0o755);
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

        results.sort();
        let mut expected = vec![file, nested_file];
        expected.sort();
        assert_eq!(results, expected);
    }

    /// Contended swaps must not redirect the walk: descent is `O_NOFOLLOW`,
    /// so a thousand walks under a flipping victim all stay in-tree.
    #[test]
    fn test_walk_dir_survives_concurrent_dir_to_symlink_swaps() {
        use std::{
            os::unix::fs::symlink,
            sync::{
                atomic::{AtomicBool, Ordering},
                Arc,
            },
            thread,
        };

        let tempdir = tempfile::tempdir().unwrap();
        let root = tempdir.path().join("root");
        let victim = root.join("victim");
        let stash = root.join("stash");
        let outside = tempdir.path().join("outside");
        fs::create_dir_all(&victim).unwrap();
        fs::create_dir_all(&outside).unwrap();
        for i in 0..5 {
            File::create(victim.join(format!("f{i:03}"))).unwrap();
        }
        fs::write(outside.join("ESCAPED"), b"escape").unwrap();

        let stop = Arc::new(AtomicBool::new(false));
        let swapper = {
            let stop = stop.clone();
            let victim = victim.clone();
            let stash = stash.clone();
            let outside = outside.clone();
            thread::spawn(move || {
                // No sleeps: flip as fast as the syscalls go, so swaps land
                // inside walks as often as scheduling allows.
                while !stop.load(Ordering::Relaxed) {
                    if fs::rename(&victim, &stash).is_ok() {
                        let _ = symlink(&outside, &victim);
                        let _ = fs::remove_file(&victim);
                        let _ = fs::rename(&stash, &victim);
                    }
                }
            })
        };

        for _ in 0..1000 {
            let mut visited = Vec::new();
            walk_dir(&root, &mut |p| -> FileSystemResult<()> {
                visited.push(p.to_path_buf());
                Ok(())
            })
            .expect("a swapped entry must skip or report, never fail the walk");
            for path in &visited {
                assert!(
                    path.starts_with(&root),
                    "walk escaped the root: {}",
                    path.display()
                );
            }
            assert!(
                !visited.iter().any(|p| p.ends_with("ESCAPED")),
                "walk entered the outside tree"
            );
        }

        stop.store(true, Ordering::Relaxed);
        swapper.join().expect("swapper thread panicked");
    }

    #[test]
    fn test_walk_dir_does_not_enter_symlinked_dirs() {
        let tempdir = tempfile::tempdir().unwrap();
        let dir = tempdir.path().join("dir");
        fs::create_dir(&dir).unwrap();
        let outside = tempdir.path().join("outside");
        fs::create_dir(&outside).unwrap();
        let canary = outside.join("canary");
        File::create(&canary).unwrap();
        std::os::unix::fs::symlink(&outside, dir.join("link")).unwrap();
        let path = dir.clone();
        let pid = unsafe { nix::libc::fork() };
        assert!(pid >= 0, "fork failed");
        if pid == 0 {
            let mut visited = Vec::new();
            let walked = walk_dir(&path, &mut |p| -> FileSystemResult<()> {
                visited.push(p.to_path_buf());
                Ok(())
            });
            let leaked = visited != vec![path.join("link")];
            let code = i32::from(walked.is_err() || leaked);
            unsafe { nix::libc::_exit(code) };
        }
        let mut status = 0;
        assert_eq!(unsafe { nix::libc::waitpid(pid, &mut status, 0) }, pid);
        assert!(
            nix::libc::WIFEXITED(status) && nix::libc::WEXITSTATUS(status) == 0,
            "walk_dir entered a symlinked dir or failed on a symlink loop"
        );
    }

    #[test]
    fn test_dir_size_ignores_symlinks() {
        // DirEntry::metadata does not follow a final symlink, so loops
        // already terminate here. This pins that symlinks contribute
        // nothing: no double counting, no outside bytes.
        let tempdir = tempfile::tempdir().unwrap();
        let dir = tempdir.path().join("dir");
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("file"), b"12345678").unwrap();
        let outside = tempdir.path().join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("big"), vec![0u8; 65536]).unwrap();
        std::os::unix::fs::symlink(&outside, dir.join("link")).unwrap();
        std::os::unix::fs::symlink(&dir, dir.join("loop")).unwrap();
        assert_eq!(dir_size(&dir).unwrap(), 8);
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
        perms.set_mode(0o755);
        fs::set_permissions(tempdir.path(), perms).unwrap();
    }
}
