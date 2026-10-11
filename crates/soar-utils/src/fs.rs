use std::{
    ffi::{OsStr, OsString},
    fs::{self, File},
    io::{BufReader, Read},
    os::{
        self,
        fd::{AsFd, OwnedFd},
        unix::ffi::OsStrExt,
    },
    path::{Component, Path, PathBuf},
};

use nix::{
    dir::Dir,
    errno::Errno,
    fcntl::{openat, OFlag},
    sys::stat::{fchmod, fstat, Mode, SFlag},
    unistd::{unlinkat, UnlinkatFlags},
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

/// Opens a caller-supplied root directory.
///
/// The root itself is trusted configuration and may be a symlink, for
/// example a packages tree on another disk or a dotfiles-managed
/// applications directory. Containment comes from opening every entry
/// below this descriptor with `O_NOFOLLOW`, which is unchanged.
fn open_dir(dir: &Path) -> FileSystemResult<Dir> {
    Dir::open(dir, OFlag::O_DIRECTORY | OFlag::O_CLOEXEC, Mode::empty()).map_err(|err| {
        let source = std::io::Error::from(err);
        if dir.is_dir() {
            FileSystemError::ReadDirectory {
                path: dir.to_path_buf(),
                source,
            }
        } else {
            FileSystemError::NotADirectory {
                path: dir.to_path_buf(),
            }
        }
    })
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

    // The root is trusted configuration: it may itself be a symlink.
    // Everything below it is still opened without following.
    let mut root = open_dir(dir).map_err(Into::into)?;

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
            // An opened directory that will not stream fails the walk:
            // silently skipping it would report a partial tree as whole.
            let mut child = match Dir::from_fd(child) {
                Ok(child) => child,
                Err(err) => {
                    return Err(FileSystemError::ReadDirectory {
                        path: path.clone(),
                        source: std::io::Error::from(err),
                    }
                    .into());
                }
            };
            walk_dir_fd(&mut child, &path, action)?;
        } else {
            // Reported, never descended into.
            action(&path)?;
        }
    }

    Ok(())
}

/// Splits `target` into its leaf and ancestor components strictly below `root`, refusing anything else.
///
/// The shared lexical gate for contained operations: absolute paths only,
/// no `..` in either side (a `..` in the root would desynchronize the
/// display path from the opened directory), and at least one component
/// below the root.
fn strict_descendant<'a>(
    root: &Path,
    target: &'a Path,
) -> Result<(&'a OsStr, Vec<&'a OsStr>), String> {
    if !target.is_absolute() {
        return Err("path is not absolute".to_string());
    }
    if target
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        return Err("path contains `..`".to_string());
    }
    if root.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err("root contains `..`".to_string());
    }
    let rest = target
        .strip_prefix(root)
        .map_err(|_| "path is outside the root".to_string())?;
    let mut normals: Vec<&OsStr> = Vec::new();
    for comp in rest.components() {
        match comp {
            Component::Normal(name) => normals.push(name),
            Component::CurDir => {}
            _ => return Err("path is outside the root".to_string()),
        }
    }
    match normals.split_last() {
        Some((leaf, ancestors)) => Ok((*leaf, ancestors.to_vec())),
        None => Err("path names the root itself".to_string()),
    }
}

