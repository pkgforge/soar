use std::{path::PathBuf, sync::Arc};

use crate::{
    download::Download,
    error::DownloadError,
    filter::Filter,
    traits::{Asset as _, Platform, Release as _},
    types::{OverwriteMode, Progress},
};

pub struct ReleaseDownload<P: Platform> {
    project: String,
    tag: Option<String>,
    filter: Filter,
    output: Option<String>,
    overwrite: OverwriteMode,
    extract: bool,
    extract_to: Option<PathBuf>,
    on_progress: Option<Arc<dyn Fn(Progress) + Send + Sync>>,
    _platform: std::marker::PhantomData<P>,
}

impl<P: Platform> ReleaseDownload<P> {
    /// Creates a new `ReleaseDownload` configured for the given project with sensible defaults.
    ///
    /// The returned builder is initialized with:
    /// - `tag = None`
    /// - a default `Filter`
    /// - no explicit output path
    /// - `overwrite = OverwriteMode::Prompt`
    /// - extraction disabled
    /// - no extraction path
    /// - no progress callback
    ///
    /// # Examples
    ///
    /// ```
    /// // `MyPlatform` must implement the `Platform` trait for your environment.
    /// // Replace `MyPlatform` with the concrete platform type you use (e.g., `GitHub`).
    /// let dl = ReleaseDownload::<MyPlatform>::new("owner/repo");
    /// // You can then chain further configuration:
    /// // let dl = dl.tag("v1.2.3").output("downloads/").extract(true);
    /// ```
    pub fn new(project: impl Into<String>) -> Self {
        Self {
            project: project.into(),
            tag: None,
            filter: Filter::default(),
            output: None,
            overwrite: OverwriteMode::Prompt,
            extract: false,
            extract_to: None,
            on_progress: None,
            _platform: std::marker::PhantomData,
        }
    }

    /// Sets the release tag to target when selecting a release.
    ///
    /// The provided tag will be used by `execute` to find a release with a matching tag.
    /// Returns the updated builder to allow method chaining.
    ///
    /// # Examples
    ///
    /// ```
    /// let builder = ReleaseDownload::<MyPlatform>::new("owner/repo").tag("v1.2.3");
    /// ```
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = Some(tag.into());
        self
    }

    /// Sets the asset filter used to select which release assets will be downloaded.
    ///
    /// The provided `filter` will be used to match asset names when executing the download.
    ///
    /// # Examples
    ///
    /// ```
    /// let rd = ReleaseDownload::new("owner/repo").filter(Filter::default());
    /// ```
    pub fn filter(mut self, filter: Filter) -> Self {
        self.filter = filter;
        self
    }

    /// Sets the base output path for downloaded assets.
    ///
    /// The provided path will be used as the destination directory or base file path when downloads are written.
    ///
    /// # Returns
    ///
    /// The modified `ReleaseDownload` builder with the output path set.
    ///
    /// # Examples
    ///
    /// ```
    /// let dl = ReleaseDownload::<MyPlatform>::new("owner/repo").output("downloads");
    /// ```
    pub fn output(mut self, path: impl Into<String>) -> Self {
        self.output = Some(path.into());
        self
    }

    /// Set the overwrite behavior for downloaded files.
    ///
    /// `mode` determines how existing files are handled when downloading (for example, overwrite, skip, or prompt).
    ///
    /// # Examples
    ///
    /// ```
    /// let dl = ReleaseDownload::new("owner/repo").overwrite(OverwriteMode::Overwrite);
    /// ```
    pub fn overwrite(mut self, mode: OverwriteMode) -> Self {
        self.overwrite = mode;
        self
    }

    /// Enables or disables extraction of downloaded assets.
    ///
    /// When set to `true`, assets that are archives will be extracted after they are downloaded.
    ///
    /// # Examples
    ///
    /// ```
    /// let rd = ReleaseDownload::<MyPlatform>::new("owner/repo").extract(true);
    /// ```
    pub fn extract(mut self, extract: bool) -> Self {
        self.extract = extract;
        self
    }

    /// Sets the destination directory where downloaded archives will be extracted.
    ///
    /// # Arguments
    ///
    /// * `path` - Destination path to extract downloaded assets into.
    ///
    /// # Examples
    ///
    /// ```
    /// let rd = ReleaseDownload::new("owner/repo").extract_to("out/artifacts");
    /// ```
    pub fn extract_to(mut self, path: impl Into<PathBuf>) -> Self {
        self.extract_to = Some(path.into());
        self
    }

    /// Registers a callback that will be invoked with progress updates for each download.
    ///
    /// The provided callback is stored and called with `Progress` events as assets are downloaded.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use soar_dl::{ReleaseDownload, Progress};
    ///
    /// let _rd = ReleaseDownload::new("owner/repo")
    ///     .progress(|progress: Progress| {
    ///         // handle progress (e.g., log or update UI)
    ///         println!("{:?}", progress);
    ///     });
    /// ```
    pub fn progress<F>(mut self, f: F) -> Self
    where
        F: Fn(Progress) + Send + Sync + 'static,
    {
        self.on_progress = Some(Arc::new(f));
        self
    }

    /// Downloads matched assets for a project's release and returns their local file paths.
    ///
    /// Selects a release by the configured tag if provided; otherwise prefers the first non-prerelease
    /// release or falls back to the first release. Filters the release's assets using the configured
    /// `Filter`, downloads each matching asset with the configured output, overwrite, and extraction
    /// options, and returns a vector of the resulting local `PathBuf`s. Returns an error if no release
    /// is found or if no assets match the filter.
    ///
    /// # Returns
    ///
    /// A `Vec<PathBuf>` containing the local paths of the downloaded assets on success, or a
    /// `DownloadError` on failure.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::PathBuf;
    ///
    /// // Example usage (replace `MyPlatform` with an implementation of `Platform`).
    /// let paths: Vec<PathBuf> = ReleaseDownload::<MyPlatform>::new("owner/repo")
    ///     .tag("v1.0")
    ///     .filter(Filter::default())
    ///     .output("downloads")
    ///     .execute()
    ///     .unwrap();
    ///
    /// assert!(!paths.is_empty());
    /// ```
    pub fn execute(self) -> Result<Vec<PathBuf>, DownloadError> {
        let releases = P::fetch_releases(&self.project, self.tag.as_deref())?;

        let release = if let Some(ref tag) = self.tag {
            releases.iter().find(|r| r.tag() == tag)
        } else {
            releases
                .iter()
                .find(|r| !r.is_prerelease())
                .or_else(|| releases.first())
        };

        let release = release.ok_or_else(|| DownloadError::InvalidResponse)?;

        let assets: Vec<_> = release
            .assets()
            .iter()
            .filter(|a| self.filter.matches(a.name()))
            .collect();

        if assets.is_empty() {
            return Err(DownloadError::NoMatch {
                available: release
                    .assets()
                    .iter()
                    .map(|a| a.name().to_string())
                    .collect(),
            });
        }

        let mut paths = Vec::new();
        for asset in assets {
            let mut dl = Download::new(asset.url())
                .overwrite(self.overwrite)
                .extract(self.extract);

            if let Some(ref output) = self.output {
                dl = dl.output(output);
            }

            if let Some(ref extract_to) = self.extract_to {
                dl = dl.extract_to(extract_to);
            }

            if let Some(ref cb) = self.on_progress {
                let cb = cb.clone();
                dl = dl.progress(move |p| cb(p));
            }

            let path = dl.execute()?;

            paths.push(path);
        }

        Ok(paths)
    }
}