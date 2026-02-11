//! File-based locking mechanism for preventing concurrent operations.
//!
//! This module provides a simple file-based lock using a `.lock` file to ensure
//! that only one process can operate on a specific resource at a time.

use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};

use crate::error::{LockError, LockResult};

/// A file-based lock using `flock`.
///
/// The lock is automatically released when `FileLock` is dropped.
pub struct FileLock {
    _file: nix::fcntl::Flock<File>,
    path: PathBuf,
}

impl FileLock {
    /// Get the default lock directory for soar.
    ///
    /// Uses `$XDG_RUNTIME_DIR/soar/locks` or falls back to `/tmp/soar-locks`.
    fn lock_dir() -> LockResult<PathBuf> {
        let xdg_runtime = std::env::var("XDG_RUNTIME_DIR").ok();
        let base = if let Some(ref runtime) = xdg_runtime {
            PathBuf::from(runtime)
        } else {
            std::env::temp_dir()
        };

        let lock_dir = base.join("soar").join("locks");

        if !lock_dir.exists() {
            fs::create_dir_all(&lock_dir)?;
        }

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

    /// Acquire an exclusive lock on a package.
    ///
    /// This will block until the lock can be acquired.
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

        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)?;

        let file = nix::fcntl::Flock::lock(file, nix::fcntl::FlockArg::LockExclusive).map_err(
            |(_, err)| LockError::AcquireFailed(format!("{}: {}", lock_path.display(), err)),
        )?;

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

        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)?;

        match nix::fcntl::Flock::lock(file, nix::fcntl::FlockArg::LockExclusiveNonblock) {
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
    use std::{thread, time::Duration};

    use super::*;

    #[test]
    fn test_lock_path_generation() {
        let path = FileLock::lock_path("test-pkg").unwrap();
        assert!(path.to_string_lossy().ends_with("test-pkg.lock"));
    }

    #[test]
    fn test_lock_sanitization() {
        let path = FileLock::lock_path("test/pkg").unwrap();
        assert!(path.to_string_lossy().contains("test_pkg"));
    }

    #[test]
    fn test_exclusive_lock() {
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
        let lock1 = FileLock::acquire("pkg-a").unwrap();
        let lock2 = FileLock::acquire("pkg-b").unwrap();

        assert!(lock1.path() != lock2.path());
    }

    #[test]
    fn test_lock_blocks_until_released() {
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
}
