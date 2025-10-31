use std::path::PathBuf;

use ureq::http::HeaderValue;
use url::Url;

use crate::error::DownloadError;

/// Extracts the final path segment from a URL as a filename.

///

/// Returns `Some(String)` containing the last non-empty path segment if the URL

/// parses and its path has a non-empty final segment, otherwise `None`.

///

/// # Examples

///

/// ```

/// let name = filename_from_url("https://example.com/path/to/file.txt");

/// assert_eq!(name.as_deref(), Some("file.txt"));

///

/// let no_name = filename_from_url("https://example.com/path/to/");

/// assert_eq!(no_name, None);

///

/// let invalid = filename_from_url("not a url");

/// assert_eq!(invalid, None);

/// ```
pub fn filename_from_url(url: &str) -> Option<String> {
    Url::parse(url).ok().and_then(|u| {
        u.path_segments()
            .and_then(|mut s| s.next_back())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
    })
}

/// Extracts a filename from a `Content-Disposition` header value.
///
/// Parses the header string for a `filename=` parameter and returns its value
/// with surrounding double quotes removed.
///
/// # Examples
///
/// ```
/// use ureq::http::HeaderValue;
/// let header = HeaderValue::from_static("attachment; filename=\"example.txt\"");
/// assert_eq!(crate::utils::filename_from_header(&header), Some("example.txt".to_string()));
/// ```
pub fn filename_from_header(value: &HeaderValue) -> Option<String> {
    value
        .to_str()
        .ok()?
        .split(';')
        .find_map(|p| p.trim().strip_prefix("filename="))
        .map(|s| s.trim_matches('"').to_string())
}

/// Compute the final file system path for a download based on an explicit output and inferred filenames.
///
/// The `output` argument can be:
/// - `Some("-")` to represent writing to stdout (path is `"-"`).
/// - A string ending with `'/'` to indicate a directory; a filename is chosen from `header_filename` or `url_filename`.
/// - A path that is an existing directory, in which case a filename is chosen from `header_filename` or `url_filename`.
/// - A file path (returned as-is).
/// If `output` is `None`, a filename is required from `header_filename` or `url_filename`.
///
/// # Returns
///
/// `Ok(PathBuf)` with the resolved path, or `Err(DownloadError::NoFilename)` if a filename is required but neither `header_filename` nor `url_filename` is available.
///
/// # Examples
///
/// ```
/// use std::path::PathBuf;
/// use crate::error::DownloadError;
///
/// // explicit directory with trailing slash prefers header filename
/// let out = super::resolve_output_path(Some("downloads/"), Some("from_url.txt".into()), Some("from_header.txt".into())).unwrap();
/// assert_eq!(out, PathBuf::from("downloads").join("from_header.txt"));
///
/// // stdout shortcut
/// let out = super::resolve_output_path(Some("-"), None, None).unwrap();
/// assert_eq!(out, PathBuf::from("-"));
///
/// // no output given uses header or url filename
/// let out = super::resolve_output_path(None, Some("a.txt".into()), None).unwrap();
/// assert_eq!(out, PathBuf::from("a.txt"));
///
/// // error when no filename available
/// let err = super::resolve_output_path(None, None, None).unwrap_err();
/// assert_eq!(err, DownloadError::NoFilename);
/// ```
pub fn resolve_output_path(
    output: Option<&str>,
    url_filename: Option<String>,
    header_filename: Option<String>,
) -> Result<PathBuf, DownloadError> {
    match output {
        Some("-") => Ok(PathBuf::from("-")),
        Some(p) if p.ends_with('/') => {
            let filename = header_filename
                .or(url_filename)
                .ok_or(DownloadError::NoFilename)?;
            Ok(PathBuf::from(p).join(filename))
        }
        Some(p) => {
            let path = PathBuf::from(p);
            if path.is_dir() {
                let filename = header_filename
                    .or(url_filename)
                    .ok_or(DownloadError::NoFilename)?;
                Ok(path.join(filename))
            } else {
                Ok(path)
            }
        }
        None => {
            header_filename
                .or(url_filename)
                .map(PathBuf::from)
                .ok_or(DownloadError::NoFilename)
        }
    }
}