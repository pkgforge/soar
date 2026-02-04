use crate::error::DownloadError;

pub trait Asset: Clone {
    fn name(&self) -> &str;
    fn size(&self) -> Option<u64>;
    fn url(&self) -> &str;
}

pub trait Release {
    type Asset: Asset;

    fn name(&self) -> &str;
    fn tag(&self) -> &str;
    fn is_prerelease(&self) -> bool;
    fn published_at(&self) -> &str;
    fn body(&self) -> Option<&str>;
    fn assets(&self) -> &[Self::Asset];
}

pub trait Platform {
    type Release: Release;

    const API_UPSTREAM: &'static str;
    const API_PKGFORGE: &'static str;
    const TOKEN_ENV: [&str; 2];

    fn fetch_releases(
        project: &str,
        tag: Option<&str>,
    ) -> Result<Vec<Self::Release>, DownloadError>;
}
