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

    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = Some(tag.into());
        self
    }

    pub fn filter(mut self, filter: Filter) -> Self {
        self.filter = filter;
        self
    }

    pub fn output(mut self, path: impl Into<String>) -> Self {
        self.output = Some(path.into());
        self
    }

    pub fn overwrite(mut self, mode: OverwriteMode) -> Self {
        self.overwrite = mode;
        self
    }

    pub fn extract(mut self, extract: bool) -> Self {
        self.extract = extract;
        self
    }

    pub fn extract_to(mut self, path: impl Into<PathBuf>) -> Self {
        self.extract_to = Some(path.into());
        self
    }

    pub fn progress<F>(mut self, f: F) -> Self
    where
        F: Fn(Progress) + Send + Sync + 'static,
    {
        self.on_progress = Some(Arc::new(f));
        self
    }

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
            let dl = Download::new(asset.url())
                .output(self.output.clone().unwrap_or_default())
                .overwrite(self.overwrite)
                .extract(self.extract)
                .extract_to(self.extract_to.clone().unwrap_or_default());

            let dl = if let Some(ref cb) = self.on_progress {
                let cb = cb.clone();
                dl.progress(move |p| cb(p))
            } else {
                dl
            };

            let path = dl.execute()?;

            paths.push(path);
        }

        Ok(paths)
    }
}
