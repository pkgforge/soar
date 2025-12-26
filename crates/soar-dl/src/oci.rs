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
use tracing::{debug, trace};
use ureq::http::header::{ACCEPT, AUTHORIZATION, ETAG, IF_RANGE, RANGE};

use crate::{
    download::Download,
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
    /// Parses an OCI/GHCR reference string into an `OciReference`.
    ///
    /// Accepts strings with optional `ghcr.io/` prefix and extracts the package and tag or digest:
    /// - `package@sha256:<digest>` → digest used as `tag`
    /// - `package:<tag>` → tag used as `tag`
    /// - otherwise the full path is treated as `package` and `tag` is set to `"latest"`.
    ///
    /// # Examples
    ///
    /// ```
    /// use soar_dl::oci::OciReference;
    ///
    /// let r = OciReference::from("ghcr.io/org/repo@sha256:deadbeef");
    /// assert_eq!(r.registry, "ghcr.io");
    /// assert_eq!(r.package, "org/repo");
    /// assert_eq!(r.tag, "sha256:deadbeef");
    ///
    /// let r2 = OciReference::from("org/repo:1.2.3");
    /// assert_eq!(r2.registry, "ghcr.io");
    /// assert_eq!(r2.package, "org/repo");
    /// assert_eq!(r2.tag, "1.2.3");
    ///
    /// let r3 = OciReference::from("org/repo");
    /// assert_eq!(r3.registry, "ghcr.io");
    /// assert_eq!(r3.package, "org/repo");
    /// assert_eq!(r3.tag, "latest");
    /// ```
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
    /// Extracts the human-friendly title from the layer's annotations.
    ///
    /// Returns `Some(&str)` containing the value of the `"org.opencontainers.image.title"`
    /// annotation if present, or `None` if that annotation is absent.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::HashMap;
    /// use soar_dl::oci::OciLayer;
    ///
    /// let mut annotations = HashMap::new();
    /// annotations.insert(
    ///     "org.opencontainers.image.title".to_string(),
    ///     "example.txt".to_string(),
    /// );
    ///
    /// let layer = OciLayer {
    ///     media_type: String::new(),
    ///     digest: String::new(),
    ///     size: 0,
    ///     annotations,
    /// };
    ///
    /// assert_eq!(layer.title(), Some("example.txt"));
    /// ```
    pub fn title(&self) -> Option<&str> {
        self.annotations
            .get("org.opencontainers.image.title")
            .map(|s| s.as_str())
    }
}

