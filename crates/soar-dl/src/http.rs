use ureq::{http::Response, Body};

use crate::{error::DownloadError, http_client::SHARED_AGENT};

pub struct Http;

impl Http {
    pub fn head(url: &str) -> Result<Response<Body>, DownloadError> {
        SHARED_AGENT.head(url).call().map_err(DownloadError::from)
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
    /// let resp = Http::fetch("https://example.com/resource", Some(1024), Some("\"etag-value\""));
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
    ) -> Result<Response<Body>, DownloadError> {
        let mut req = SHARED_AGENT.get(url);

        if let Some(pos) = resume_from {
            req = req.header("Range", &format!("bytes={}-", pos));
            if let Some(tag) = etag {
                req = req.header("If-Range", tag);
            }
        }

        req.call().map_err(DownloadError::from)
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
        SHARED_AGENT
            .get(url)
            .call()?
            .body_mut()
            .read_json()
            .map_err(|_| DownloadError::InvalidResponse)
    }
}