/// Deletes `target`, which must lie strictly inside `root`, without trusting pathnames while doing it.
///
/// Opens `root` and descends to `target` one component at a time with
/// `O_NOFOLLOW`, then removes the tree through the open descriptors: every
/// entry is unlinked relative to its parent directory, so a symlink swapped
/// in along the way fails the deletion instead of redirecting it.
///
/// Missing targets succeed: removal is idempotent. A link at the leaf is
/// unlinked as a link, never followed; anything else unsuitable on the way
/// down fails closed.
///
/// Directories that archives shipped read-only get the owner write bit
/// first, or their entries could not be unlinked.
///
/// # Arguments
///
/// * `root` - The directory the deletion is confined to. It may itself be
///   a symlink, for example a packages tree on another disk; confinement
///   applies to everything below the opened root.
/// * `target` - The directory or file to remove. Must be strictly inside `root`.
///
/// # Errors
///
/// Returns [`FileSystemError::RemoveDirectory`] if the target is not strictly
/// inside `root`, if a link or non-directory blocks the descent, or if an
/// entry cannot be unlinked. Returns [`FileSystemError::ReadDirectory`] if a
/// directory cannot be opened or listed.
///
/// # Example
///
/// ```no_run
/// use std::path::Path;
/// use soar_utils::{error::FileSystemResult, fs::remove_contained_dir};
///
/// fn main() -> FileSystemResult<()> {
///     remove_contained_dir(
///         Path::new("/data/packages"),
///         Path::new("/data/packages/foo-1.0"),
///     )?;
///     Ok(())
/// }
/// ```
pub fn remove_contained_dir(root: &Path, target: &Path) -> FileSystemResult<()> {
    // Lexical gate first. The descriptor walk below carries the traversal
    // safety; this only picks clear errors for plainly wrong inputs.
    let (leaf, ancestors) = strict_descendant(root, target).map_err(|why| refuse(target, &why))?;

    let mut display = root.to_path_buf();
    let mut dir = open_dir(root)?;
    for &comp in &ancestors {
        display.push(comp);
        let child = match openat(
            dir.as_fd(),
            comp,
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        ) {
            Ok(child) => child,
            // Gone above the target: nothing left to remove.
            Err(Errno::ENOENT) => return Ok(()),
            Err(err) => {
                return Err(FileSystemError::ReadDirectory {
                    path: display.clone(),
                    source: std::io::Error::from(err),
                });
            }
        };
        dir = match Dir::from_fd(child) {
            Ok(dir) => dir,
            Err(err) => {
                return Err(FileSystemError::ReadDirectory {
                    path: display.clone(),
                    source: std::io::Error::from(err),
                });
            }
        };
    }
    display.push(leaf);
    remove_leaf(&dir, leaf, &display)
}

/// A refusal to delete: the input never names a valid target.
fn refuse(path: &Path, why: &str) -> FileSystemError {
    FileSystemError::RemoveDirectory {
        path: path.to_path_buf(),
        source: std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("refusing to remove {}: {why}", path.display()),
        ),
    }
}

/// Unlinks the last component of a contained removal through its open parent.
fn remove_leaf(parent: &Dir, leaf: &OsStr, display: &Path) -> FileSystemResult<()> {
    // Classify without following anything.
    let fd = match openat(
        parent.as_fd(),
        leaf,
        OFlag::O_PATH | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    ) {
        Ok(fd) => fd,
        // Gone already: removal is idempotent.
        Err(Errno::ENOENT) => return Ok(()),
        Err(err) => {
            return Err(FileSystemError::ReadDirectory {
                path: display.to_path_buf(),
                source: std::io::Error::from(err),
            });
        }
    };
    let kind = match fstat(&fd) {
        Ok(stat) => SFlag::from_bits_truncate(stat.st_mode),
        // Vanished between open and stat: nothing left to remove.
        Err(Errno::ENOENT) => return Ok(()),
        Err(err) => {
            return Err(FileSystemError::ReadDirectory {
                path: display.to_path_buf(),
                source: std::io::Error::from(err),
            });
        }
    };
    drop(fd);
    if kind.contains(SFlag::S_IFDIR) {
        let child = match openat(
            parent.as_fd(),
            leaf,
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        ) {
            Ok(child) => child,
            Err(Errno::ENOENT) => return Ok(()),
            Err(err) => {
                return Err(FileSystemError::ReadDirectory {
                    path: display.to_path_buf(),
                    source: std::io::Error::from(err),
                });
            }
        };
        let mut child = match Dir::from_fd(child) {
            Ok(child) => child,
            Err(err) => {
                return Err(FileSystemError::ReadDirectory {
                    path: display.to_path_buf(),
                    source: std::io::Error::from(err),
                });
            }
        };
        remove_tree_contents(&mut child, display)?;
        drop(child);
        unlink_entry(parent, leaf, display, UnlinkatFlags::RemoveDir)
    } else {
        // Files, links, and special nodes unlink as themselves: nothing is
        // ever traversed here.
        unlink_entry(parent, leaf, display, UnlinkatFlags::NoRemoveDir)
    }
}

