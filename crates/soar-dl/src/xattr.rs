use std::path::Path;

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
