use serde::Deserialize;

use crate::{
    error::DownloadError,
    platform::fetch_with_fallback,
    traits::{Asset, Platform, Release},
};

pub struct Github;

#[derive(Debug, Clone, Deserialize)]
pub struct GithubRelease {
    name: Option<String>,
    tag_name: String,
    prerelease: bool,
    published_at: String,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GithubAsset {
    name: String,
    size: u64,
    browser_download_url: String,
}

impl Platform for Github {
    type Release = GithubRelease;

    const API_PKGFORGE: &'static str = "https://api.gh.pkgforge.dev";
    const API_UPSTREAM: &'static str = "https://api.github.com";
    const TOKEN_ENV: &'static str = "GITHUB_TOKEN";

    fn fetch_releases(
        project: &str,
        tag: Option<&str>,
    ) -> Result<Vec<Self::Release>, DownloadError> {
        let path = match tag {
            Some(tag) => {
                let encoded_tag =
                    url::form_urlencoded::byte_serialize(tag.as_bytes()).collect::<String>();
                format!(
                    "/repos/{project}/releases/tags/{}?per_page=100",
                    encoded_tag
                )
            }
            None => format!("/repos/{project}/releases?per_page=100"),
        };

        fetch_with_fallback::<Self::Release>(
            &path,
            Self::API_UPSTREAM,
            Self::API_PKGFORGE,
            Self::TOKEN_ENV,
        )
    }
}

impl Release for GithubRelease {
    type Asset = GithubAsset;

    fn name(&self) -> &str {
        self.name.as_deref().unwrap_or("")
    }

    fn tag(&self) -> &str {
        &self.tag_name
    }

    fn is_prerelease(&self) -> bool {
        self.prerelease
    }

    fn published_at(&self) -> &str {
        &self.published_at
    }

    fn assets(&self) -> &[Self::Asset] {
        &self.assets
    }
}

impl Asset for GithubAsset {
    fn name(&self) -> &str {
        &self.name
    }

    fn size(&self) -> Option<u64> {
        Some(self.size)
    }

    fn url(&self) -> &str {
        &self.browser_download_url
    }
}
