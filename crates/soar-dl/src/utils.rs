use std::{
    fs::{File, OpenOptions},
    io::{Seek as _, SeekFrom},
    os::unix::fs::OpenOptionsExt as _,
    path::{Path, PathBuf},
};

use nix::libc;
use percent_encoding::percent_decode_str;
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
/// use soar_dl::utils::filename_from_url;
///
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
            .and_then(|s| {
                percent_decode_str(s)
                    .decode_utf8()
                    .ok()
                    .map(|cow| cow.into_owned())
            })
    })
}

/// Extracts a filename from a `Content-Disposition` header value.
///
/// An RFC 5987 `filename*` parameter is preferred over a plain `filename`, and
/// parameter names match in any case. Only the final path component is kept,
/// and a name that is empty or only dots yields `None`.
///
/// # Examples
///
/// ```
/// use ureq::http::HeaderValue;
/// use soar_dl::utils;
///
/// let header = HeaderValue::from_static("attachment; filename=\"example.txt\"");
/// assert_eq!(utils::filename_from_header(&header), Some("example.txt".to_string()));
///
/// let header = HeaderValue::from_static("attachment; FILENAME*=UTF-8''na%C3%AFve.txt");
/// assert_eq!(utils::filename_from_header(&header), Some("na\u{ef}ve.txt".to_string()));
/// ```
pub fn filename_from_header(value: &HeaderValue) -> Option<String> {
    let mut plain = None;
    let mut extended = None;
    for param in value.to_str().ok()?.split(';') {
        let Some((key, value)) = param.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim().trim_matches('"');
        if key.eq_ignore_ascii_case("filename*") {
            extended = value.splitn(3, '\'').nth(2).and_then(|encoded| {
                percent_decode_str(encoded)
                    .decode_utf8()
                    .ok()
                    .map(|name| name.into_owned())
            });
        } else if key.eq_ignore_ascii_case("filename") {
            plain = Some(value.to_string());
        }
    }

    let name = extended.or(plain)?;
    let name = name.rsplit(['/', '\\']).next().unwrap_or_default();
    (!name.trim_matches('.').is_empty()).then(|| name.to_string())
}

/// Opens a download's output at `path` for writing, refusing to follow a
/// symlink planted there or to write into anything but a regular file.
///
/// With `resume_pos`, the existing file is cut back to that offset, dropping
/// whatever was written past the last checkpoint. A file shorter than the
/// offset no longer matches its resume state and yields
/// [`DownloadError::ResumeMismatch`]. Without it the file is created or
/// truncated.
pub(crate) fn open_output(path: &Path, resume_pos: Option<u64>) -> Result<File, DownloadError> {
    let mut options = OpenOptions::new();
    options.write(true).custom_flags(libc::O_NOFOLLOW);
    if resume_pos.is_none() {
        options.create(true).truncate(true);
    }

    let mut file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(
            std::io::Error::other(format!("{} is not a regular file", path.display())).into(),
        );
    }

    if let Some(pos) = resume_pos {
        if metadata.len() < pos {
            return Err(DownloadError::ResumeMismatch);
        }
        file.set_len(pos)?;
        file.seek(SeekFrom::End(0))?;
    }
    Ok(file)
}

