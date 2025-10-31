use ureq::{http::Response, Body};

use crate::{error::DownloadError, http_client::SHARED_AGENT};

pub struct Http;

impl Http {
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

    pub fn json<T: serde::de::DeserializeOwned>(url: &str) -> Result<T, DownloadError> {
        SHARED_AGENT
            .get(url)
            .call()?
            .body_mut()
            .read_json()
            .map_err(|_| DownloadError::InvalidResponse)
    }
}
