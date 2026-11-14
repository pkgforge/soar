use tracing::{debug, trace};
use ureq::{
    Body,
    http::{
        Response,
        header::{
            ACCEPT_ENCODING, ACCEPT_RANGES, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, ETAG,
            LOCATION,
        },
    },
};

use crate::{error::DownloadError, http_client::SHARED_AGENT};

pub struct Http;

/// Asks for the bytes as stored. ureq otherwise offers gzip and decodes it,
/// while sizes and byte ranges count the coded bytes, so a compressing server
/// would hand back a size and ranges that do not match what is written.
const IDENTITY: &str = "identity";

/// What a HEAD response says about the resource a segmented download would
/// target: its size, whether it can be fetched in byte ranges, and the ETag a
/// later ranged GET can pin itself to.
#[derive(Debug, Clone, Default)]
pub(crate) struct RemoteInfo {
    pub size: Option<u64>,
    pub ranges_ok: bool,
    pub etag: Option<String>,
}

impl RemoteInfo {
    pub(crate) fn from_head(resp: &Response<Body>) -> Self {
        let headers = resp.headers();
        Self {
            size: content_length_header(headers),
            ranges_ok: headers
                .get(ACCEPT_RANGES)
                .and_then(|h| h.to_str().ok())
                .is_some_and(|v| v.split(',').any(|u| u.trim().eq_ignore_ascii_case("bytes"))),
            etag: headers
                .get(ETAG)
                .and_then(|h| h.to_str().ok())
                .map(String::from),
        }
    }
}

/// Returns a valid strong entity tag suitable for an If-Range request.
pub(crate) fn strong_etag(tag: &str) -> Option<&str> {
    let bytes = tag.as_bytes();
    if bytes.len() >= 2
        && bytes[0] == b'"'
        && bytes[bytes.len() - 1] == b'"'
        && bytes[1..bytes.len() - 1]
            .iter()
            .all(|&byte| byte == 0x21 || (0x23..=0x7e).contains(&byte) || byte >= 0x80)
    {
        Some(tag)
    } else {
        None
    }
}

fn content_length_header(headers: &ureq::http::HeaderMap) -> Option<u64> {
    headers
        .get(CONTENT_LENGTH)
        .and_then(|h| h.to_str().ok())
        .and_then(|len| len.parse().ok())
}

impl Http {
    pub fn head(url: &str) -> Result<Response<Body>, DownloadError> {
        trace!(url = url, "sending HEAD request");
        let result = SHARED_AGENT
            .head(url)
            .header(ACCEPT_ENCODING, IDENTITY)
            .call()
            .map_err(|err| DownloadError::request(url, err));
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

        if let Some(content_length) = headers.get(CONTENT_LENGTH)
            && let Ok(len) = content_length.to_str()
        {
            trace!("  Content-Length: {}", len);
        }
        if let Some(content_type) = headers.get(CONTENT_TYPE)
            && let Ok(ct) = content_type.to_str()
        {
            trace!("  Content-Type: {}", ct);
        }
        if let Some(location) = headers.get(LOCATION)
            && let Ok(loc) = location.to_str()
        {
            debug!("  Location: {}", loc);
        }
    }

    /// Fetches a GET response for the given URL, optionally requesting a byte range and using an ETag for conditional requests.
    ///
    /// If `resume_from` is `Some(pos)`, the request includes a `Range: bytes={pos}-` header. If `etag` is `Some(tag)` and a range is requested,
    /// the request also includes an `If-Range: {tag}` header when the tag is a valid strong ETag.
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
        let mut req = SHARED_AGENT.get(url).header(ACCEPT_ENCODING, IDENTITY);

        if ghcr_blob {
            trace!("adding GHCR authorization header");
            req = req.header("Authorization", "Bearer QQ==");
        }

        if let Some(pos) = resume_from {
            debug!("  Range: bytes={}-", pos);
            req = req.header("Range", &format!("bytes={}-", pos));
            if let Some(tag) = etag.and_then(strong_etag) {
                trace!(etag = tag, "adding If-Range header");
                req = req.header("If-Range", tag);
            }
        }

