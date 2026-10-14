use std::path::Path;

use tracing::warn;
use ureq::http::HeaderValue;

use crate::types::ResumeInfo;

const XATTR_RESUME_KEY: &str = "user.soar.resume";

/// Reads the `user.soar.resume` extended attribute from the given path and deserializes it into a `ResumeInfo`.
///
/// Returns `Some(ResumeInfo)` if the attribute exists and contains valid JSON; returns `None` if the attribute is missing, cannot be read, or fails to deserialize.
///
/// # Examples
///
/// ```
/// use soar_dl::xattr::read_resume;
///
/// // Attempt to read resume info from a file path.
/// if let Some(info) = read_resume("/path/to/file") {
///     let _ = info;
/// } else {
///     // no resume info available or it could not be parsed
/// }
/// ```
pub fn read_resume<P: AsRef<Path>>(path: P) -> Option<ResumeInfo> {
    xattr::get(path, XATTR_RESUME_KEY)
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_slice(&v).ok())
}

/// Reads the resume state stored for `path`, keeping it only while it still
/// describes the file there.
///
/// The state is stale when `path` is not a regular file, is shorter than the
/// offset recorded, when that offset runs past the recorded total, or when the
/// recorded ETag could not be sent back in `If-Range`. Resuming from stale
/// state would pad the file or splice two versions of it together, so it is
/// dropped and the download starts over.
pub fn read_usable_resume(path: &Path) -> Option<ResumeInfo> {
    let info = read_resume(path)?;
    let usable = std::fs::symlink_metadata(path).is_ok_and(|meta| {
        meta.is_file()
            && meta.len() >= info.downloaded
            && (info.total == 0 || info.downloaded <= info.total)
            && info
                .etag
                .as_deref()
                .is_none_or(|tag| HeaderValue::from_str(tag).is_ok())
    });
    if usable {
        return Some(info);
    }

    warn!(path = %path.display(), "discarding stale resume state");
    if let Err(err) = remove_resume(path) {
        warn!(path = %path.display(), %err, "failed to clear resume state");
    }
    None
}

/// Writes the provided `ResumeInfo` into the file's extended attribute `user.soar.resume`.
///
/// Serializes `info` to JSON and stores the resulting bytes in the extended attribute for `path`.
///
/// # Returns
///
/// `Ok(())` on success, or an `std::io::Error` if serialization or the xattr write fails.
///
/// # Examples
///
/// ```no_run
/// use soar_dl::xattr::write_resume;
/// use soar_dl::types::ResumeInfo;
///
/// let info = ResumeInfo {
///     downloaded: 1024,
///     total: 10240,
///     etag: Some("etag-value".into()),
///     last_modified: None,
/// };
/// let path = std::path::Path::new("/tmp/download.partial");
/// write_resume(path, &info).unwrap();
/// ```
pub fn write_resume<P: AsRef<Path>>(path: P, info: &ResumeInfo) -> std::io::Result<()> {
    xattr::set(path, XATTR_RESUME_KEY, &serde_json::to_vec(info)?)
}

/// Removes the stored resume extended attribute from the given path.
///
/// Returns `Ok(())` if the attribute was removed successfully, or an `Err` with the I/O error encountered.
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use soar_dl::xattr::remove_resume;
///
/// // Call with any path; result indicates whether the xattr removal succeeded.
/// let _ = remove_resume(Path::new("/tmp/some_file"));
/// ```
pub fn remove_resume<P: AsRef<Path>>(path: P) -> std::io::Result<()> {
    if read_resume(&path).is_none() {
        return Ok(());
    }
    xattr::remove(path, XATTR_RESUME_KEY)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resume(downloaded: u64, total: u64, etag: Option<&str>) -> ResumeInfo {
        ResumeInfo {
            downloaded,
            total,
            etag: etag.map(String::from),
            last_modified: None,
        }
    }

    #[test]
    fn stale_resume_state_is_discarded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("partial");
        std::fs::write(&path, b"abc").unwrap();
        if write_resume(&path, &resume(1, 10, None)).is_err() {
            eprintln!("skipping: filesystem lacks user xattrs");
            return;
        }
        assert!(read_usable_resume(&path).is_some());

        for stale in [
            resume(1000, 2000, None),
            resume(3, 2, None),
            resume(1, 10, Some("bad\netag")),
        ] {
            write_resume(&path, &stale).unwrap();
            assert!(read_usable_resume(&path).is_none(), "{stale:?}");
            assert!(read_resume(&path).is_none(), "stale state kept: {stale:?}");
        }
    }
}
