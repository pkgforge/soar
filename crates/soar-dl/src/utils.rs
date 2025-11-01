use std::path::PathBuf;

use percent_encoding::percent_decode_str;
use ureq::http::HeaderValue;
use url::Url;

use crate::error::DownloadError;

/// Extract filename from URL path
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

/// Extract filename from Content-Disposition header
pub fn filename_from_header(value: &HeaderValue) -> Option<String> {
    value
        .to_str()
        .ok()?
        .split(';')
        .find_map(|p| p.trim().strip_prefix("filename="))
        .map(|s| s.trim_matches('"').to_string())
        .map(|s| {
            s.split(['/', '\\'])
                .next_back()
                .map(String::from)
                .unwrap_or(s)
        })
}

/// Determine output path
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