/// Compute the final file system path for a download based on an explicit output and inferred filenames.
///
/// The `output` argument can be:
/// - `Some("-")` to represent writing to stdout (path is `"-"`).
/// - A string ending with `'/'` to indicate a directory; a filename is chosen from `header_filename` or `url_filename`.
/// - A path that is an existing directory, in which case a filename is chosen from `header_filename` or `url_filename`.
/// - A file path (returned as-is).
///
/// If `output` is `None`, a filename is required from `header_filename` or
///
/// # Returns
///
/// `Ok(PathBuf)` with the resolved path, or `Err(DownloadError::NoFilename)` if a filename is required but neither `header_filename` nor `url_filename` is available.
///
/// # Examples
///
/// ```
/// use std::path::PathBuf;
/// use soar_dl::error::DownloadError;
/// use soar_dl::utils::resolve_output_path;
///
/// // explicit directory with trailing slash prefers header filename
/// let out = resolve_output_path(Some("downloads/"), Some("from_url.txt".into()), Some("from_header.txt".into())).unwrap();
/// assert_eq!(out, PathBuf::from("downloads").join("from_header.txt"));
///
/// // stdout shortcut
/// let out = resolve_output_path(Some("-"), None, None).unwrap();
/// assert_eq!(out, PathBuf::from("-"));
///
/// // no output given uses header or url filename
/// let out = resolve_output_path(None, Some("a.txt".into()), None).unwrap();
/// assert_eq!(out, PathBuf::from("a.txt"));
///
/// // error when no filename available
/// let _ = resolve_output_path(None, None, None).unwrap_err();
/// ```
pub fn resolve_output_path(
    output: Option<&str>,
    url_filename: Option<String>,
    header_filename: Option<String>,
) -> Result<PathBuf, DownloadError> {
    let filename = || header_filename.or(url_filename);

    match output {
        Some("-") => Ok(PathBuf::from("-")),
        Some(p) => {
            let path = PathBuf::from(p);
            if p.ends_with('/') || path.is_dir() {
                Ok(path.join(filename().ok_or(DownloadError::NoFilename)?))
            } else {
                Ok(path)
            }
        }
        None => {
            filename()
                .map(PathBuf::from)
                .ok_or(DownloadError::NoFilename)
        }
    }
}

#[cfg(test)]
mod tests {
    use ureq::http::HeaderValue;

    use super::*;

    #[test]
    fn test_filename_from_url_simple() {
        assert_eq!(
            filename_from_url("https://example.com/file.txt"),
            Some("file.txt".to_string())
        );
        assert_eq!(
            filename_from_url("https://example.com/path/to/archive.tar.gz"),
            Some("archive.tar.gz".to_string())
        );
    }

    #[test]
    fn test_filename_from_url_trailing_slash() {
        assert_eq!(filename_from_url("https://example.com/path/"), None);
        assert_eq!(filename_from_url("https://example.com/"), None);
    }

    #[test]
    fn test_filename_from_url_no_path() {
        assert_eq!(filename_from_url("https://example.com"), None);
    }

    #[test]
    fn test_filename_from_url_invalid() {
        assert_eq!(filename_from_url("not a url"), None);
        assert_eq!(filename_from_url(""), None);
    }

    #[test]
    fn test_filename_from_url_percent_encoded() {
        assert_eq!(
            filename_from_url("https://example.com/hello%20world.txt"),
            Some("hello world.txt".to_string())
        );
        assert_eq!(
            filename_from_url("https://example.com/file%2Bname.tar.gz"),
            Some("file+name.tar.gz".to_string())
        );
    }

    #[test]
    fn test_filename_from_url_query_params() {
        assert_eq!(
            filename_from_url("https://example.com/file.txt?version=1"),
            Some("file.txt".to_string())
        );
    }

    #[test]
    fn test_filename_from_url_fragment() {
        assert_eq!(
            filename_from_url("https://example.com/file.txt#section"),
            Some("file.txt".to_string())
        );
    }

    #[test]
    fn test_filename_from_header_simple() {
        let header = HeaderValue::from_static("attachment; filename=\"example.txt\"");
        assert_eq!(
            filename_from_header(&header),
            Some("example.txt".to_string())
        );
    }

    #[test]
    fn test_filename_from_header_no_quotes() {
        let header = HeaderValue::from_static("attachment; filename=example.txt");
        assert_eq!(
            filename_from_header(&header),
            Some("example.txt".to_string())
        );
    }

    #[test]
    fn test_filename_from_header_with_path() {
        let header = HeaderValue::from_static("attachment; filename=\"/path/to/file.txt\"");
        assert_eq!(filename_from_header(&header), Some("file.txt".to_string()));

        let header = HeaderValue::from_static("attachment; filename=\"path\\to\\file.txt\"");
        assert_eq!(filename_from_header(&header), Some("file.txt".to_string()));
    }

    #[test]
    fn test_filename_from_header_multiple_params() {
        let header =
            HeaderValue::from_static("inline; name=value; filename=\"test.pdf\"; size=1024");
        assert_eq!(filename_from_header(&header), Some("test.pdf".to_string()));
    }

    #[test]
    fn test_filename_from_header_no_filename() {
        let header = HeaderValue::from_static("attachment");
        assert_eq!(filename_from_header(&header), None);

        let header = HeaderValue::from_static("inline; name=value");
        assert_eq!(filename_from_header(&header), None);
    }

