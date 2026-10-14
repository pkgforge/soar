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

    #[error("{}", describe_request_error(err, url))]
    #[diagnostic(
        code(soar_dl::network),
        help("Check your internet connection or try again later")
    )]
    Network { url: String, err: Box<ureq::Error> },

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

    #[error("Unsafe layer path in manifest: {title}")]
    #[diagnostic(code(soar_dl::unsafe_layer_path))]
    UnsafeLayerPath { title: String },

    #[error("Checksum mismatch: expected {expected}, got {got}")]
    #[diagnostic(code(soar_dl::checksum_mismatch))]
    ChecksumMismatch { expected: String, got: String },

    #[error("Digest mismatch: expected {expected}, got {got}")]
    #[diagnostic(code(soar_dl::digest_mismatch))]
    DigestMismatch { expected: String, got: String },

    #[error("Invalid response from server")]
    #[diagnostic(code(soar_dl::invalid_response))]
    InvalidResponse,

    #[error("Invalid JSON from {url}: {reason}")]
    #[diagnostic(code(soar_dl::invalid_json))]
    InvalidJson { url: String, reason: String },

    #[error("No release found{}", .tag.as_deref().map(|t| format!(" for tag {t}")).unwrap_or_default())]
    #[diagnostic(
        code(soar_dl::release_not_found),
        help("Check the project name, or omit the tag to take the latest release")
    )]
    ReleaseNotFound { tag: Option<String> },

    #[error("File name could not be determined")]
    #[diagnostic(
        code(soar_dl::no_filename),
        help("Try specifying an output path explicitly")
    )]
    NoFilename,

    #[error("A checksum cannot be verified on output streamed to stdout")]
    #[diagnostic(
        code(soar_dl::checksum_on_stdout),
        help("Download to a file to have it verified")
    )]
    ChecksumOnStdout,

    #[error("Resume metadata mismatch")]
    #[diagnostic(code(soar_dl::resume_mismatch))]
    ResumeMismatch,

    /// Several downloads failed. `transient` holds when every one of them
    /// failed in a way a retry may get past.
    #[error("{}", .errors.join("; "))]
    #[diagnostic(code(soar_dl::multiple_errors))]
    Multiple {
        errors: Vec<String>,
        transient: bool,
    },

    #[error("{0}")]
    #[diagnostic(
        code(soar_dl::forge_request),
        help("Check your internet connection or try again later")
    )]
    ForgeRequest(String),

    #[error("zsync: {0}")]
    #[diagnostic(
        code(soar_dl::zsync),
        help("The publisher's zsync feed may be stale; a plain download still works")
    )]
    Zsync(String),
}

pub type Result<T> = miette::Result<T>;

impl DownloadError {
    /// A request to `url` that failed, as either the status the server refused
    /// it with or the network error that kept any response from coming back.
    pub fn request(url: &str, err: ureq::Error) -> Self {
        match err {
            ureq::Error::StatusCode(status) => {
                Self::HttpError {
                    status,
                    url: url.to_string(),
                }
            }
            err => {
                Self::Network {
                    url: url.to_string(),
                    err: Box::new(err),
                }
            }
        }
    }

    /// A response body from `url` that did not parse as the JSON expected.
    pub fn json(url: &str, err: impl std::fmt::Display) -> Self {
        Self::InvalidJson {
            url: url.to_string(),
            reason: err.to_string(),
        }
    }

    /// Whether trying the same request again later may succeed: the network
    /// failed, before or during the transfer, the server asked to be given
    /// time, or it failed on its side.
    ///
    /// A body read that fails surfaces as `Io`. ureq hands its own errors
    /// there wrapped in an `io::Error`, the stall timeout among them, so a
    /// wrapped ureq error counts as a network failure.
    pub fn is_transient(&self) -> bool {
        match self {
            Self::Network {
                ..
            } => true,
            Self::HttpError {
                status, ..
            } => *status == 429 || *status >= 500,
            Self::Io(err) => {
                err.get_ref().is_some_and(|inner| inner.is::<ureq::Error>())
                    || matches!(
                        err.kind(),
                        std::io::ErrorKind::TimedOut
                            | std::io::ErrorKind::ConnectionReset
                            | std::io::ErrorKind::ConnectionAborted
                            | std::io::ErrorKind::UnexpectedEof
                            | std::io::ErrorKind::BrokenPipe
                    )
            }
            Self::Multiple {
                transient, ..
            } => *transient,
            _ => false,
        }
    }
}

