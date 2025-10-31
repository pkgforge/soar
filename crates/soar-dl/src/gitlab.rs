use serde::Deserialize;

use crate::{
    error::DownloadError,
    platform::fetch_with_fallback,
    traits::{Asset, Platform, Release},
};

pub struct GitLab;

#[derive(Debug, Clone, Deserialize)]
pub struct GitLabRelease {
    name: String,
    tag_name: String,
    upcoming_release: bool,
    released_at: String,
    assets: GitLabAssets,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitLabAssets {
    pub links: Vec<GitLabAsset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitLabAsset {
    pub name: String,
    pub direct_asset_url: String,
}

impl Platform for GitLab {
    type Release = GitLabRelease;

    const API_PKGFORGE: &'static str = "https://api.gl.pkgforge.dev";
    const API_UPSTREAM: &'static str = "https://gitlab.com";
    const TOKEN_ENV: &'static str = "GITLAB_TOKEN";

    fn fetch_releases(
        project: &str,
        tag: Option<&str>,
    ) -> Result<Vec<Self::Release>, DownloadError> {
        let encoded = project.replace('/', "%2F");
        let path = match tag {
            Some(t) if project.chars().all(char::is_numeric) => {
                format!("/api/v4/projects/{}/releases/{}", encoded, t)
            }
            _ => format!("/api/v4/projects/{}/releases", encoded),
        };

        fetch_with_fallback::<Self::Release>(
            &path,
            Self::API_UPSTREAM,
            Self::API_PKGFORGE,
            Self::TOKEN_ENV,
        )
    }
}

impl Release for GitLabRelease {
    type Asset = GitLabAsset;

    fn name(&self) -> &str {
        &self.name
    }

    fn tag(&self) -> &str {
        &self.tag_name
    }

    fn is_prerelease(&self) -> bool {
        self.upcoming_release
    }

    fn published_at(&self) -> &str {
        &self.released_at
    }

    fn assets(&self) -> &[Self::Asset] {
        &self.assets.links
    }
}

impl Asset for GitLabAsset {
    fn name(&self) -> &str {
        &self.name
    }

    fn size(&self) -> Option<u64> {
        None
    }

    fn url(&self) -> &str {
        &self.direct_asset_url
    }
}