/// Empties an open directory, unlinking every entry relative to it.
///
/// Entries are handled by what each step finds, never by what an earlier
/// step saw: only directories opened through their own descriptor are
/// descended into, and only after they are emptied the same way.
fn remove_tree_contents(dir: &mut Dir, display: &Path) -> FileSystemResult<()> {
    ensure_owner_writable(dir, display)?;
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
        let path = display.join(name);
        let child = match openat(
            dir.as_fd(),
            name,
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        ) {
            Ok(child) => child,
            // Gone, a link, or not a directory: unlink the entry itself,
            // traversing nothing.
            Err(Errno::ENOENT | Errno::ELOOP | Errno::ENOTDIR) => {
                unlink_entry(dir, name, &path, UnlinkatFlags::NoRemoveDir)?;
                continue;
            }
            Err(err) => {
                return Err(FileSystemError::ReadDirectory {
                    path: path.clone(),
                    source: std::io::Error::from(err),
                });
            }
        };
        let mut child = match Dir::from_fd(child) {
            Ok(child) => child,
            Err(err) => {
                return Err(FileSystemError::ReadDirectory {
                    path: path.clone(),
                    source: std::io::Error::from(err),
                });
            }
        };
        remove_tree_contents(&mut child, &path)?;
        drop(child);
        unlink_entry(dir, name, &path, UnlinkatFlags::RemoveDir)?;
    }
    Ok(())
}

/// Adds the owner write bit to an open directory, so its entries can be unlinked.
///
/// A chmod failure is ignored: the unlink reports the real error either way.
fn ensure_owner_writable(dir: &Dir, display: &Path) -> FileSystemResult<()> {
    let mode = match fstat(dir.as_fd()) {
        Ok(stat) => Mode::from_bits_truncate(stat.st_mode),
        Err(err) => {
            return Err(FileSystemError::ReadDirectory {
                path: display.to_path_buf(),
                source: std::io::Error::from(err),
            });
        }
    };
    if !mode.contains(Mode::S_IWUSR) {
        fchmod(dir.as_fd(), mode | Mode::S_IWUSR).ok();
    }
    Ok(())
}

/// Unlinks one entry relative to its open parent directory.
///
/// A vanished entry is already gone: removal is idempotent.
fn unlink_entry(
    parent: &Dir,
    name: &OsStr,
    display: &Path,
    flag: UnlinkatFlags,
) -> FileSystemResult<()> {
    match unlinkat(parent.as_fd(), name, flag) {
        Ok(()) => Ok(()),
        Err(Errno::ENOENT) => Ok(()),
        Err(err) => {
            Err(FileSystemError::RemoveDirectory {
                path: display.to_path_buf(),
                source: std::io::Error::from(err),
            })
        }
    }
}

/// Adds the executable bits to `path`, which must lie strictly inside `dir`, without trusting pathnames while doing it.
///
/// Every directory on the way down is opened with `O_NOFOLLOW` from the
/// previous descriptor, and the file itself is validated and chmodded
/// through its own descriptor. A link swapped in along the way fails the
/// operation instead of redirecting it.
///
/// A directory that is itself a link is descended through only when it
/// resolves inside `dir`; anything resolving outside fails closed. A link
/// at the leaf is chmodded through its target when that target resolves
/// inside, and reported without touching anything when it resolves
/// outside.
///
/// Returns `true` when the file is executable afterwards, `false` when a
/// leaf alias resolves outside and was left alone.
///
/// Missing paths and non-regular files fail: only a real file is ever chmodded.
///
/// # Arguments
///
/// * `dir` - The package tree the chmod is confined to.
/// * `path` - The file to make executable. Must be strictly inside `dir`.
///
/// # Errors
///
/// Returns [`FileSystemError::SetPermissions`] if the target is unsuitable
/// or cannot be reached without following an outside link.
///
/// # Example
///
/// ```no_run
/// use std::path::Path;
/// use soar_utils::{error::FileSystemResult, fs::make_executable_contained};
///
/// fn main() -> FileSystemResult<()> {
///     make_executable_contained(
///         Path::new("/data/packages/foo-1.0"),
///         Path::new("/data/packages/foo-1.0/bin/foo"),
///     )?;
///     Ok(())
/// }
/// ```
pub fn make_executable_contained(dir: &Path, path: &Path) -> FileSystemResult<bool> {
    // Same lexical gate as deletion: absolute, no `..`, strictly below.
    let (leaf, ancestors) = strict_descendant(dir, path).map_err(|why| deny(path, &why))?;
    let mut display = dir.to_path_buf();
    let start = open_dir(dir)?;
    let parent = descend_dirs(dir, start, &mut display, &ancestors)?;
    display.push(leaf);
    finish_executable(dir, &parent, leaf, &display)
}