#[derive(Clone)]
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
    /// Creates a new `OciDownload` for the given OCI reference using sensible defaults.
    ///
    /// Defaults:
    /// - `api` = "https://ghcr.io/v2"
    /// - `filter` = `Filter::default()`
    /// - no output path (downloads to current working directory unless `output` is set)
    /// - `overwrite` = `OverwriteMode::Prompt`
    /// - `extract` = `false`
    /// - `parallel` = 1 (sequential downloads)
    /// - no progress callback
    ///
    /// # Examples
    ///
    /// ```
    /// use soar_dl::oci::OciDownload;
    ///
    /// let dl = OciDownload::new("ghcr.io/myorg/myrepo:latest");
    /// // configure and run:
    /// // let result = dl.output("out").execute();
    /// ```
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

    /// Sets the base API URL used for manifest and blob requests and returns the downloader for chaining.
    ///
    /// # Examples
    ///
    /// ```
    /// use soar_dl::oci::OciDownload;
    ///
    /// let dl = OciDownload::new("ghcr.io/foo/bar:latest").api("https://ghcr.example.com/v2");
    /// ```
    pub fn api(mut self, api: impl Into<String>) -> Self {
        self.api = api.into();
        self
    }

    /// Sets the layer filter used to select which OCI image layers will be downloaded.
    ///
    /// The provided `filter` is applied when a manifest is inspected to decide which layers are included
    /// in the download operation. Returns the downloader with the filter updated for further chaining.
    ///
    /// # Examples
    ///
    /// ```
    /// use soar_dl::oci::OciDownload;
    /// use soar_dl::filter::Filter;
    ///
    /// let downloader = OciDownload::new("owner/repo:tag")
    ///     .filter(Filter::default())
    ///     .output("out/dir");
    /// ```
    pub fn filter(mut self, filter: Filter) -> Self {
        self.filter = filter;
        self
    }

    /// Sets the output directory path where downloaded files will be written.
    ///
    /// The provided path is stored and used as the destination for downloaded blobs and extracted contents.
    ///
    /// # Examples
    ///
    /// ```
    /// use soar_dl::oci::OciDownload;
    ///
    /// let dl = OciDownload::new("ghcr.io/org/repo:tag").output("downloads/");
    /// ```
    pub fn output(mut self, path: impl Into<String>) -> Self {
        self.output = Some(path.into());
        self
    }

    /// Sets how existing files are handled when writing downloads.
    ///
    /// This configures the downloader's overwrite behavior to the provided `mode`.
    ///
    /// # Examples
    ///
    /// ```
    /// use soar_dl::oci::OciDownload;
    /// use soar_dl::types::OverwriteMode;
    ///
    /// let dl = OciDownload::new("ghcr.io/example/repo:tag").overwrite(OverwriteMode::Prompt);
    /// ```
    ///  Returns the modified downloader with the given overwrite mode.
    pub fn overwrite(mut self, mode: OverwriteMode) -> Self {
        self.overwrite = mode;
        self
    }

    /// Enable or disable extraction of downloaded layers.
    ///
    /// When `true`, downloaded archive layers will be extracted after download into the extraction
    /// destination (if set via `extract_to`) or into the configured output directory. When `false`,
    /// archives are left as downloaded.
    ///
    /// # Examples
    ///
    /// ```
    /// use soar_dl::oci::OciDownload;
    ///
    /// let dl = OciDownload::new("ghcr.io/org/pkg:1.0")
    ///     .extract(true)
    ///     .extract_to("/tmp/out");
    /// ```
    pub fn extract(mut self, extract: bool) -> Self {
        self.extract = extract;
        self
    }

    /// Sets the destination directory for extracting downloaded archives.
    ///
    /// When extraction is enabled, extracted files will be written to this path instead of the download output directory.
    ///
    /// # Examples
    ///
    /// ```
    /// use soar_dl::oci::OciDownload;
    ///
    /// let downloader = OciDownload::new("ghcr.io/org/pkg:1.0")
    ///     .extract(true)
    ///     .extract_to("/tmp/extracted");
    /// ```
    pub fn extract_to(mut self, path: impl Into<PathBuf>) -> Self {
        self.extract_to = Some(path.into());
        self
    }

    /// Set the number of parallel download workers (at least 1).
    ///
    /// If `n` is less than 1, the value will be clamped to 1. Returns the updated downloader to allow chaining.
    ///
    /// # Examples
    ///
    /// ```
    /// use soar_dl::oci::OciDownload;
    ///
    /// let _ = OciDownload::new("owner/repo:tag").parallel(4);
    /// ```
    pub fn parallel(mut self, n: usize) -> Self {
        self.parallel = n.max(1);
        self
    }

    /// Registers a progress callback to receive download `Progress` events.
    ///
    /// The provided callback will be invoked with progress updates (e.g., Starting, Chunk, Complete)
    /// from the download workers and must be thread-safe (`Send + Sync`) and `'static`.
    ///
    /// # Examples
    ///
    /// ```
    /// use soar_dl::oci::OciDownload;
    ///
    /// let downloader = OciDownload::new("ghcr.io/owner/repo:tag")
    ///     .progress(|progress| {
    ///         println!("progress: {:?}", progress);
    ///     });
    /// ```
    pub fn progress<F>(mut self, f: F) -> Self
    where
        F: Fn(Progress) + Send + Sync + 'static,
    {
        self.on_progress = Some(Arc::new(f));
        self
    }

    /// Downloads the OCI reference according to the configured options and returns the downloaded file paths.
    ///
    /// Attempts a direct blob download if the reference tag is a digest (e.g., `sha256:...`); otherwise it
    /// fetches the image manifest, selects layers whose titles match the configured filter, creates the
    /// output directory if provided, and downloads the selected layers.
    ///
    /// Emits `Progress::Starting` and `Progress::Complete` events via the registered progress callback when present.
    ///
    /// # Returns
    ///
    /// A `Vec<PathBuf>` containing the filesystem paths of the downloaded files on success. Returns a
    /// `DownloadError::LayerNotFound` if no manifest layers match the configured filter, or other
    /// `DownloadError` variants for network, IO, or extraction failures.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::path::PathBuf;
    /// # use soar_dl::oci::OciDownload;
    /// // Download layers from an OCI reference into "out" directory.
    /// let paths: Vec<PathBuf> = OciDownload::new("ghcr.io/owner/repo:latest")
    ///     .output("out")
    ///     .execute()
    ///     .unwrap();
    /// assert!(!paths.is_empty());
    /// ```
    pub fn execute(self) -> Result<Vec<PathBuf>, DownloadError> {
        debug!(
            registry = self.reference.registry,
            package = self.reference.package,
            tag = self.reference.tag,
            "starting OCI download"
        );

        // If it's a blob digest, download directly
        if self.reference.tag.starts_with("sha256:") {
            trace!("tag is digest, downloading blob directly");
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
            debug!("no matching layers found in manifest");
            return Err(DownloadError::LayerNotFound);
        }

        let total_size: u64 = layers.iter().map(|l| l.size).sum();
        debug!(
            layer_count = layers.len(),
            total_size = total_size,
            "downloading layers"
        );

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

    /// Downloads the provided OCI layers one after another into `output_dir` and returns the paths of the saved files.
    ///
    /// Each layer is written using its `title()` as the filename. If `self.extract` is enabled, each downloaded archive
    /// is extracted into `self.extract_to` (if set) or into `output_dir`. The method updates an internal downloaded byte
    /// counter while performing transfers.
    ///
    /// # Errors
    ///
    /// Returns a `DownloadError` if any individual layer download or extraction fails.
    ///
    /// # Parameters
    ///
    /// - `layers`: slice of layer references to download (order is preserved).
    /// - `output_dir`: destination directory for downloaded files.
    ///
    /// # Returns
    ///
    /// A `Vec<PathBuf>` containing the full paths to the downloaded (and optionally extracted) files.
    fn download_layers_sequential(
        &self,
        layers: &[&OciLayer],
        output_dir: &Path,
    ) -> Result<Vec<PathBuf>, DownloadError> {
        let mut paths = Vec::new();
        let mut downloaded = 0u64;
        let total_size: u64 = layers.iter().map(|l| l.size).sum();

        for layer in layers {
            let filename = layer.title().unwrap();
            let path = output_dir.join(filename);

            if path.is_file() {
                if let Ok(metadata) = path.metadata() {
                    if metadata.len() == layer.size {
                        downloaded += layer.size;
                        if let Some(ref cb) = self.on_progress {
                            cb(Progress::Chunk {
                                current: downloaded,
                                total: total_size,
                            });
                        }
                        paths.push(path);
                        continue;
                    }
                }
            }

            self.download_layer(layer, &path, &mut downloaded, total_size)?;

            if self.extract {
                let extract_dir = self
                    .extract_to
                    .clone()
                    .unwrap_or_else(|| output_dir.to_path_buf());
                compak::extract_archive(&path, &extract_dir)?;
            }

            paths.push(path);
        }

        Ok(paths)
    }

    /// Download multiple OCI layers concurrently and return the downloaded file paths.
    ///
    /// This method downloads the provided layers using up to `self.parallel` worker threads,
    /// writes each layer into `output_dir` (using the layer title as filename), optionally
    /// extracts archives when extraction is enabled, and aggregates any errors that occur
    /// across worker threads into a single `DownloadError::Multiple`.
    ///
    /// - If a layer has no title, it is skipped.
    /// - On success, returns a `Vec<PathBuf>` containing the paths to successfully downloaded files.
    /// - On failure, returns `DownloadError::Multiple` with stringified error messages from workers.
    ///
    /// # Parameters
    ///
    /// - `layers`: slice of layer references to download. Each layer's title is used as the filename.
    /// - `output_dir`: directory where downloaded files will be written.
    ///
    /// # Returns
    ///
    /// `Ok(Vec<PathBuf>)` with the paths of successfully downloaded files, or `Err(DownloadError::Multiple)`
    /// if one or more worker threads report errors.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use soar_dl::oci::OciDownload;
    ///
    /// let dl = OciDownload::new("ghcr.io/owner/repo:tag")
    ///     .output("out")
    ///     .parallel(4);
    /// let _ = dl.execute(); // invokes the parallel download path when appropriate
    /// ```
    pub fn download_layers_parallel(
        &self,
        layers: &[&OciLayer],
        output_dir: &Path,
    ) -> Result<Vec<PathBuf>, DownloadError> {
        let downloaded = Arc::new(Mutex::new(0u64));
        let paths = Arc::new(Mutex::new(Vec::new()));
        let errors = Arc::new(Mutex::new(Vec::new()));

        let total_size: u64 = layers.iter().map(|l| l.size).sum();

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

                        if path.is_file() {
                            if let Ok(metadata) = path.metadata() {
                                if metadata.len() == layer.size {
                                    {
                                        let mut shared = downloaded.lock().unwrap();
                                        *shared += layer.size;
                                        let current = *shared;
                                        if let Some(ref cb) = on_progress {
                                            cb(Progress::Chunk {
                                                current,
                                                total: total_size,
                                            });
                                        }
                                    }
                                    paths.lock().unwrap().push(path);
                                    continue;
                                }
                            }
                        }

                        let mut local_downloaded = 0u64;
                        let result = download_layer_impl(
                            &api,
                            &reference,
                            &layer,
                            &path,
                            &mut local_downloaded,
                            on_progress.as_ref(),
                            &downloaded,
                            total_size,
                        );

                        match result {
                            Ok(()) => {
                                if extract {
                                    let extract_dir =
                                        extract_to.clone().unwrap_or_else(|| output_dir.clone());
                                    if let Err(e) = compak::extract_archive(&path, &extract_dir) {
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
            if let Err(err) = handle.join() {
                errors
                    .lock()
                    .unwrap()
                    .push(format!("Worker thread panicked: {err:?}"));
            }
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

    /// Fetches the OCI/Docker manifest for the configured reference and returns it deserialized as an `OciManifest`.
    ///
    /// The request is made against the download instance's `api` base and the reference's `package`/`tag`. On
    /// non-success HTTP status codes this returns `DownloadError::HttpError`; if the response body cannot be
    /// parsed as a manifest JSON this returns `DownloadError::InvalidResponse`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use soar_dl::oci::OciDownload;
    ///
    /// let dl = OciDownload::new("ghcr.io/example/repo:latest");
    /// let manifest = dl.fetch_manifest().unwrap();
    /// ```
    pub fn fetch_manifest(&self) -> Result<OciManifest, DownloadError> {
        let url = format!(
            "{}/{}/manifests/{}",
            self.api.trim_end_matches('/'),
            self.reference.package,
            self.reference.tag
        );
        debug!(url = url, "fetching OCI manifest");

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

        trace!(
            status = resp.status().as_u16(),
            "manifest response received"
        );

        if !resp.status().is_success() {
            debug!(status = resp.status().as_u16(), "manifest fetch failed");
            return Err(DownloadError::HttpError {
                status: resp.status().as_u16(),
                url,
            });
        }

        let manifest: OciManifest = resp
            .body_mut()
            .read_json()
            .map_err(|_| DownloadError::InvalidResponse)?;

        trace!(
            layers = manifest.layers.len(),
            media_type = manifest.media_type,
            "manifest parsed successfully"
        );

        Ok(manifest)
    }

    /// Downloads the single blob identified by the downloader's reference into the configured output location.
    ///
    /// The resulting file is written using the configured output path (or a name derived from the reference if no output is set), and download options such as overwrite mode and the registered progress callback are respected.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use soar_dl::oci::OciDownload;
    ///
    /// let dl = OciDownload::new("ghcr.io/org/package:tag");
    /// let paths = dl.download_blob().unwrap();
    /// assert_eq!(paths.len(), 1);
    /// let downloaded = &paths[0];
    /// println!("Downloaded to: {:?}", downloaded);
    /// ```
    pub fn download_blob(&self) -> Result<Vec<PathBuf>, DownloadError> {
        let filename = self
            .reference
            .package
            .rsplit_once('/')
            .map(|(_, name)| name)
            .unwrap_or(&self.reference.tag);

        let output = self.output.as_deref().unwrap_or(filename);

        let url = format!(
            "{}/{}/blobs/{}",
            self.api.trim_end_matches('/'),
            self.reference.package,
            self.reference.tag
        );

        let dl = Download::new(url)
            .output(output)
            .overwrite(self.overwrite)
            .ghcr_blob();

        let dl = if let Some(ref cb) = self.on_progress {
            let cb = cb.clone();
            dl.progress(move |p| cb(p))
        } else {
            dl
        };

        let path = dl.execute()?;

        Ok(vec![path])
    }

    /// Downloads a single OCI layer blob to the given file path, updating the provided
    /// cumulative `downloaded` byte counter and emitting progress events if configured.
    ///
    /// # Parameters
    ///
    /// - `layer`: The manifest layer to download.
    /// - `path`: Destination filesystem path for the downloaded blob.
    /// - `downloaded`: Mutable cumulative byte counter; this function increments it by the
    ///   number of bytes written for this layer.
    ///
    /// # Returns
    ///
    /// `Ok(())` on success, or a `DownloadError` describing the failure.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    /// use soar_dl::oci::{OciDownload, OciLayer};
    ///
    /// let downloader = OciDownload::new("ghcr.io/owner/repo:tag");
    /// let layer = OciLayer {
    ///     media_type: "application/vnd.oci.image.layer.v1.tar".to_string(),
    ///     digest: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
    ///     size: 1024,
    ///     annotations: Default::default(),
    /// };
    /// let mut downloaded = 0u64;
    /// let dest = Path::new("/tmp/layer.tar");
    /// downloader.download_layer(&layer, dest, &mut downloaded, 1024).unwrap();
    /// assert!(downloaded > 0);
    /// ```
    pub fn download_layer(
        &self,
        layer: &OciLayer,
        path: &Path,
        downloaded: &mut u64,
        total_size: u64,
    ) -> Result<(), DownloadError> {
        download_layer_impl(
            &self.api,
            &self.reference,
            layer,
            path,
            downloaded,
            self.on_progress.as_ref(),
            &Arc::new(Mutex::new(0u64)),
            total_size,
        )
    }
}

/// Downloads a single OCI layer blob to the given file path with resume support, progress reporting, and post-download handling.
///
/// This function:
/// - Resumes partially downloaded blobs when resume metadata exists (uses Range and If-Range headers).
/// - Appends to the existing file when a partial response (206) is returned, otherwise creates a new file.
/// - Periodically persists resume metadata while downloading (every 1 MiB).
/// - Emits `Progress::Chunk` updates via the optional `on_progress` callback using the shared downloaded counter.
/// - Marks the file executable (0o755) if it appears to be an ELF binary and removes any resume metadata on success.
///
/// Returns `Ok(())` on success or a `DownloadError` on failure.
#[allow(clippy::too_many_arguments)]
fn download_layer_impl(
    api: &str,
    reference: &OciReference,
    layer: &OciLayer,
    path: &Path,
    local_downloaded: &mut u64,
    on_progress: Option<&Arc<dyn Fn(Progress) + Send + Sync>>,
    shared_downloaded: &Arc<Mutex<u64>>,
    total_size: u64,
) -> Result<(), DownloadError> {
    let url = format!(
        "{}/{}/blobs/{}",
        api.trim_end_matches('/'),
        reference.package,
        layer.digest
    );

    trace!(
        digest = layer.digest,
        size = layer.size,
        path = %path.display(),
        "downloading layer"
    );

    let resume_info = read_resume(path);
    let (resume_from, etag) = resume_info
        .as_ref()
        .map(|r| (Some(r.downloaded), r.etag.as_deref()))
        .unwrap_or((None, None));

    let mut req = SHARED_AGENT.get(&url).header(AUTHORIZATION, "Bearer QQ==");

    if let Some(pos) = resume_from {
        trace!(resume_from = pos, "attempting to resume download");
        req = req.header(RANGE, &format!("bytes={}-", pos));
        if let Some(tag) = etag {
            req = req.header(IF_RANGE, tag);
        }
    }

    let resp = req.call()?;

    if !resp.status().is_success() {
        debug!(
            status = resp.status().as_u16(),
            digest = layer.digest,
            "layer download failed"
        );
        return Err(DownloadError::HttpError {
            status: resp.status().as_u16(),
            url,
        });
    }

    let is_resuming = resume_from.is_some() && resp.status() == 206;
    if is_resuming {
        trace!(resumed_from = resume_from.unwrap(), "resuming download");
    }
    let mut file = if is_resuming {
        if let Some(cb) = on_progress {
            cb(Progress::Resuming {
                current: resume_from.unwrap(),
                total: total_size,
            });
        }
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
    let mut last_checkpoint = *local_downloaded / (1024 * 1024);

    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }

        file.write_all(&buffer[..n])?;
        *local_downloaded += n as u64;

        let current_total = {
            let mut shared = shared_downloaded.lock().unwrap();
            *shared += n as u64;
            *shared
        };

        let checkpoint = *local_downloaded / (1024 * 1024);
        if checkpoint > last_checkpoint {
            last_checkpoint = checkpoint;
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
            cb(Progress::Chunk {
                current: current_total,
                total: total_size,
            });
        }
    }

    if is_elf(path) {
        trace!(path = %path.display(), "setting executable permissions on ELF binary");
        std::fs::set_permissions(path, Permissions::from_mode(0o755))?;
    }

    remove_resume(path)?;
    trace!(
        path = %path.display(),
        bytes = *local_downloaded,
        "layer download complete"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oci_reference_from_str_simple() {
        let reference = OciReference::from("org/repo:tag");
        assert_eq!(reference.registry, "ghcr.io");
        assert_eq!(reference.package, "org/repo");
        assert_eq!(reference.tag, "tag");
    }

    #[test]
    fn test_oci_reference_from_str_with_prefix() {
        let reference = OciReference::from("ghcr.io/org/repo:latest");
        assert_eq!(reference.registry, "ghcr.io");
        assert_eq!(reference.package, "org/repo");
        assert_eq!(reference.tag, "latest");
    }

    #[test]
    fn test_oci_reference_from_str_with_digest() {
        let reference = OciReference::from("org/repo@sha256:deadbeef1234567890");
        assert_eq!(reference.registry, "ghcr.io");
        assert_eq!(reference.package, "org/repo");
        assert_eq!(reference.tag, "sha256:deadbeef1234567890");
    }

    #[test]
    fn test_oci_reference_from_str_no_tag() {
        let reference = OciReference::from("org/repo");
        assert_eq!(reference.registry, "ghcr.io");
        assert_eq!(reference.package, "org/repo");
        assert_eq!(reference.tag, "latest");
    }

    #[test]
    fn test_oci_reference_from_str_nested_package() {
        let reference = OciReference::from("org/team/repo:v1.0");
        assert_eq!(reference.registry, "ghcr.io");
        assert_eq!(reference.package, "org/team/repo");
        assert_eq!(reference.tag, "v1.0");
    }

    #[test]
    fn test_oci_reference_from_str_digest_with_prefix() {
        let reference = OciReference::from("ghcr.io/org/repo@sha256:abc123");
        assert_eq!(reference.registry, "ghcr.io");
        assert_eq!(reference.package, "org/repo");
        assert_eq!(reference.tag, "sha256:abc123");
    }

    #[test]
    fn test_oci_reference_clone() {
        let ref1 = OciReference::from("org/repo:tag");
        let ref2 = ref1.clone();
        assert_eq!(ref1.registry, ref2.registry);
        assert_eq!(ref1.package, ref2.package);
        assert_eq!(ref1.tag, ref2.tag);
    }

    #[test]
    fn test_oci_layer_title_present() {
        let mut annotations = std::collections::HashMap::new();
        annotations.insert(
            "org.opencontainers.image.title".to_string(),
            "myfile.tar.gz".to_string(),
        );

        let layer = OciLayer {
            media_type: "application/vnd.oci.image.layer.v1.tar".to_string(),
            digest: "sha256:abc123".to_string(),
            size: 1024,
            annotations,
        };

        assert_eq!(layer.title(), Some("myfile.tar.gz"));
    }

    #[test]
    fn test_oci_layer_title_absent() {
        let layer = OciLayer {
            media_type: "application/vnd.oci.image.layer.v1.tar".to_string(),
            digest: "sha256:abc123".to_string(),
            size: 1024,
            annotations: std::collections::HashMap::new(),
        };

        assert_eq!(layer.title(), None);
    }

    #[test]
    fn test_oci_layer_clone() {
        let mut annotations = std::collections::HashMap::new();
        annotations.insert("key".to_string(), "value".to_string());

        let layer1 = OciLayer {
            media_type: "type".to_string(),
            digest: "digest".to_string(),
            size: 100,
            annotations,
        };

        let layer2 = layer1.clone();
        assert_eq!(layer1.media_type, layer2.media_type);
        assert_eq!(layer1.digest, layer2.digest);
        assert_eq!(layer1.size, layer2.size);
    }

    #[test]
    fn test_oci_download_new() {
        let dl = OciDownload::new("org/repo:tag");
        assert_eq!(dl.reference.registry, "ghcr.io");
        assert_eq!(dl.reference.package, "org/repo");
        assert_eq!(dl.reference.tag, "tag");
        assert_eq!(dl.parallel, 1);
        assert!(!dl.extract);
    }

    #[test]
    fn test_oci_download_builder_pattern() {
        let dl = OciDownload::new("org/repo:tag")
            .api("https://custom.registry/v2")
            .output("downloads")
            .extract(true)
            .extract_to("/tmp/extract")
            .parallel(4);

        assert_eq!(dl.api, "https://custom.registry/v2");
        assert_eq!(dl.output, Some("downloads".to_string()));
        assert!(dl.extract);
        assert_eq!(dl.extract_to, Some(PathBuf::from("/tmp/extract")));
        assert_eq!(dl.parallel, 4);
    }

    #[test]
    fn test_oci_download_parallel_clamped() {
        let dl = OciDownload::new("org/repo:tag").parallel(0);
        assert_eq!(dl.parallel, 1);

        let dl = OciDownload::new("org/repo:tag").parallel(100);
        assert_eq!(dl.parallel, 100);
    }

    #[test]
    fn test_oci_manifest_deserialize() {
        let json = r#"{
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "config": {
                "mediaType": "application/vnd.oci.image.config.v1+json",
                "digest": "sha256:config123",
                "size": 512
            },
            "layers": [
                {
                    "mediaType": "application/vnd.oci.image.layer.v1.tar",
                    "digest": "sha256:layer123",
                    "size": 1024,
                    "annotations": {
                        "org.opencontainers.image.title": "file.tar.gz"
                    }
                }
            ]
        }"#;

        let manifest: OciManifest = serde_json::from_str(json).unwrap();
        assert_eq!(
            manifest.media_type,
            "application/vnd.oci.image.manifest.v1+json"
        );
        assert_eq!(manifest.config.digest, "sha256:config123");
        assert_eq!(manifest.layers.len(), 1);
        assert_eq!(manifest.layers[0].title(), Some("file.tar.gz"));
    }

    #[test]
    fn test_oci_layer_deserialize_without_annotations() {
        let json = r#"{
            "mediaType": "application/vnd.oci.image.layer.v1.tar",
            "digest": "sha256:abc",
            "size": 2048
        }"#;

        let layer: OciLayer = serde_json::from_str(json).unwrap();
        assert_eq!(layer.media_type, "application/vnd.oci.image.layer.v1.tar");
        assert_eq!(layer.digest, "sha256:abc");
        assert_eq!(layer.size, 2048);
        assert!(layer.annotations.is_empty());
    }

    #[test]
    fn test_oci_config_deserialize() {
        let json = r#"{
            "mediaType": "application/vnd.oci.image.config.v1+json",
            "digest": "sha256:xyz789",
            "size": 256
        }"#;

        let config: OciConfig = serde_json::from_str(json).unwrap();
        assert_eq!(
            config.media_type,
            "application/vnd.oci.image.config.v1+json"
        );
        assert_eq!(config.digest, "sha256:xyz789");
        assert_eq!(config.size, 256);
    }
}