    #[test]
    fn test_filename_from_header_rejects_names_that_are_not_files() {
        for header in [
            "attachment; filename=\"\"",
            "attachment; filename=\"..\"",
            "attachment; filename=\"a/..\"",
            "attachment; filename=\".\"",
        ] {
            let header = HeaderValue::from_str(header).unwrap();
            assert_eq!(filename_from_header(&header), None, "{header:?}");
        }
    }

    #[test]
    fn test_filename_from_header_prefers_extended_name() {
        let header = HeaderValue::from_static(
            "attachment; filename=\"fallback.txt\"; filename*=UTF-8''real%20name.txt",
        );
        assert_eq!(
            filename_from_header(&header),
            Some("real name.txt".to_string())
        );

        let header = HeaderValue::from_static("attachment; FileName=\"Mixed.txt\"");
        assert_eq!(filename_from_header(&header), Some("Mixed.txt".to_string()));

        let header = HeaderValue::from_static("attachment; filename*=UTF-8''..%2F..%2Fevil");
        assert_eq!(filename_from_header(&header), Some("evil".to_string()));
    }

    #[test]
    fn test_open_output_refuses_a_planted_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        let output = dir.path().join("output");
        std::os::unix::fs::symlink(&target, &output).unwrap();

        assert!(open_output(&output, None).is_err());
        assert!(!target.exists(), "the symlink was followed");

        std::fs::write(&target, b"keep").unwrap();
        assert!(open_output(&output, None).is_err());
        assert!(open_output(&output, Some(0)).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"keep");
    }

    #[test]
    fn test_open_output_rejects_resume_past_end() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("output");
        std::fs::write(&output, b"abcdef").unwrap();

        assert!(matches!(
            open_output(&output, Some(100)),
            Err(DownloadError::ResumeMismatch)
        ));
        assert_eq!(std::fs::read(&output).unwrap(), b"abcdef");

        open_output(&output, Some(3)).unwrap();
        assert_eq!(std::fs::read(&output).unwrap(), b"abc");
    }

    #[test]
    fn test_resolve_output_path_stdout() {
        let result = resolve_output_path(Some("-"), None, None).unwrap();
        assert_eq!(result, PathBuf::from("-"));
    }

    #[test]
    fn test_resolve_output_path_trailing_slash() {
        let result = resolve_output_path(
            Some("downloads/"),
            Some("from_url.txt".into()),
            Some("from_header.txt".into()),
        )
        .unwrap();
        // Should prefer header filename
        assert_eq!(result, PathBuf::from("downloads/from_header.txt"));
    }

    #[test]
    fn test_resolve_output_path_trailing_slash_no_header() {
        let result =
            resolve_output_path(Some("downloads/"), Some("from_url.txt".into()), None).unwrap();
        assert_eq!(result, PathBuf::from("downloads/from_url.txt"));
    }

    #[test]
    fn test_resolve_output_path_trailing_slash_no_filenames() {
        let result = resolve_output_path(Some("downloads/"), None, None);
        assert!(matches!(result, Err(DownloadError::NoFilename)));
    }

    #[test]
    fn test_resolve_output_path_explicit_file() {
        let result = resolve_output_path(
            Some("output.txt"),
            Some("url.txt".into()),
            Some("header.txt".into()),
        )
        .unwrap();
        assert_eq!(result, PathBuf::from("output.txt"));
    }

    #[test]
    fn test_resolve_output_path_none_uses_header() {
        let result = resolve_output_path(
            None,
            Some("from_url.txt".into()),
            Some("from_header.txt".into()),
        )
        .unwrap();
        assert_eq!(result, PathBuf::from("from_header.txt"));
    }

    #[test]
    fn test_resolve_output_path_none_uses_url() {
        let result = resolve_output_path(None, Some("from_url.txt".into()), None).unwrap();
        assert_eq!(result, PathBuf::from("from_url.txt"));
    }

    #[test]
    fn test_resolve_output_path_none_no_filenames() {
        let result = resolve_output_path(None, None, None);
        assert!(matches!(result, Err(DownloadError::NoFilename)));
    }

    #[test]
    fn test_resolve_output_path_with_subdirectories() {
        let result = resolve_output_path(Some("path/to/"), Some("file.txt".into()), None).unwrap();
        assert_eq!(result, PathBuf::from("path/to/file.txt"));
    }
}
