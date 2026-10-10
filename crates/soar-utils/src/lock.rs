//! File-based locking mechanism for preventing concurrent operations.
//!
//! This module provides a simple file-based lock using a `.lock` file to ensure
//! that only one process can operate on a specific resource at a time.

use std::{
    fs::{self, File},
    os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};

use nix::{
    fcntl::{Flock, FlockArg, OFlag},
    sys::stat::Mode,
    unistd::Uid,
};

use crate::error::{LockError, LockResult};

/// A file-based lock using `flock`.
///
/// The lock is automatically released when `FileLock` is dropped.
#[derive(Debug)]
pub struct FileLock {
    _file: nix::fcntl::Flock<File>,
    path: PathBuf,
}

impl FileLock {
    /// The fallback lock directory: uid-scoped under the temp dir.
    fn fallback_lock_dir() -> PathBuf {
        std::env::temp_dir().join(format!("soar-locks-{}", Uid::current()))
    }

    /// Prepares a fallback lock directory in shared temp space.
    ///
    /// Creates it owner-only and re-verifies on every use; plants are
    /// refused, and the creation itself is never trusted.
    fn ensure_lock_dir(dir: &Path) -> LockResult<()> {
        if fs::symlink_metadata(dir).is_err_and(|err| err.kind() == std::io::ErrorKind::NotFound) {
            // Only the leaf may be missing; an `AlreadyExists` rival is
            // revalidated below, never trusted.
            match fs::DirBuilder::new().mode(0o700).create(dir) {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(err) => return Err(err.into()),
            }
        }
        // Revalidate whatever is there now.
        match fs::symlink_metadata(dir) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(LockError::AcquireFailed(format!(
                    "lock directory {} is a symlink",
                    dir.display()
                )));
            }
            Ok(meta) if meta.is_dir() => {
                if meta.uid() != Uid::current().as_raw() {
                    return Err(LockError::AcquireFailed(format!(
                        "lock directory {} is not owned by the current user",
                        dir.display()
                    )));
                }
                // Only the owner may look inside.
                fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
            }
            Ok(_) => {
                return Err(LockError::AcquireFailed(format!(
                    "lock path {} is not a directory",
                    dir.display()
                )));
            }
            Err(err) => return Err(err.into()),
        }
        Ok(())
    }

    /// Get the default lock directory for soar.
    ///
    /// Uses `$XDG_RUNTIME_DIR/soar/locks` or falls back to a uid-scoped
    /// directory under the temp dir, created owner-only and re-verified.
    fn lock_dir() -> LockResult<PathBuf> {
        if let Some(runtime) = std::env::var("XDG_RUNTIME_DIR")
            .ok()
            .filter(|s| !s.is_empty())
        {
            let lock_dir = PathBuf::from(runtime).join("soar/locks");
            fs::create_dir_all(&lock_dir)?;
            return Ok(lock_dir);
        }

        let lock_dir = Self::fallback_lock_dir();
        Self::ensure_lock_dir(&lock_dir)?;
        Ok(lock_dir)
    }

    /// Generate a lock file path for a package.
    fn lock_path(name: &str) -> LockResult<PathBuf> {
        let lock_dir = Self::lock_dir()?;

        // Sanitize the package name to ensure a valid filename
        let sanitize = |s: &str| {
            s.chars()
                .map(|c| {
                    if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect::<String>()
        };

        let filename = format!("{}.lock", sanitize(name));
        Ok(lock_dir.join(filename))
    }

    /// Opens the lock file without following a trailing symlink: a plant
    /// fails the open with the link left in place.
    fn open_lock_file(path: &Path) -> LockResult<File> {
        let fd = nix::fcntl::open(
            path,
            OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::from_bits_truncate(0o600),
        )
        .map_err(|err| LockError::AcquireFailed(format!("{}: {}", path.display(), err)))?;
        Ok(File::from(fd))
    }

    /// Acquire an exclusive lock on a package.
    ///
    /// This will block until the lock can be acquired. Callers that cannot
    /// afford to block use [`Self::try_acquire`] with their own retry bound.
    ///
    /// # Arguments
    ///
    /// * `name` - Package name
    ///
    /// # Returns
    ///
    /// Returns a `FileLock` that will automatically release the lock when dropped.
    pub fn acquire(name: &str) -> LockResult<Self> {
        let lock_path = Self::lock_path(name)?;

        let file = Self::open_lock_file(&lock_path)?;

        let file = Flock::lock(file, FlockArg::LockExclusive).map_err(|(_, err)| {
            LockError::AcquireFailed(format!("{}: {}", lock_path.display(), err))
        })?;

        Ok(FileLock {
            path: lock_path,
            _file: file,
        })
    }

    /// Try to acquire an exclusive lock without blocking.
    ///
    /// Returns `None` if the lock is already held by another process.
    ///
    /// # Arguments
    ///
    /// * `name` - Package name
    pub fn try_acquire(name: &str) -> LockResult<Option<Self>> {
        let lock_path = Self::lock_path(name)?;

        let file = Self::open_lock_file(&lock_path)?;

        match Flock::lock(file, FlockArg::LockExclusiveNonblock) {
            Ok(file) => {
                Ok(Some(FileLock {
                    path: lock_path,
                    _file: file,
                }))
            }
            Err((_, err)) => {
                if matches!(err, nix::errno::Errno::EWOULDBLOCK) {
                    return Ok(None);
                }
                Err(LockError::AcquireFailed(format!(
                    "{}: {}",
                    lock_path.display(),
                    err
                )))
            }
        }
    }

    /// Get the path to the lock file.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Mutex, thread, time::Duration};

    use super::*;

    /// Process-wide env: every test resolving a lock path holds this.
    static ENV_SERIAL: Mutex<()> = Mutex::new(());

    #[test]
    fn test_lock_path_generation() {
        let _env = ENV_SERIAL.lock().unwrap();
        let path = FileLock::lock_path("test-pkg").unwrap();
        assert!(path.to_string_lossy().ends_with("test-pkg.lock"));
    }

    #[test]
    fn test_lock_sanitization() {
        let _env = ENV_SERIAL.lock().unwrap();
        let path = FileLock::lock_path("test/pkg").unwrap();
        assert!(path.to_string_lossy().contains("test_pkg"));
    }

    #[test]
    fn test_exclusive_lock() {
        let _env = ENV_SERIAL.lock().unwrap();
        let lock1 = FileLock::acquire("test-exclusive").unwrap();

        let lock2 = FileLock::try_acquire("test-exclusive").unwrap();
        assert!(lock2.is_none(), "Should not be able to acquire lock");

        drop(lock1);

        let lock3 = FileLock::try_acquire("test-exclusive").unwrap();
        assert!(
            lock3.is_some(),
            "Should be able to acquire lock after release"
        );
    }

    #[test]
    fn test_concurrent_locks_different_packages() {
        let _env = ENV_SERIAL.lock().unwrap();
        let lock1 = FileLock::acquire("pkg-a").unwrap();
        let lock2 = FileLock::acquire("pkg-b").unwrap();

        assert!(lock1.path() != lock2.path());
    }

    #[test]
    fn test_lock_blocks_until_released() {
        let _env = ENV_SERIAL.lock().unwrap();
        let lock1 = FileLock::acquire("test-block").unwrap();
        let path = lock1.path().to_path_buf();

        let handle = thread::spawn(move || {
            let lock2 = FileLock::acquire("test-block").unwrap();
            assert_eq!(lock2.path(), &path);
        });

        thread::sleep(Duration::from_millis(100));

        drop(lock1);

        handle.join().unwrap();
    }

    #[test]
    fn test_fallback_lock_dir_is_uid_scoped() {
        let dir = FileLock::fallback_lock_dir();
        assert!(
            dir.to_string_lossy()
                .contains(&format!("soar-locks-{}", Uid::current())),
            "{dir:?}"
        );
    }

    #[test]
    fn test_planted_fallback_dir_is_refused() {
        use tempfile::tempdir;
        let root = tempdir().unwrap();
        let plant = root.path().join("locks");
        std::os::unix::fs::symlink("/nonexistent", &plant).unwrap();

        let err = FileLock::ensure_lock_dir(&plant).unwrap_err();
        assert!(err.to_string().contains("is a symlink"), "{err}");
    }

    #[test]
    fn test_fallback_dir_permissions_are_tightened() {
        use std::os::unix::fs::PermissionsExt;

        use tempfile::tempdir;
        let root = tempdir().unwrap();
        let dir = root.path().join("locks");
        fs::create_dir_all(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o777)).unwrap();

        FileLock::ensure_lock_dir(&dir).unwrap();
        assert_eq!(dir.metadata().unwrap().permissions().mode() & 0o777, 0o700);
    }

    #[test]
    fn test_planted_file_is_revalidated_not_trusted() {
        use tempfile::tempdir;
        let root = tempdir().unwrap();
        let plant = root.path().join("locks");
        fs::write(&plant, b"planted").unwrap();

        let err = FileLock::ensure_lock_dir(&plant).unwrap_err();
        assert!(err.to_string().contains("not a directory"), "{err}");
    }

    /// Serialized: this test repoints `XDG_RUNTIME_DIR` process-wide.
    #[test]
    fn test_planted_lock_file_is_not_followed() {
        use tempfile::tempdir;
        let _env = ENV_SERIAL.lock().unwrap();
        let runtime = tempdir().unwrap();
        let saved = std::env::var("XDG_RUNTIME_DIR").ok();
        std::env::set_var("XDG_RUNTIME_DIR", runtime.path());

        let victim = runtime.path().join("victim");
        fs::write(&victim, b"victim").unwrap();
        // The parent must exist before the link goes in.
        let lock_file = FileLock::lock_path("planted").unwrap();
        std::os::unix::fs::symlink(&victim, &lock_file).unwrap();

        let err = FileLock::try_acquire("planted").unwrap_err();
        assert!(!err.to_string().is_empty(), "acquisition must fail");
        assert_eq!(fs::read(&victim).unwrap(), b"victim");

        if let Some(value) = saved {
            std::env::set_var("XDG_RUNTIME_DIR", value);
        } else {
            std::env::remove_var("XDG_RUNTIME_DIR");
        }
    }
}
