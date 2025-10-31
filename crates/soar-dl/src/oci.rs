use std::{
    fs::{File, OpenOptions, Permissions},
    io::{Read as _, Write as _},
    os::unix::fs::PermissionsExt as _,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
};

use serde::Deserialize;
use soar_utils::fs::is_elf;
use ureq::http::header::{ACCEPT, AUTHORIZATION, ETAG, IF_RANGE, RANGE};

use crate::{
    download::{extract_archive, Download},
    error::DownloadError,
    filter::Filter,
    http_client::SHARED_AGENT,
    types::{OverwriteMode, Progress, ResumeInfo},
    xattr::{read_resume, remove_resume, write_resume},
};

#[derive(Debug, Clone)]
pub struct OciReference {
    pub registry: String,
    pub package: String,
    pub tag: String,
}

impl From<&str> for OciReference {
    fn from(value: &str) -> Self {
        let paths = value.trim_start_matches("ghcr.io/");

        // <package>@sha256:<digest>
        if let Some((package, digest)) = paths.split_once('@') {
            return Self {
                registry: "ghcr.io".to_string(),
                package: package.to_string(),
                tag: digest.to_string(),
            };
        }

        // <package>:<tag>
        if let Some((package, tag)) = paths.split_once(':') {
            return Self {
                registry: "ghcr.io".to_string(),
                package: package.to_string(),
                tag: tag.to_string(),
            };
        }

        Self {
            registry: "ghcr.io".to_string(),
            package: paths.to_string(),
            tag: "latest".to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct OciManifest {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub config: OciConfig,
    pub layers: Vec<OciLayer>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OciConfig {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub digest: String,
    pub size: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OciLayer {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub digest: String,
    pub size: u64,
    #[serde(default)]
    pub annotations: std::collections::HashMap<String, String>,
}

impl OciLayer {
    pub fn title(&self) -> Option<&str> {
        self.annotations
            .get("org.opencontainers.image.title")
            .map(|s| s.as_str())
    }
}

pub struct OciDownload {
    reference: OciReference,
    api: String,
    filter: Filter,
    output: Option<String>,
    overwrite: OverwriteMode,
    extract: bool,
    extract_to: Option<PathBuf>,
    parallel: usize,
    on_progress: Option<Arc<dyn Fn(Progress) + Send + Sync>>,
}

impl OciDownload {
    pub fn new(reference: impl Into<OciReference>) -> Self {
        Self {
            reference: reference.into(),
            api: "https://ghcr.io/v2".into(),
            filter: Filter::default(),
            output: None,
            overwrite: OverwriteMode::Prompt,
            extract: false,
            extract_to: None,
            parallel: 1,
            on_progress: None,
        }
    }

    pub fn api(mut self, api: impl Into<String>) -> Self {
        self.api = api.into();
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

    pub fn parallel(mut self, n: usize) -> Self {
        self.parallel = n.max(1);
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
        // If it's a blob digest, download directly
        if self.reference.tag.starts_with("sha256:") {
            return self.download_blob();
        }

        let manifest = self.fetch_manifest()?;

        let layers: Vec<_> = manifest
            .layers
            .iter()
            .filter(|layer| {
                layer
                    .title()
                    .map(|t| self.filter.matches(t))
                    .unwrap_or(false)
            })
            .collect();

        if layers.is_empty() {
            return Err(DownloadError::LayerNotFound);
        }

        let total_size: u64 = layers.iter().map(|l| l.size).sum();

        if let Some(cb) = &self.on_progress {
            cb(Progress::Starting {
                total: total_size,
            });
        }

        let output_dir = self
            .output
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_default();
        if !output_dir.as_os_str().is_empty() {
            std::fs::create_dir_all(&output_dir)?;
        }

        let paths = if self.parallel > 1 {
            self.download_layers_parallel(&layers, &output_dir)?
        } else {
            self.download_layers_sequential(&layers, &output_dir)?
        };

        if let Some(cb) = &self.on_progress {
            cb(Progress::Complete {
                total: total_size,
            });
        }

        Ok(paths)
    }

    fn download_layers_sequential(
        &self,
        layers: &[&OciLayer],
        output_dir: &Path,
    ) -> Result<Vec<PathBuf>, DownloadError> {
        let mut paths = Vec::new();
        let mut downloaded = 0u64;

        for layer in layers {
            let filename = layer.title().unwrap();
            let path = output_dir.join(filename);

            self.download_layer(layer, &path, &mut downloaded)?;

            if self.extract {
                let extract_dir = self
                    .extract_to
                    .clone()
                    .unwrap_or_else(|| output_dir.to_path_buf());
                extract_archive(&path, &extract_dir)?;
            }

            paths.push(path);
        }

        Ok(paths)
    }

    fn download_layers_parallel(
        &self,
        layers: &[&OciLayer],
        output_dir: &Path,
    ) -> Result<Vec<PathBuf>, DownloadError> {
        let downloaded = Arc::new(Mutex::new(0u64));
        let paths = Arc::new(Mutex::new(Vec::new()));
        let errors = Arc::new(Mutex::new(Vec::new()));

        let owned_layers: Vec<OciLayer> = layers.iter().map(|&layer| layer.clone()).collect();
        let chunks: Vec<_> = owned_layers
            .chunks(layers.len().div_ceil(self.parallel))
            .map(|chunk| chunk.to_vec())
            .collect();

        let handles: Vec<_> = chunks
            .into_iter()
            .map(|chunk| {
                let api = self.api.clone();
                let reference = self.reference.clone();
                let output_dir = output_dir.to_path_buf();
                let downloaded = Arc::clone(&downloaded);
                let paths = Arc::clone(&paths);
                let errors = Arc::clone(&errors);
                let extract = self.extract;
                let extract_to = self.extract_to.clone();
                let on_progress = self.on_progress.clone();

                thread::spawn(move || {
                    for layer in chunk {
                        let filename = match layer.title() {
                            Some(f) => f,
                            None => continue,
                        };
                        let path = output_dir.join(filename);

                        let mut local_downloaded = 0u64;
                        let result = download_layer_impl(
                            &api,
                            &reference,
                            &layer,
                            &path,
                            &mut local_downloaded,
                            on_progress.as_ref(),
                            &downloaded,
                        );

                        match result {
                            Ok(()) => {
                                if extract {
                                    let extract_dir =
                                        extract_to.clone().unwrap_or_else(|| output_dir.clone());
                                    if let Err(e) = extract_archive(&path, &extract_dir) {
                                        errors.lock().unwrap().push(format!("{e}"));
                                        continue;
                                    }
                                }
                                paths.lock().unwrap().push(path);
                            }
                            Err(e) => {
                                errors.lock().unwrap().push(format!("{e}"));
                            }
                        }
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().ok();
        }

        let errors = errors.lock().unwrap();
        if !errors.is_empty() {
            return Err(DownloadError::Multiple {
                errors: errors.clone(),
            });
        }

        let paths = paths.lock().unwrap().clone();
        Ok(paths)
    }

    fn fetch_manifest(&self) -> Result<OciManifest, DownloadError> {
        let url = format!(
            "{}/{}/manifests/{}",
            self.api.trim_end_matches('/'),
            self.reference.package,
            self.reference.tag
        );

        let mut resp = SHARED_AGENT
            .get(&url)
            .header(
                ACCEPT,
                "application/vnd.docker.distribution.manifest.v2+json, \
                application/vnd.docker.distribution.manifest.list.v2+json, \
                application/vnd.oci.image.manifest.v1+json, \
                application/vnd.oci.image.index.v1+json",
            )
            .header(AUTHORIZATION, "Bearer QQ==")
            .call()?;

        if !resp.status().is_success() {
            return Err(DownloadError::HttpError {
                status: resp.status().as_u16(),
                url,
            });
        }

        resp.body_mut()
            .read_json()
            .map_err(|_| DownloadError::InvalidResponse)
    }

    fn download_blob(&self) -> Result<Vec<PathBuf>, DownloadError> {
        let filename = self
            .reference
            .package
            .rsplit_once('/')
            .map(|(_, name)| name)
            .unwrap_or(&self.reference.tag);

        let output = self.output.as_deref().unwrap_or(filename);
        let path = PathBuf::from(output);

        let url = format!(
            "{}/{}/blobs/{}",
            self.api.trim_end_matches('/'),
            self.reference.package,
            self.reference.tag
        );

        let dl = Download::new(url).output(output).overwrite(self.overwrite);

        let dl = if let Some(ref cb) = self.on_progress {
            let cb = cb.clone();
            dl.progress(move |p| cb(p))
        } else {
            dl
        };

        dl.execute()?;

        Ok(vec![path])
    }

    fn download_layer(
        &self,
        layer: &OciLayer,
        path: &Path,
        downloaded: &mut u64,
    ) -> Result<(), DownloadError> {
        download_layer_impl(
            &self.api,
            &self.reference,
            layer,
            path,
            downloaded,
            self.on_progress.as_ref(),
            &std::sync::Arc::new(std::sync::Mutex::new(0u64)),
        )
    }
}

fn download_layer_impl(
    api: &str,
    reference: &OciReference,
    layer: &OciLayer,
    path: &Path,
    local_downloaded: &mut u64,
    on_progress: Option<&Arc<dyn Fn(Progress) + Send + Sync>>,
    shared_downloaded: &std::sync::Arc<std::sync::Mutex<u64>>,
) -> Result<(), DownloadError> {
    let url = format!(
        "{}/{}/blobs/{}",
        api.trim_end_matches('/'),
        reference.package,
        layer.digest
    );

    let resume_info = read_resume(path);
    let (resume_from, etag) = resume_info
        .as_ref()
        .map(|r| (Some(r.downloaded), r.etag.as_deref()))
        .unwrap_or((None, None));

    let mut req = SHARED_AGENT.get(&url).header(AUTHORIZATION, "Bearer QQ==");

    if let Some(pos) = resume_from {
        req = req.header(RANGE, &format!("bytes={}-", pos));
        if let Some(tag) = etag {
            req = req.header(IF_RANGE, tag);
        }
    }

    let resp = req.call()?;

    if !resp.status().is_success() {
        return Err(DownloadError::HttpError {
            status: resp.status().as_u16(),
            url,
        });
    }

    let mut file = if resume_from.is_some() && resp.status() == 206 {
        OpenOptions::new().append(true).open(path)?
    } else {
        File::create(path)?
    };

    let new_etag = resp
        .headers()
        .get(ETAG)
        .and_then(|h| h.to_str().ok())
        .map(String::from);
    let mut reader = resp.into_body().into_reader();
    let mut buffer = [0u8; 8192];
    *local_downloaded = resume_from.unwrap_or(0);

    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }

        file.write_all(&buffer[..n])?;
        *local_downloaded += n as u64;

        {
            let mut shared = shared_downloaded.lock().unwrap();
            *shared += n as u64;
        }

        if (*local_downloaded).is_multiple_of(1024 * 1024) {
            write_resume(
                path,
                &ResumeInfo {
                    downloaded: *local_downloaded,
                    total: layer.size,
                    etag: new_etag.clone(),
                    last_modified: None,
                },
            )?;
        }

        if let Some(cb) = on_progress {
            let shared = shared_downloaded.lock().unwrap();
            cb(Progress::Chunk {
                current: *shared,
                total: layer.size,
            });
        }
    }

    if is_elf(path) {
        std::fs::set_permissions(path, Permissions::from_mode(0o755))?;
    }

    remove_resume(path)?;
    Ok(())
}