/// A refusal to chmod: the input never names a valid target.
fn deny(path: &Path, why: &str) -> FileSystemError {
    FileSystemError::SetPermissions {
        path: path.to_path_buf(),
        source: std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("refusing to chmod {}: {why}", path.display()),
        ),
    }
}

/// A chmod failure carrying the underlying error.
fn deny_src(path: &Path, source: std::io::Error) -> FileSystemError {
    FileSystemError::SetPermissions {
        path: path.to_path_buf(),
        source,
    }
}

/// Opens each of `comps` below `parent` in turn, returning the last directory.
///
/// Every component is opened with `O_NOFOLLOW` from the previous
/// descriptor. A component that is a link is descended through only when
/// it resolves inside `root`; anything else unsuitable fails closed.
/// `display` tracks the current path for errors.
fn descend_dirs(
    root: &Path,
    mut parent: Dir,
    display: &mut PathBuf,
    comps: &[&OsStr],
) -> FileSystemResult<Dir> {
    for &comp in comps {
        display.push(comp);
        parent = open_next(root, parent, display, comp)?;
    }
    Ok(parent)
}

/// Opens one directory component below an open parent without following untrusted links.
fn open_next(root: &Path, parent: Dir, display: &Path, comp: &OsStr) -> FileSystemResult<Dir> {
    match openat(
        parent.as_fd(),
        comp,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    ) {
        Ok(fd) => {
            Dir::from_fd(fd).map_err(|err| {
                FileSystemError::ReadDirectory {
                    path: display.to_path_buf(),
                    source: std::io::Error::from(err),
                }
            })
        }
        // Gone, unreadable, or neither a plain directory: find out which
        // without following anything.
        Err(err) => follow_inside_link(root, &parent, display, comp, err),
    }
}

/// Descends through a component that refused a direct directory open, when it is a link resolving inside the root.
///
/// Anything else unsuitable fails closed: only a link staying inside the
/// root is ever descended through, and then from a fresh descriptor on the
/// root rather than through the link itself.
fn follow_inside_link(
    root: &Path,
    parent: &Dir,
    display: &Path,
    comp: &OsStr,
    cause: Errno,
) -> FileSystemResult<Dir> {
    // Classify without following anything.
    let kind = match openat(
        parent.as_fd(),
        comp,
        OFlag::O_PATH | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    ) {
        Ok(fd) => {
            match fstat(&fd) {
                Ok(stat) => SFlag::from_bits_truncate(stat.st_mode),
                Err(err) => {
                    return Err(FileSystemError::ReadDirectory {
                        path: display.to_path_buf(),
                        source: std::io::Error::from(err),
                    });
                }
            }
        }
        Err(err) => {
            return Err(FileSystemError::ReadDirectory {
                path: display.to_path_buf(),
                source: std::io::Error::from(err),
            });
        }
    };
    if !kind.contains(SFlag::S_IFLNK) {
        // A real directory only lands here when it cannot be opened at
        // all: keep the original error instead of misreporting it.
        if kind.contains(SFlag::S_IFDIR) {
            return Err(FileSystemError::ReadDirectory {
                path: display.to_path_buf(),
                source: std::io::Error::from(cause),
            });
        }
        return Err(deny(display, "not a directory"));
    }
    let Some(rel) = resolve_inside(root, display)? else {
        return Err(deny(display, "link resolves outside the root"));
    };
    let start = open_dir(root)?;
    let mut restart = root.to_path_buf();
    let refs: Vec<&OsStr> = rel.iter().map(|s| s.as_os_str()).collect();
    descend_dirs(root, start, &mut restart, &refs)
}

