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

    /// Fetches releases for a GitLab project, optionally narrowing to a specific tag.
    ///
    /// The `project` is the repository identifier (for example `"group/name"` or a numeric project ID).
    /// If `tag` is provided and the `project` consists only of digits, the fetch targets that single release; otherwise the fetch returns the project's release list.
    ///
    /// # Parameters
    ///
    /// - `project`: repository identifier or numeric project ID.
    /// - `tag`: optional release tag to narrow the request.
    ///
    /// # Returns
    ///
    /// `Ok(Vec<GitLabRelease>)` with the fetched releases on success, or a `DownloadError` on failure.
    ///
    /// # Examples
    ///
    /// ```
    /// // Fetch all releases for a namespaced project
    /// let _ = GitLab::fetch_releases("group/project", None);
    ///
    /// // Fetch a specific release when using a numeric project ID
    /// let _ = GitLab::fetch_releases("123456", Some("v1.0.0"));
    /// ```
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

    /// Gets the asset's name.
    ///
    /// # Examples
    ///
    /// ```
    /// let asset = GitLabAsset { name: String::from("v1.0.0"), direct_asset_url: String::from("https://example") };
    /// assert_eq!(asset.name(), "v1.0.0");
    /// ```
    fn name(&self) -> &str {
        &self.name
    }

    /// Get the release's tag name.
    ///
    /// # Examples
    ///
    /// ```
    /// let r = GitLabRelease {
    ///     name: "Release".into(),
    ///     tag_name: "v1.0.0".into(),
    ///     upcoming_release: false,
    ///     released_at: "2025-01-01T00:00:00Z".into(),
    ///     assets: GitLabAssets { links: vec![] },
    /// };
    /// assert_eq!(r.tag(), "v1.0.0");
    /// ```
    fn tag(&self) -> &str {
        &self.tag_name
    }

    /// Indicates whether the release is marked as upcoming.
    ///
    /// # Returns
    ///
    /// `true` if the release is marked as upcoming, `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// let rel = GitLabRelease {
    ///     name: "v1".to_string(),
    ///     tag_name: "v1".to_string(),
    ///     upcoming_release: true,
    ///     released_at: "".to_string(),
    ///     assets: GitLabAssets { links: vec![] },
    /// };
    /// assert!(rel.is_prerelease());
    /// ```
    fn is_prerelease(&self) -> bool {
        self.upcoming_release
    }

    /// Get the release's published date/time string.
    ///
    /// # Examples
    ///
    /// ```
    /// let r = GitLabRelease {
    ///     name: String::from("v1"),
    ///     tag_name: String::from("v1"),
    ///     upcoming_release: false,
    ///     released_at: String::from("2020-01-01T00:00:00Z"),
    ///     assets: GitLabAssets { links: vec![] },
    /// };
    /// assert_eq!(r.published_at(), "2020-01-01T00:00:00Z");
    /// ```
    fn published_at(&self) -> &str {
        &self.released_at
    }

    /// A slice of assets associated with the release.
    ///
    /// # Examples
    ///
    /// ```
    /// let asset = GitLabAsset { name: "file.tar.gz".into(), direct_asset_url: "https://example.com/file.tar.gz".into() };
    /// let assets = GitLabAssets { links: vec![asset.clone()] };
    /// let release = GitLabRelease {
    ///     name: "v1.0".into(),
    ///     tag_name: "v1.0".into(),
    ///     upcoming_release: false,
    ///     released_at: "2025-10-31T00:00:00Z".into(),
    ///     assets,
    /// };
    /// let slice = release.assets();
    /// assert_eq!(slice.len(), 1);
    /// assert_eq!(slice[0].name(), "file.tar.gz");
    /// ```
    ///
    /// # Returns
    ///
    /// A slice of the release's assets.
    fn assets(&self) -> &[Self::Asset] {
        &self.assets.links
    }
}

impl Asset for GitLabAsset {
    /// Gets the asset's name.
    ///
    /// # Examples
    ///
    /// ```
    /// let asset = GitLabAsset { name: String::from("v1.0.0"), direct_asset_url: String::from("https://example") };
    /// assert_eq!(asset.name(), "v1.0.0");
    /// ```
    fn name(&self) -> &str {
        &self.name
    }

    /// Returns the asset size when available; for GitLab assets this is not provided.
    ///
    /// This implementation always reports that size information is unavailable.
    ///
    /// # Examples
    ///
    /// ```
    /// let asset = GitLabAsset {
    ///     name: "example".into(),
    ///     direct_asset_url: "https://gitlab.com/example".into(),
    /// };
    /// assert_eq!(asset.size(), None);
    /// ```
    fn size(&self) -> Option<u64> {
        None
    }

    /// Returns the direct URL of the asset.
    ///
    /// # Examples
    ///
    /// ```
    /// let asset = GitLabAsset {
    ///     name: String::from("example"),
    ///     direct_asset_url: String::from("https://example.com/download"),
    /// };
    /// assert_eq!(asset.url(), "https://example.com/download");
    /// ```
    fn url(&self) -> &str {
        &self.direct_asset_url
    }
}