/// Describes a failed request, naming the host when the failure was a DNS
/// lookup, since the host a request goes to is rarely the one users test by
/// hand.
pub fn describe_request_error(err: &ureq::Error, url: &str) -> String {
    let reason = match err {
        ureq::Error::HostNotFound => "no address found".to_string(),
        ureq::Error::Io(e) => {
            match e
                .to_string()
                .strip_prefix("failed to lookup address information: ")
            {
                Some(reason) => reason.to_string(),
                None => return err.to_string(),
            }
        }
        _ => return err.to_string(),
    };

    let host = url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_else(|| url.to_string());

    format!("could not resolve {host} ({reason}); check your DNS settings")
}

impl From<releasekit::Error> for DownloadError {
    /// A forge error reported as the download error closest to it.
    ///
    /// The status of a refused request is what tells a rate limit apart from a
    /// project that does not exist, so it is kept rather than flattened into a
    /// message.
    fn from(err: releasekit::Error) -> Self {
        match err {
            releasekit::Error::Http {
                status,
                url,
            } => {
                Self::HttpError {
                    status,
                    url,
                }
            }
            other => Self::ForgeRequest(other.to_string()),
        }
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
            transient: false,
        };
        let msg = format!("{}", err);
        assert_eq!(msg, "Error 1; Error 2");
    }

    #[test]
    fn test_download_error_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err = DownloadError::Io(io_err);
        let msg = format!("{}", err);
        assert!(msg.contains("file not found"));
    }

    #[test]
    fn test_download_error_debug() {
        let err = DownloadError::LayerNotFound;
        let debug = format!("{:?}", err);
        assert!(debug.contains("LayerNotFound"));
    }

    #[test]
    fn a_refused_request_is_an_http_error() {
        let err = DownloadError::request("https://ghcr.io/v2/x", ureq::Error::StatusCode(404));
        assert!(matches!(
            err,
            DownloadError::HttpError {
                status: 404,
                ..
            }
        ));
        assert!(!err.is_transient());

        let err = DownloadError::request("https://ghcr.io/v2/x", ureq::Error::ConnectionFailed);
        assert!(matches!(err, DownloadError::Network { .. }));
        assert!(err.is_transient());
    }

    #[test]
    fn transient_statuses_are_retried() {
        for (status, transient) in [
            (429, true),
            (500, true),
            (503, true),
            (403, false),
            (416, false),
        ] {
            let err = DownloadError::HttpError {
                status,
                url: String::new(),
            };
            assert_eq!(err.is_transient(), transient, "status {status}");
        }
    }

    #[test]
    fn a_failed_body_read_is_transient() {
        let stalled = DownloadError::Io(ureq::Error::ConnectionFailed.into_io());
        assert!(stalled.is_transient());

        let reset = std::io::Error::from(std::io::ErrorKind::ConnectionReset);
        assert!(DownloadError::Io(reset).is_transient());

        let disk = std::io::Error::from(std::io::ErrorKind::StorageFull);
        assert!(!DownloadError::Io(disk).is_transient());
    }

    #[test]
    fn invalid_json_carries_its_cause() {
        let cause = serde_json::from_str::<serde_json::Value>("{\"layers\": [").unwrap_err();
        let msg = DownloadError::json("https://ghcr.io/v2/m", &cause).to_string();
        assert!(msg.contains("https://ghcr.io/v2/m"), "{msg}");
        assert!(msg.contains("EOF"), "{msg}");
    }

    #[test]
    fn a_failed_lookup_names_the_host() {
        let err = ureq::Error::Io(std::io::Error::other(
            "failed to lookup address information: Try again",
        ));
        assert_eq!(
            DownloadError::request("https://api.github.com/repos/o/r/releases", err).to_string(),
            "could not resolve api.github.com (Try again); check your DNS settings"
        );
        assert_eq!(
            describe_request_error(&ureq::Error::HostNotFound, "https://codeberg.org/api"),
            "could not resolve codeberg.org (no address found); check your DNS settings"
        );
        assert_eq!(
            describe_request_error(&ureq::Error::ConnectionFailed, "https://codeberg.org"),
            "connection failed"
        );
    }

    #[test]
    fn test_error_source_chain() {
        let err = DownloadError::InvalidUrl {
            url: "invalid".to_string(),
            source: url::ParseError::RelativeUrlWithoutBase,
        };

        // Check that we can get the source
        assert!(std::error::Error::source(&err).is_some());
    }
}