/// Resolves `path` to components below `root`, or `None` when it escapes.
///
/// Both sides are fully resolved first, so a trusted symlinked root and
/// `..` anywhere in the chain compare by where they actually land.
fn resolve_inside(root: &Path, path: &Path) -> FileSystemResult<Option<Vec<OsString>>> {
    let real_root = std::fs::canonicalize(root).map_err(|err| {
        FileSystemError::SetPermissions {
            path: root.to_path_buf(),
            source: err,
        }
    })?;
    let real = std::fs::canonicalize(path).map_err(|err| {
        FileSystemError::SetPermissions {
            path: path.to_path_buf(),
            source: err,
        }
    })?;
    Ok(real.strip_prefix(&real_root).ok().map(|rel| {
        rel.components()
            .filter_map(|c| {
                match c {
                    Component::Normal(name) => Some(name.to_owned()),
                    _ => None,
                }
            })
            .collect()
    }))
}

/// Validates the last component and makes it executable through its own descriptor.
///
/// Returns `false` (leaving everything alone) when a leaf alias resolves
/// outside the root; anything else unsuitable fails.
fn finish_executable(
    root: &Path,
    parent: &Dir,
    name: &OsStr,
    display: &Path,
) -> FileSystemResult<bool> {
    // Classify without following anything.
    let kind = match openat(
        parent.as_fd(),
        name,
        OFlag::O_PATH | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    ) {
        Ok(fd) => {
            match fstat(&fd) {
                Ok(stat) => SFlag::from_bits_truncate(stat.st_mode),
                Err(err) => {
                    return Err(FileSystemError::ReadDirectory {
                        path: display.to_path_buf(),
                        source: std::io::Error::from(err),
                    });
                }
            }
        }
        Err(err) => {
            return Err(deny_src(display, std::io::Error::from(err)));
        }
    };
    if kind.contains(SFlag::S_IFLNK) {
        // Aliases resolve through the same confinement: an inside target
        // chmods through its own descriptor, an outside one is reported.
        let Some(rel) = resolve_inside(root, display)? else {
            return Ok(false);
        };
        let refs: Vec<&OsStr> = rel.iter().map(|s| s.as_os_str()).collect();
        let Some((last, dirs)) = refs.split_last() else {
            return Err(deny(display, "not a regular file"));
        };
        let start = open_dir(root)?;
        let mut restart = root.to_path_buf();
        let parent = descend_dirs(root, start, &mut restart, dirs)?;
        restart.push(last);
        return finish_regular(&parent, last, &restart);
    }
    if !kind.contains(SFlag::S_IFREG) {
        return Err(deny(display, "not a regular file"));
    }
    finish_regular(parent, name, display)
}

