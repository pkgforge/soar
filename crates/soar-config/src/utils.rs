use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::PermissionsExt as _,
    path::Path,
};

/// Writes `contents` to `path` through a new file beside it that replaces it
/// in one rename, so a crash or a full disk never leaves it half written.
/// The directory is synced after the rename, so success means it is durable.
///
/// A symlinked file is written at its target, keeping the link, and an
/// existing file keeps its permissions.
pub fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    let target = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut name = target
        .file_name()
        .ok_or_else(|| std::io::Error::other(format!("{} names no file", path.display())))?
        .to_os_string();
    name.push(format!(".{}.tmp", std::process::id()));
    let tmp = target.with_file_name(name);
    fs::remove_file(&tmp).ok();

    let result = (|| {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        if let Ok(meta) = fs::metadata(&target) {
            file.set_permissions(fs::Permissions::from_mode(meta.permissions().mode()))?;
        }
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
        fs::rename(&tmp, &target)?;
        let dir = target
            .parent()
            .filter(|dir| !dir.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::File::open(dir)?.sync_all()
    })();
    if result.is_err() {
        fs::remove_file(&tmp).ok();
    }
    result
}

pub fn default_install_patterns() -> Vec<String> {
    ["!*.log", "!SBUILD", "!*.json", "!*.version"]
        .into_iter()
        .map(String::from)
        .collect::<Vec<String>>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_atomic_replaces_and_keeps_links_and_modes() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real.toml");
        let link = dir.path().join("link.toml");
        fs::write(&real, "old").unwrap();
        fs::set_permissions(&real, fs::Permissions::from_mode(0o600)).unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();

        write_atomic(&link, "new").unwrap();

        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_to_string(&real).unwrap(), "new");
        assert_eq!(
            fs::metadata(&real).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::read_dir(dir.path()).unwrap().count(),
            2,
            "temp file left behind"
        );

        let fresh = dir.path().join("fresh.toml");
        write_atomic(&fresh, "x").unwrap();
        assert_eq!(fs::read_to_string(&fresh).unwrap(), "x");
    }

    #[test]
    fn test_default_install_patterns() {
        let patterns = default_install_patterns();

        assert_eq!(patterns.len(), 4);
        assert!(patterns.contains(&"!*.log".to_string()));
        assert!(patterns.contains(&"!SBUILD".to_string()));
        assert!(patterns.contains(&"!*.json".to_string()));
        assert!(patterns.contains(&"!*.version".to_string()));
    }
}
