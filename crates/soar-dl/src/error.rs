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

    #[error("Network request failed")]
    #[diagnostic(
        code(soar_dl::network),
        help("Check your internet connection or try again later")
    )]
    Network(#[from] Box<ureq::Error>),

    #[error("HTTP {status}: {url}")]
    #[diagnostic(code(soar_dl::http_error))]
    HttpError { status: u16, url: String },

    #[error("I/O error: {0}")]
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
    fn from(e: ureq::Error) -> Self {
        Self::Network(Box::new(e))
    }
}