        let result = req.call().map_err(|err| DownloadError::request(url, err));
        if let Ok(ref resp) = result {
            Self::log_response_headers(resp, "GET");
        }
        result
    }

    /// Fetches a closed byte range of the resource at `url`, optionally pinning
    /// the request to the resource version an ETag names.
    ///
    /// The request carries `Range: bytes={start}-{end}` with both ends
    /// inclusive. A server that answers `200` ignored the range; callers
    /// surface that as [`DownloadError::RangeUnsupported`] so the download can
    /// fall back to a single stream. HTTP error statuses are returned as
    /// responses so callers can inspect retry headers.
    pub(crate) fn fetch_range(
        url: &str,
        start: u64,
        end: u64,
        etag: Option<&str>,
        ghcr_blob: bool,
    ) -> Result<Response<Body>, DownloadError> {
        debug!("GET {url} (Range: bytes={start}-{end})");
        trace!(etag = ?etag, ghcr_blob = ghcr_blob, "range request details");
        let mut req = SHARED_AGENT.get(url).header(ACCEPT_ENCODING, IDENTITY);

        if ghcr_blob {
            trace!("adding GHCR authorization header");
            req = req.header("Authorization", "Bearer QQ==");
        }

        req = req.header("Range", &format!("bytes={start}-{end}"));
        if let Some(tag) = etag.and_then(strong_etag) {
            trace!(etag = tag, "adding If-Range header");
            req = req.header("If-Range", tag);
        }

        let result = req
            .config()
            .http_status_as_error(false)
            .build()
            .call()
            .map_err(|err| DownloadError::request(url, err));
        if let Ok(ref resp) = result {
            Self::log_response_headers(resp, "GET");
        }
        result
    }

    /// Asks the server directly whether it serves byte ranges, with a
    /// one-byte range GET.
    ///
    /// Some servers refuse HEAD outright and others answer it without
    /// `Accept-Ranges` while still honoring `Range` on GET, so the probe is
    /// what a segmented download falls back to when HEAD says nothing. The
    /// `206`'s `Content-Range` names the resource's full size and its ETag
    /// pins later range requests to this version. `None` when the server
    /// gives no usable answer; the caller then downloads in one stream.
    pub(crate) fn probe_range(url: &str, ghcr_blob: bool) -> Option<RemoteInfo> {
        debug!("probing range support: GET {url} (Range: bytes=0-0)");
        let mut req = SHARED_AGENT.get(url).header(ACCEPT_ENCODING, IDENTITY);
        if ghcr_blob {
            req = req.header("Authorization", "Bearer QQ==");
        }
        req = req.header("Range", "bytes=0-0");

        let resp = match req.call() {
            Ok(resp) => resp,
            Err(err) => {
                debug!(%err, "range probe request failed");
                return None;
            }
        };
        if resp.status() != 206 {
            debug!(
                status = resp.status().as_u16(),
                "range probe was not answered with a partial response"
            );
            return None;
        }
        let total = resp
            .headers()
            .get(CONTENT_RANGE)
            .and_then(|h| h.to_str().ok())
            .and_then(|range| range.rsplit_once('/'))
            .and_then(|(_, total)| total.trim().parse::<u64>().ok());
        match total {
            Some(total) => {
                let etag = resp
                    .headers()
                    .get(ETAG)
                    .and_then(|h| h.to_str().ok())
                    .map(String::from);
                debug!(total, "range probe confirms byte ranges");
                Some(RemoteInfo {
                    size: Some(total),
                    ranges_ok: true,
                    etag,
                })
            }
            None => {
                debug!("range probe's Content-Range names no total size");
                None
            }
        }
    }

    /// Fetches JSON from the given URL and deserializes it into `T`.
    ///
    /// Performs an HTTP GET request to `url` and deserializes the response body into the requested type.
    ///
    /// # Returns
    ///
    /// `T` parsed from the response body on success. A refused request is an
    /// `HttpError`, and a body that does not parse is `InvalidJson` naming why.
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
            .call()
            .map_err(|err| DownloadError::request(url, err))?
            .body_mut()
            .read_json()
            .map_err(|err| DownloadError::json(url, err));
        if result.is_ok() {
            trace!(url = url, "JSON parsed successfully");
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::strong_etag;

    #[test]
    fn only_strong_entity_tags_are_usable_for_if_range() {
        for tag in ["\"\"", "\"abc!#$%\""] {
            assert_eq!(strong_etag(tag), Some(tag));
        }
        for tag in ["W/\"abc\"", "abc", "\"a b\"", "\"a\"b\"", "\"a\n\""] {
            assert_eq!(strong_etag(tag), None);
        }
    }
}
