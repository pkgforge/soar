use miette::Diagnostic;
use thiserror::Error;

#[derive(Error, Diagnostic, Debug)]
pub enum DownloadError {
    #[error("Invalid URL: {url}")]
    #[diagnostic(code(soar_dl::invalid_url))]
    InvalidUrl {
        url: String,
        #[source]
        source: url::ParseError,
    },

    #[error(transparent)]
    #[diagnostic(code(soar_dl::extract_error))]
    ExtractError(#[from] compak::error::ArchiveError),

    #[error(transparent)]
    #[diagnostic(
        code(soar_dl::network),
        help("Check your internet connection or try again later")
    )]
    Network(#[from] Box<ureq::Error>),

    #[error("HTTP {status}: {url}")]
    #[diagnostic(code(soar_dl::http_error))]
    HttpError { status: u16, url: String },

    #[error(transparent)]
    #[diagnostic(code(soar_dl::io))]
    Io(#[from] std::io::Error),

    #[error("No matching assets found")]
    #[diagnostic(
        code(soar_dl::no_match),
        help("Available assets:\n{}", .available.join("\n"))
    )]
    NoMatch { available: Vec<String> },

    #[error("Layer not found")]
    #[diagnostic(code(soar_dl::layer_not_found))]
    LayerNotFound,

    #[error("Invalid response from server")]
    #[diagnostic(code(soar_dl::invalid_response))]
    InvalidResponse,

    #[error("File name could not be determined")]
    #[diagnostic(
        code(soar_dl::no_filename),
        help("Try specifying an output path explicitly")
    )]
    NoFilename,

    #[error("Resume metadata mismatch")]
    #[diagnostic(code(soar_dl::resume_mismatch))]
    ResumeMismatch,

    #[error("Multiple download errors occurred")]
    #[diagnostic(code(soar_dl::multiple_errors))]
    Multiple { errors: Vec<String> },
}

pub type Result<T> = miette::Result<T>;

impl From<ureq::Error> for DownloadError {
    /// Converts a `ureq::Error` into a `DownloadError::Network` variant.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use soar_dl::error::DownloadError;
    ///
    /// // Given a `ureq::Error` `e`, convert it into a `DownloadError`
    /// let e: ureq::Error = /* obtained from a ureq request */ unimplemented!();
    /// let err: DownloadError = DownloadError::from(e);
    /// match err {
    ///     DownloadError::Network(_) => (),
    ///     _ => panic!("expected DownloadError::Network"),
    /// }
    /// ```
    fn from(e: ureq::Error) -> Self {
        Self::Network(Box::new(e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_download_error_invalid_url() {
        let err = DownloadError::InvalidUrl {
            url: "invalid".to_string(),
            source: url::ParseError::RelativeUrlWithoutBase,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("Invalid URL"));
        assert!(msg.contains("invalid"));
    }

    #[test]
    fn test_download_error_http_error() {
        let err = DownloadError::HttpError {
            status: 404,
            url: "https://example.com/notfound".to_string(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("HTTP 404"));
        assert!(msg.contains("https://example.com/notfound"));
    }

    #[test]
    fn test_download_error_no_match() {
        let err = DownloadError::NoMatch {
            available: vec!["file1.zip".to_string(), "file2.tar.gz".to_string()],
        };
        let msg = format!("{}", err);
        assert!(msg.contains("No matching assets found"));
    }

    #[test]
    fn test_download_error_layer_not_found() {
        let err = DownloadError::LayerNotFound;
        let msg = format!("{}", err);
        assert_eq!(msg, "Layer not found");
    }

    #[test]
    fn test_download_error_invalid_response() {
        let err = DownloadError::InvalidResponse;
        let msg = format!("{}", err);
        assert_eq!(msg, "Invalid response from server");
    }

    #[test]
    fn test_download_error_no_filename() {
        let err = DownloadError::NoFilename;
        let msg = format!("{}", err);
        assert_eq!(msg, "File name could not be determined");
    }

    #[test]
    fn test_download_error_resume_mismatch() {
        let err = DownloadError::ResumeMismatch;
        let msg = format!("{}", err);
        assert_eq!(msg, "Resume metadata mismatch");
    }

    #[test]
    fn test_download_error_multiple() {
        let err = DownloadError::Multiple {
            errors: vec!["Error 1".to_string(), "Error 2".to_string()],
        };
        let msg = format!("{}", err);
        assert_eq!(msg, "Multiple download errors occurred");
    }

    #[test]
    fn test_download_error_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err = DownloadError::Io(io_err);
        let msg = format!("{}", err);
        assert!(msg.contains("I/O error"));
    }

    #[test]
    fn test_download_error_debug() {
        let err = DownloadError::LayerNotFound;
        let debug = format!("{:?}", err);
        assert!(debug.contains("LayerNotFound"));
    }

    #[test]
    fn test_from_ureq_error() {
        let ureq_err = ureq::Error::ConnectionFailed;
        let download_err: DownloadError = ureq_err.into();

        match download_err {
            DownloadError::Network(_) => (),
            _ => panic!("Expected Network error variant"),
        }
    }

    #[test]
    fn test_error_source_chain() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err = DownloadError::Io(io_err);

        // Check that we can get the source
        assert!(std::error::Error::source(&err).is_some());
    }
}
