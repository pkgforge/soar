//! Remote package update utilities.

/// Validate that a download URL is properly formed.
///
/// Checks that the URL:
/// - Starts with http:// or https://
/// - Is a valid URL structure (has host, etc.)
pub fn is_valid_download_url(url: &str) -> bool {
    let url = url.trim();
    if url.is_empty() {
        return false;
    }

    let lower = url.to_lowercase();
    if !lower.starts_with("http://") && !lower.starts_with("https://") {
        return false;
    }

    match url::Url::parse(url) {
        Ok(parsed) => parsed.host().is_some(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_valid_download_url() {
        // Valid URLs
        assert!(is_valid_download_url("https://example.com/file.AppImage"));
        assert!(is_valid_download_url("http://example.com/file"));
        assert!(is_valid_download_url(
            "https://github.com/user/repo/releases/download/v1.0/app.zip"
        ));

        // Invalid URLs
        assert!(!is_valid_download_url("")); // empty
        assert!(!is_valid_download_url("   ")); // whitespace only
        assert!(!is_valid_download_url("https://")); // no host
        assert!(!is_valid_download_url("https://?query=1")); // no host
        assert!(!is_valid_download_url("not-a-url")); // no protocol
        assert!(!is_valid_download_url("ftp://example.com/file")); // wrong protocol
        assert!(!is_valid_download_url("file:///path/to/file")); // file protocol
    }
}
