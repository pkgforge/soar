//! Downloading the assets attached to a forge release.

use std::{path::PathBuf, sync::Arc};

use crate::{
    download::Download,
    error::DownloadError,
    filter::Filter,
    forge::Forge,
    types::{OverwriteMode, Progress},
};

/// A download of the assets a forge release publishes.
pub struct ReleaseDownload {
    forge: Forge,
    project: String,
    tag: Option<String>,
    filter: Filter,
    output: Option<String>,
    overwrite: OverwriteMode,
    extract: bool,
    extract_to: Option<PathBuf>,
    on_progress: Option<Arc<dyn Fn(Progress) + Send + Sync>>,
}

impl ReleaseDownload {
    /// Downloads from `project` on `forge`, taking the latest release and
    /// every asset in it unless narrowed further.
    ///
    /// # Examples
    ///
    /// ```
    /// use soar_dl::{forge::Forge, release::ReleaseDownload};
    ///
    /// let dl = ReleaseDownload::new(Forge::GitHub, "owner/repo");
    /// ```
    pub fn new(forge: Forge, project: impl Into<String>) -> Self {
        Self {
            forge,
            project: project.into(),
            tag: None,
            filter: Filter::default(),
            output: None,
            overwrite: OverwriteMode::Prompt,
            extract: false,
            extract_to: None,
            on_progress: None,
        }
    }

    /// Takes the release this tag names rather than the latest one.
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = Some(tag.into());
        self
    }

    /// Selects which of the release's assets to download.
    pub fn filter(mut self, filter: Filter) -> Self {
        self.filter = filter;
        self
    }

    /// Sets where the downloaded assets are written.
    pub fn output(mut self, path: impl Into<String>) -> Self {
        self.output = Some(path.into());
        self
    }

    /// Sets how an asset already on disk is handled.
    pub fn overwrite(mut self, mode: OverwriteMode) -> Self {
        self.overwrite = mode;
        self
    }

    /// Extracts downloaded archives.
    pub fn extract(mut self, extract: bool) -> Self {
        self.extract = extract;
        self
    }

    /// Sets the directory downloaded archives are extracted into.
    pub fn extract_to(mut self, path: impl Into<PathBuf>) -> Self {
        self.extract_to = Some(path.into());
        self
    }

    /// Reports progress for each asset as it downloads.
    pub fn progress<F>(mut self, f: F) -> Self
    where
        F: Fn(Progress) + Send + Sync + 'static,
    {
        self.on_progress = Some(Arc::new(f));
        self
    }

    /// Downloads the matching assets and returns where each was written.
    ///
    /// Without a tag the newest release that is not a prerelease is taken,
    /// falling back to the newest of all when a project only publishes
    /// prereleases.
    pub fn execute(self) -> Result<Vec<PathBuf>, DownloadError> {
        let releases = self
            .forge
            .fetch_releases(&self.project, self.tag.as_deref())?;

        let release = if let Some(ref tag) = self.tag {
            releases.iter().find(|r| r.tag() == tag)
        } else {
            releases
                .iter()
                .find(|r| !r.is_prerelease())
                .or_else(|| releases.first())
        };

        let release = release.ok_or(DownloadError::InvalidResponse)?;

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

            paths.push(dl.execute()?);
        }

        Ok(paths)
    }
}
