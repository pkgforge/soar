use tracing::{debug, trace};
use ureq::{
    http::{
        header::{CONTENT_LENGTH, CONTENT_TYPE, LOCATION},
        Response,
    },
    Body,
};

use crate::{error::DownloadError, http_client::SHARED_AGENT};

pub struct Http;

impl Http {
    pub fn head(url: &str) -> Result<Response<Body>, DownloadError> {
        trace!(url = url, "sending HEAD request");
        let result = SHARED_AGENT.head(url).call().map_err(DownloadError::from);
        if let Ok(ref resp) = result {
            trace!(status = resp.status().as_u16(), "HEAD response received");
            Self::log_response_headers(resp, "HEAD");
        }
        result
    }

    fn log_response_headers(resp: &Response<Body>, method: &str) {
        let status = resp.status();
        let headers = resp.headers();

        debug!(
            "{} {} {}",
            method,
            status.as_u16(),
            status.canonical_reason().unwrap_or("")
        );

        if let Some(content_length) = headers.get(CONTENT_LENGTH) {
            if let Ok(len) = content_length.to_str() {
                trace!("  Content-Length: {}", len);
            }
        }
        if let Some(content_type) = headers.get(CONTENT_TYPE) {
            if let Ok(ct) = content_type.to_str() {
                trace!("  Content-Type: {}", ct);
            }
        }
        if let Some(location) = headers.get(LOCATION) {
            if let Ok(loc) = location.to_str() {
                debug!("  Location: {}", loc);
            }
        }
    }

    /// Fetches a GET response for the given URL, optionally requesting a byte range and using an ETag for conditional requests.
    ///
    /// If `resume_from` is `Some(pos)`, the request includes a `Range: bytes={pos}-` header. If `etag` is `Some(tag)` and a range is requested,
    /// the request also includes an `If-Range: {tag}` header.
    ///
    /// # Returns
    ///
    /// `Ok(Response<Body>)` with the HTTP response on success, `Err(DownloadError)` on failure.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use soar_dl::http::Http;
    ///
    /// let resp = Http::fetch("https://example.com/resource", Some(1024), Some("\"etag-value\""), false);
    /// match resp {
    ///     Ok(r) => {
    ///         assert!(r.status().as_u16() < 600); // got a response
    ///     }
    ///     Err(e) => panic!("request failed: {:?}", e),
    /// }
    /// ```
    pub fn fetch(
        url: &str,
        resume_from: Option<u64>,
        etag: Option<&str>,
        ghcr_blob: bool,
    ) -> Result<Response<Body>, DownloadError> {
        debug!("GET {}", url);
        trace!(resume_from = ?resume_from, ghcr_blob = ghcr_blob, "request details");
        let mut req = SHARED_AGENT.get(url);

        if ghcr_blob {
            trace!("adding GHCR authorization header");
            req = req.header("Authorization", "Bearer QQ==");
        }

        if let Some(pos) = resume_from {
            debug!("  Range: bytes={}-", pos);
            req = req.header("Range", &format!("bytes={}-", pos));
            if let Some(tag) = etag {
                trace!(etag = tag, "adding If-Range header");
                req = req.header("If-Range", tag);
            }
        }

        let result = req.call().map_err(DownloadError::from);
        if let Ok(ref resp) = result {
            Self::log_response_headers(resp, "GET");
        }
        result
    }

    /// Fetches JSON from the given URL and deserializes it into `T`.
    ///
    /// Performs an HTTP GET request to `url` and deserializes the response body into the requested type.
    ///
    /// # Returns
    ///
    /// `T` parsed from the response body on success, `DownloadError` on failure.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use serde_json::Value;
    /// use soar_dl::http::Http;
    ///
    /// let json: Value = Http::json("https://example.com/data.json").unwrap();
    /// ```
    pub fn json<T: serde::de::DeserializeOwned>(url: &str) -> Result<T, DownloadError> {
        debug!(url = url, "fetching JSON");
        let result = SHARED_AGENT
            .get(url)
            .call()?
            .body_mut()
            .read_json()
            .map_err(|_| DownloadError::InvalidResponse);
        if result.is_ok() {
            trace!(url = url, "JSON parsed successfully");
        }
        result
    }
}