/// Makes an already-classified regular file executable through a fresh descriptor.
///
/// Opens with `O_NONBLOCK` so a node swapped in for the file cannot hang
/// the open, then verifies the kind from the opened descriptor itself:
/// the classify-then-open gap and the alias path both end here, so only a
/// regular file is ever chmodded.
/// The rule matches the old behavior exactly: add every executable bit,
/// but only when none is set.
fn finish_regular(parent: &Dir, name: &OsStr, display: &Path) -> FileSystemResult<bool> {
    let fd = match openat(
        parent.as_fd(),
        name,
        OFlag::O_RDONLY | OFlag::O_NONBLOCK | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    ) {
        Ok(fd) => fd,
        Err(err) => {
            return Err(deny_src(display, std::io::Error::from(err)));
        }
    };
    let mode = match fstat(&fd) {
        Ok(stat) => stat.st_mode,
        Err(err) => {
            return Err(FileSystemError::ReadDirectory {
                path: display.to_path_buf(),
                source: std::io::Error::from(err),
            });
        }
    };
    // The descriptor may no longer be what classification saw, and the
    // alias path never classified its target at all: compare the masked
    // type instead of trusting either.
    if SFlag::from_bits_truncate(mode) & SFlag::S_IFMT != SFlag::S_IFREG {
        return Err(deny(display, "not a regular file"));
    }
    if mode & 0o111 == 0 {
        fchmod(&fd, Mode::from_bits_truncate(mode | 0o111)).map_err(|err| {
            FileSystemError::SetPermissions {
                path: display.to_path_buf(),
                source: std::io::Error::from(err),
            }
        })?;
    }
    Ok(true)
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

    /// A read-only tree still deletes: archives commonly ship without the
    /// owner write bit the unlinking needs.
    #[test]
    fn test_remove_contained_dir_deletes_read_only_tree() {
        use std::os::unix::fs::PermissionsExt;
        let tempdir = tempdir().unwrap();
        let root = tempdir.path().join("packages");
        let leaf = root.join("pkg-1.0");
        let sub = leaf.join("sub");
        fs::create_dir_all(&sub).unwrap();
        File::create(leaf.join("bin")).unwrap();
        File::create(sub.join("f")).unwrap();
        for dir in [&leaf, &sub] {
            let mut perms = fs::metadata(dir).unwrap().permissions();
            perms.set_mode(0o555);
            fs::set_permissions(dir, perms).unwrap();
        }
        remove_contained_dir(&root, &leaf).unwrap();
        assert!(!leaf.exists(), "the tree is gone");
        assert!(root.is_dir(), "the root stays");
    }

    /// The root itself, anything outside it, and anything smuggling `..`
    /// are refused before a descriptor is even opened.
    #[test]
    fn test_remove_contained_dir_refuses_unclear_targets() {
        let tempdir = tempdir().unwrap();
        let root = tempdir.path().join("packages");
        fs::create_dir_all(&root).unwrap();
        let outside = tempdir.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("keep"), b"keep").unwrap();
        let bad = vec![
            root.clone(),
            root.join(""),
            outside.clone(),
            outside.join("pkg-1.0"),
            root.join("..").join("outside"),
            Path::new("relative/pkg-1.0").to_path_buf(),
        ];
        for target in &bad {
            assert!(
                remove_contained_dir(&root, target).is_err(),
                "accepted {target:?}"
            );
        }
        assert_eq!(
            fs::read(outside.join("keep")).unwrap(),
            b"keep",
            "nothing outside is touched"
        );
        assert!(root.is_dir(), "the root stays");
    }

    /// A symlink swapped in below the root fails the deletion; the outside
    /// tree it points at survives.
    #[test]
    fn test_remove_contained_dir_stops_at_planted_symlink() {
        use std::os::unix::fs::symlink;
        let tempdir = tempdir().unwrap();
        let root = tempdir.path().join("packages");
        let mid = root.join("mid");
        let leaf = mid.join("pkg-1.0");
        fs::create_dir_all(&leaf).unwrap();
        File::create(leaf.join("bin")).unwrap();
        let outside = tempdir.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("keep"), b"keep").unwrap();
        // Swap the middle component for a link to the outside tree.
        let stash = root.join("stash");
        fs::rename(&mid, &stash).unwrap();
        symlink(&outside, &mid).unwrap();
        assert!(remove_contained_dir(&root, &leaf).is_err());
        assert_eq!(
            fs::read(outside.join("keep")).unwrap(),
            b"keep",
            "the outside tree survives"
        );
        assert!(
            stash.join("pkg-1.0").join("bin").exists(),
            "the real tree is untouched"
        );
    }

    /// A link at the leaf unlinks as a link: its target is never traversed.
    #[test]
    fn test_remove_contained_dir_unlinks_symlink_leaf() {
        use std::os::unix::fs::symlink;
        let tempdir = tempdir().unwrap();
        let root = tempdir.path().join("packages");
        fs::create_dir_all(&root).unwrap();
        let outside = tempdir.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("keep"), b"keep").unwrap();
        let link = root.join("pkg-1.0");
        symlink(&outside, &link).unwrap();
        remove_contained_dir(&root, &link).unwrap();
        assert!(link.symlink_metadata().is_err(), "the link itself is gone");
        assert_eq!(
            fs::read(outside.join("keep")).unwrap(),
            b"keep",
            "the target survives"
        );
    }

    /// Missing targets succeed: removal is idempotent.
    #[test]
    fn test_remove_contained_dir_missing_target_is_ok() {
        let tempdir = tempdir().unwrap();
        let root = tempdir.path().join("packages");
        fs::create_dir_all(&root).unwrap();
        remove_contained_dir(&root, &root.join("gone-1.0")).unwrap();
        remove_contained_dir(&root, &root.join("nope").join("gone-1.0")).unwrap();
    }

    /// A stray file at the leaf unlinks as a file, matching remove_dir_all.
    #[test]
    fn test_remove_contained_dir_removes_file_leaf() {
        let tempdir = tempdir().unwrap();
        let root = tempdir.path().join("packages");
        fs::create_dir_all(&root).unwrap();
        let file = root.join("pkg-1.0");
        File::create(&file).unwrap();
        remove_contained_dir(&root, &file).unwrap();
        assert!(!file.exists());
        assert!(root.is_dir(), "the root stays");
    }

    /// Links inside the tree unlink as links; what they point at survives.
    #[test]
    fn test_remove_contained_dir_never_follows_inner_links() {
        use std::os::unix::fs::symlink;
        let tempdir = tempdir().unwrap();
        let root = tempdir.path().join("packages");
        let leaf = root.join("pkg-1.0");
        let sub = leaf.join("sub");
        fs::create_dir_all(&sub).unwrap();
        File::create(sub.join("f")).unwrap();
        let outside = tempdir.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("keep"), b"keep").unwrap();
        symlink(&outside, sub.join("escape")).unwrap();
        symlink(outside.join("keep"), leaf.join("keep-link")).unwrap();
        remove_contained_dir(&root, &leaf).unwrap();
        assert!(!leaf.exists(), "the tree is gone");
        assert_eq!(
            fs::read(outside.join("keep")).unwrap(),
            b"keep",
            "linked targets survive"
        );
    }

    /// A symlinked root walks fine: the root is trusted configuration,
    /// and containment applies below the opened directory.
    #[test]
    fn test_walk_dir_follows_symlinked_root() {
        use std::os::unix::fs::symlink;
        let tempdir = tempdir().unwrap();
        let real = tempdir.path().join("real");
        let sub = real.join("sub");
        fs::create_dir_all(&sub).unwrap();
        File::create(sub.join("f")).unwrap();
        let link = tempdir.path().join("linked");
        symlink(&real, &link).unwrap();
        let mut visited = Vec::new();
        walk_dir(&link, &mut |p| -> FileSystemResult<()> {
            visited.push(p.to_path_buf());
            Ok(())
        })
        .unwrap();
        assert!(visited.iter().any(|p| p.ends_with("sub/f")));
    }

    /// A symlinked root deletes fine: the tree goes, the link stays.
    #[test]
    fn test_remove_contained_dir_follows_symlinked_root() {
        use std::os::unix::fs::symlink;
        let tempdir = tempdir().unwrap();
        let real = tempdir.path().join("real");
        let leaf = real.join("pkg-1.0");
        fs::create_dir_all(&leaf).unwrap();
        File::create(leaf.join("bin")).unwrap();
        let link = tempdir.path().join("linked");
        symlink(&real, &link).unwrap();
        remove_contained_dir(&link, &link.join("pkg-1.0")).unwrap();
        assert!(!leaf.exists(), "the real tree is gone");
        assert!(
            link.symlink_metadata()
                .is_ok_and(|m| m.file_type().is_symlink()),
            "the root link stays"
        );
    }

    /// A regular file gains every executable bit, but only when none is set.
    #[test]
    fn test_make_executable_contained_chmods_regular_file() {
        use std::os::unix::fs::PermissionsExt;
        let tempdir = tempdir().unwrap();
        let dir = tempdir.path().join("pkg-1.0");
        let bin = dir.join("bin");
        fs::create_dir_all(&bin).unwrap();
        let file = bin.join("tool");
        File::create(&file).unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(make_executable_contained(&dir, &file).unwrap());
        assert_eq!(mode_of(&file), 0o755);
        // Already executable: still true, bits untouched.
        assert!(make_executable_contained(&dir, &file).unwrap());
        assert_eq!(mode_of(&file), 0o755);
    }

    /// A shipped `bin` link to an outside tree fails instead of chmodding
    /// through it.
    #[test]
    fn test_make_executable_contained_refuses_outside_symlinked_dir() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let tempdir = tempdir().unwrap();
        let dir = tempdir.path().join("pkg-1.0");
        fs::create_dir_all(&dir).unwrap();
        let outside = tempdir.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        let victim = outside.join("tool");
        File::create(&victim).unwrap();
        fs::set_permissions(&victim, fs::Permissions::from_mode(0o644)).unwrap();
        symlink(&outside, dir.join("bin")).unwrap();
        assert!(make_executable_contained(&dir, &dir.join("bin").join("tool")).is_err());
        assert_eq!(mode_of(&victim), 0o644, "the outside file is untouched");
    }

    /// A `bin` link staying inside the tree keeps working: only escapes fail.
    #[test]
    fn test_make_executable_contained_follows_inside_symlinked_dir() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let tempdir = tempdir().unwrap();
        let dir = tempdir.path().join("pkg-1.0");
        let real = dir.join("real");
        fs::create_dir_all(&real).unwrap();
        let file = real.join("tool");
        File::create(&file).unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        symlink(&real, dir.join("bin")).unwrap();
        assert!(make_executable_contained(&dir, &dir.join("bin").join("tool")).unwrap());
        assert_eq!(mode_of(&file), 0o755);
    }

    /// An alias resolving inside chmods through its target's descriptor.
    #[test]
    fn test_make_executable_contained_chmods_inside_alias() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let tempdir = tempdir().unwrap();
        let dir = tempdir.path().join("pkg-1.0");
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("tool");
        File::create(&target).unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o644)).unwrap();
        let alias = dir.join("alias");
        symlink("tool", &alias).unwrap();
        assert!(make_executable_contained(&dir, &alias).unwrap());
        assert_eq!(mode_of(&target), 0o755);
    }

    /// An alias resolving outside reports without touching anything.
    #[test]
    fn test_make_executable_contained_leaves_outside_alias_alone() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let tempdir = tempdir().unwrap();
        let dir = tempdir.path().join("pkg-1.0");
        fs::create_dir_all(&dir).unwrap();
        let outside = tempdir.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        let victim = outside.join("tool");
        File::create(&victim).unwrap();
        fs::set_permissions(&victim, fs::Permissions::from_mode(0o644)).unwrap();
        let alias = dir.join("alias");
        symlink(&victim, &alias).unwrap();
        assert!(!make_executable_contained(&dir, &alias).unwrap());
        assert_eq!(mode_of(&victim), 0o644, "the outside file is untouched");
    }

    /// Directories, missing paths, and outside targets all fail.
    #[test]
    fn test_make_executable_contained_refuses_unsuitable_targets() {
        let tempdir = tempdir().unwrap();
        let dir = tempdir.path().join("pkg-1.0");
        fs::create_dir_all(&dir).unwrap();
        assert!(make_executable_contained(&dir, &dir).is_err());
        assert!(make_executable_contained(&dir, &dir.join("gone")).is_err());
        assert!(make_executable_contained(&dir, &tempdir.path().join("other")).is_err());
    }

    /// An alias to a directory fails instead of chmodding it.
    #[test]
    fn test_make_executable_contained_refuses_alias_to_dir() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let tempdir = tempdir().unwrap();
        let dir = tempdir.path().join("pkg-1.0");
        let sub = dir.join("sub");
        fs::create_dir_all(&sub).unwrap();
        fs::set_permissions(&sub, fs::Permissions::from_mode(0o555)).unwrap();
        let alias = dir.join("alias");
        symlink("sub", &alias).unwrap();
        assert!(make_executable_contained(&dir, &alias).is_err());
        assert_eq!(mode_of(&sub), 0o555, "the directory mode is untouched");
    }

    /// An alias to a fifo fails instead of chmodding (or hanging on) it.
    #[test]
    fn test_make_executable_contained_refuses_alias_to_fifo() {
        use std::os::unix::fs::symlink;
        let tempdir = tempdir().unwrap();
        let dir = tempdir.path().join("pkg-1.0");
        fs::create_dir_all(&dir).unwrap();
        let fifo = dir.join("pipe");
        nix::unistd::mkfifo(&fifo, Mode::from_bits_truncate(0o644)).unwrap();
        let alias = dir.join("alias");
        symlink("pipe", &alias).unwrap();
        assert!(make_executable_contained(&dir, &alias).is_err());
        assert_eq!(mode_of(&fifo) & 0o111, 0, "no exec bits are added");
    }

    fn mode_of(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path).unwrap().permissions().mode() & 0o777
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
