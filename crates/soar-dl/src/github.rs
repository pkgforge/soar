use serde::Deserialize;

use crate::{
    error::DownloadError,
    platform::fetch_with_fallback,
    traits::{Asset, Platform, Release},
};

pub struct Github;

#[derive(Debug, Clone, Deserialize)]
pub struct GithubRelease {
    pub name: Option<String>,
    pub tag_name: String,
    pub prerelease: bool,
    pub published_at: String,
    pub assets: Vec<GithubAsset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GithubAsset {
    pub name: String,
    pub size: u64,
    pub browser_download_url: String,
}

impl Platform for Github {
    type Release = GithubRelease;

    const API_PKGFORGE: &'static str = "https://api.gh.pkgforge.dev";
    const API_UPSTREAM: &'static str = "https://api.github.com";
    const TOKEN_ENV: [&str; 2] = ["GITHUB_TOKEN", "GH_TOKEN"];

    /// Fetches releases for the given GitHub repository, optionally filtered by a specific tag.
    ///
    /// If `tag` is provided, fetches the release that matches that tag; otherwise fetches the repository's releases (up to 100 per page).
    ///
    /// # Arguments
    ///
    /// * `project` — repository identifier in the form "owner/repo".
    /// * `tag` — optional release tag to filter the results.
    ///
    /// # Returns
    ///
    /// `Ok` with a vector of releases on success, or `Err(DownloadError)` on failure.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use soar_dl::github::Github;
    /// use soar_dl::traits::{Platform, Release};
    ///
    /// let releases = Github::fetch_releases("rust-lang/rust", None).unwrap();
    /// assert!(releases.iter().all(|r| r.tag().len() > 0));
    /// ```
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

    /// The release's name, or an empty string if the release has no name.
    ///
    /// # Examples
    ///
    /// ```
    /// use soar_dl::github::GithubRelease;
    /// use soar_dl::traits::Release;
    ///
    /// let r = GithubRelease {
    ///     name: Some("v1.0".into()),
    ///     tag_name: "v1.0".into(),
    ///     prerelease: false,
    ///     published_at: "".into(),
    ///     assets: vec![],
    /// };
    /// assert_eq!(r.name(), "v1.0");
    ///
    /// let unnamed = GithubRelease {
    ///     name: None,
    ///     tag_name: "v1.1".into(),
    ///     prerelease: false,
    ///     published_at: "".into(),
    ///     assets: vec![],
    /// };
    /// assert_eq!(unnamed.name(), "");
    /// ```
    fn name(&self) -> &str {
        self.name.as_deref().unwrap_or("")
    }

    /// Get the release tag as a string slice.
    ///
    /// # Examples
    ///
    /// ```
    /// use soar_dl::github::GithubRelease;
    /// use soar_dl::traits::Release;
    ///
    /// let release = GithubRelease {
    ///     name: None,
    ///     tag_name: "v1.0.0".into(),
    ///     prerelease: false,
    ///     published_at: "".into(),
    ///     assets: vec![],
    /// };
    /// assert_eq!(release.tag(), "v1.0.0");
    /// ```
    ///
    /// # Returns
    ///
    /// `&str` containing the release tag.
    fn tag(&self) -> &str {
        &self.tag_name
    }

    /// Indicates whether the release is marked as a prerelease.
    ///
    /// # Returns
    ///
    /// `true` if the release is marked as a prerelease, `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use soar_dl::github::GithubRelease;
    /// use soar_dl::traits::Release;
    ///
    /// let r = GithubRelease {
    ///     name: None,
    ///     tag_name: "v1.0.0".to_string(),
    ///     prerelease: true,
    ///     published_at: "".to_string(),
    ///     assets: vec![],
    /// };
    /// assert!(r.is_prerelease());
    /// ```
    fn is_prerelease(&self) -> bool {
        self.prerelease
    }

    /// Returns the release's publication timestamp as an RFC 3339 formatted string.
    ///
    /// # Examples
    ///
    /// ```
    /// use soar_dl::github::GithubRelease;
    /// use soar_dl::traits::Release;
    ///
    /// let r = GithubRelease {
    ///     name: None,
    ///     tag_name: "v1.0.0".into(),
    ///     prerelease: false,
    ///     published_at: "2021-01-01T00:00:00Z".into(),
    ///     assets: vec![],
    /// };
    /// assert_eq!(r.published_at(), "2021-01-01T00:00:00Z");
    /// ```
    fn published_at(&self) -> &str {
        &self.published_at
    }

    /// Get a slice of assets associated with the release.
    ///
    /// The slice contains the release's assets in declaration order.
    ///
    /// # Examples
    ///
    /// ```
    /// use soar_dl::github::{GithubRelease, GithubAsset};
    /// use soar_dl::traits::Release;
    ///
    /// let asset = GithubAsset {
    ///     name: "example.zip".into(),
    ///     size: 1024,
    ///     browser_download_url: "https://example.com/example.zip".into(),
    /// };
    ///
    /// let release = GithubRelease {
    ///     name: Some("v1.0".into()),
    ///     tag_name: "v1.0".into(),
    ///     prerelease: false,
    ///     published_at: "2025-01-01T00:00:00Z".into(),
    ///     assets: vec![asset],
    /// };
    ///
    /// assert_eq!(release.assets().len(), 1);
    /// ```
    fn assets(&self) -> &[Self::Asset] {
        &self.assets
    }
}

impl Asset for GithubAsset {
    /// Retrieves the asset's name.
    ///
    /// # Examples
    ///
    /// ```
    /// use soar_dl::github::GithubAsset;
    /// use soar_dl::traits::Asset;
    ///
    /// let asset = GithubAsset {
    ///     name: "file.zip".to_string(),
    ///     size: 123,
    ///     browser_download_url: "https://example.com/file.zip".to_string(),
    /// };
    /// assert_eq!(asset.name(), "file.zip");
    /// ```
    ///
    /// # Returns
    ///
    /// A `&str` containing the asset's name.
    fn name(&self) -> &str {
        &self.name
    }

    /// Asset size in bytes.
    ///
    /// # Returns
    ///
    /// `Some(size)` containing the asset size in bytes.
    ///
    /// # Examples
    ///
    /// ```
    /// use soar_dl::github::GithubAsset;
    /// use soar_dl::traits::Asset;
    ///
    /// let asset = GithubAsset { name: "file".into(), size: 12345, browser_download_url: "https://example.com".into() };
    /// assert_eq!(asset.size(), Some(12345));
    /// ```
    fn size(&self) -> Option<u64> {
        Some(self.size)
    }

    /// Returns the asset's browser download URL.
    ///
    /// # Examples
    ///
    /// ```
    /// use soar_dl::github::GithubAsset;
    /// use soar_dl::traits::Asset;
    ///
    /// let asset = GithubAsset {
    ///     name: "example".into(),
    ///     size: 123,
    ///     browser_download_url: "https://example.com/download".into(),
    /// };
    /// assert_eq!(asset.url(), "https://example.com/download");
    /// ```
    fn url(&self) -> &str {
        &self.browser_download_url
    }
}
