use std::{
    fs::{File, Permissions},
    io::{Read as _, Write as _},
    os::unix::fs::PermissionsExt as _,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
    thread,
};

use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use soar_utils::fs::is_elf;
use tracing::{debug, trace, warn};
use ureq::http::header::{ACCEPT, AUTHORIZATION, ETAG, IF_RANGE, RANGE};

use crate::{
    download::{Download, check_partial_response},
    error::DownloadError,
    filter::Filter,
    http_client::SHARED_AGENT,
    types::{OverwriteMode, Progress, ResumeInfo},
    utils::open_output,
    xattr::{read_usable_resume, remove_resume, write_resume},
};

const DEFAULT_REGISTRY: &str = "ghcr.io";

/// Bytes a registry path segment cannot carry as they are. A tag or digest
/// read from a manifest or a reference is escaped so that it stays one
/// segment of the URL it is placed in.
const PATH_SEGMENT: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'/')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'\\')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}');

/// The registry API URL for `kind` (`manifests` or `blobs`) of `reference` in
/// `package`, each segment of the package name and the reference escaped.
fn registry_url(api: &str, package: &str, kind: &str, reference: &str) -> String {
    let package = package
        .split('/')
        .map(|segment| utf8_percent_encode(segment, PATH_SEGMENT).to_string())
        .collect::<Vec<_>>()
        .join("/");
    format!(
        "{}/{package}/{kind}/{}",
        api.trim_end_matches('/'),
        utf8_percent_encode(reference, PATH_SEGMENT)
    )
}

#[derive(Debug, Clone)]
pub struct OciReference {
    pub registry: String,
    pub package: String,
    pub tag: String,
}

impl From<&str> for OciReference {
    /// Parses an OCI reference string into an `OciReference`.
    ///
    /// The first path component names the registry when it looks like a host,
    /// that is, it contains a `.` or a `:` or is `localhost`, as in
    /// `ghcr.io/org/repo` or `localhost:5000/repo`. Otherwise the registry is
    /// `ghcr.io` and the whole string is the package. After it:
    /// - `package@sha256:<digest>` → digest used as `tag`
    /// - `package:<tag>` → tag used as `tag`, split at the last `:` after the last `/`
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
    /// let r3 = OciReference::from("localhost:5000/repo");
    /// assert_eq!(r3.registry, "localhost:5000");
    /// assert_eq!(r3.package, "repo");
    /// assert_eq!(r3.tag, "latest");
    /// ```
    fn from(value: &str) -> Self {
        let (registry, path) = match value.split_once('/') {
            Some((host, path))
                if host.contains(['.', ':']) || host.eq_ignore_ascii_case("localhost") =>
            {
                (host, path)
            }
            _ => (DEFAULT_REGISTRY, value),
        };

        let (package, tag) = if let Some((package, digest)) = path.split_once('@') {
            (package, digest)
        } else {
            let name_start = path.rfind('/').map_or(0, |i| i + 1);
            match path[name_start..].rfind(':') {
                Some(i) => (&path[..name_start + i], &path[name_start + i + 1..]),
                None => (path, "latest"),
            }
        };

        Self {
            registry: registry.to_string(),
            package: package.to_string(),
            tag: tag.to_string(),
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

/// Resolves an untrusted OCI layer title into a path confined to `output_dir`.
///
/// Only the final path component of the title is used, so titles such as
/// `../../etc/passwd`, `/etc/passwd`, or `..` cannot escape `output_dir`.
fn safe_layer_path(output_dir: &Path, title: &str) -> Result<PathBuf, DownloadError> {
    let name = Path::new(title).file_name().ok_or_else(|| {
        DownloadError::UnsafeLayerPath {
            title: title.to_string(),
        }
    })?;
    Ok(output_dir.join(name))
}

/// Whether a layer is already in place at `path`: a regular file of the
/// layer's size whose digest matches. A file that fails the digest is removed
/// so the layer is fetched again.
fn is_cached_layer(path: &Path, layer: &OciLayer) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_file() && meta.len() == layer.size)
        && verify_layer_digest(path, &layer.digest).is_ok()
}

/// Verifies the file at `path` against an OCI content-addressable digest
/// (e.g. `sha256:<hex>`). On mismatch the file is removed and an error returned.
fn verify_layer_digest(path: &Path, digest: &str) -> Result<(), DownloadError> {
    let expected = digest.strip_prefix("sha256:").unwrap_or(digest);

    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    let got: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();

    if got.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        std::fs::remove_file(path).ok();
        Err(DownloadError::DigestMismatch {
            expected: expected.to_string(),
            got,
        })
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
    /// - `api` = the reference's registry, e.g. "https://ghcr.io/v2"
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
        let reference = reference.into();
        Self {
            api: format!("https://{}/v2", reference.registry),
            reference,
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

        if let Some(cb) = &self.on_progress {
            cb(Progress::Preparing);
        }

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
    /// is extracted into `self.extract_to` (if set) or into `output_dir`. Progress counts bytes across the whole batch,
    /// so `Progress::Chunk` rises from one layer into the next and ends at the batch total.
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
        let downloaded = AtomicU64::new(0);
        let total_size: u64 = layers.iter().map(|l| l.size).sum();

        for layer in layers {
            let filename = layer.title().unwrap();
            let path = safe_layer_path(output_dir, filename)?;

            if is_cached_layer(&path, layer) {
                let current = downloaded.fetch_add(layer.size, Ordering::Relaxed) + layer.size;
                if let Some(ref cb) = self.on_progress {
                    cb(Progress::Chunk {
                        current,
                        total: total_size,
                    });
                }
                paths.push(path);
                continue;
            }

            download_layer_impl(
                &self.api,
                &self.reference,
                layer,
                &path,
                self.on_progress.as_ref(),
                &downloaded,
                total_size,
            )?;

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
        let downloaded = Arc::new(AtomicU64::new(0));
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
                        let path = match safe_layer_path(&output_dir, filename) {
                            Ok(p) => p,
                            Err(e) => {
                                lock(&errors).push((format!("{filename}: {e}"), false));
                                continue;
                            }
                        };

                        if is_cached_layer(&path, &layer) {
                            let current =
                                downloaded.fetch_add(layer.size, Ordering::Relaxed) + layer.size;
                            if let Some(ref cb) = on_progress {
                                cb(Progress::Chunk {
                                    current,
                                    total: total_size,
                                });
                            }
                            lock(&paths).push(path);
                            continue;
                        }

                        let result = download_layer_impl(
                            &api,
                            &reference,
                            &layer,
                            &path,
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
                                        lock(&errors).push((format!("{filename}: {e}"), false));
                                        continue;
                                    }
                                }
                                lock(&paths).push(path);
                            }
                            Err(e) => {
                                lock(&errors).push((format!("{filename}: {e}"), e.is_transient()));
                            }
                        }
                    }
                })
            })
            .collect();

        for handle in handles {
            if let Err(err) = handle.join() {
                lock(&errors).push((format!("Worker thread panicked: {err:?}"), false));
            }
        }

        let errors = lock(&errors);
        if !errors.is_empty() {
            return Err(DownloadError::Multiple {
                errors: errors.iter().map(|(message, _)| message.clone()).collect(),
                transient: errors.iter().all(|(_, transient)| *transient),
            });
        }

        let paths = lock(&paths).clone();
        Ok(paths)
    }

    /// Fetches the OCI/Docker manifest for the configured reference and returns it deserialized as an `OciManifest`.
    ///
    /// The request is made against the download instance's `api` base and the reference's `package`/`tag`. On
    /// non-success HTTP status codes this returns `DownloadError::HttpError`; if the response body cannot be
    /// parsed as a manifest JSON this returns `DownloadError::InvalidJson` naming why.
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
        let url = registry_url(
            &self.api,
            &self.reference.package,
            "manifests",
            &self.reference.tag,
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
            .call()
            .map_err(|err| DownloadError::request(&url, err))?;

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
            .map_err(|err| DownloadError::json(&url, err))?;

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

        let output = match self.output.as_deref() {
            Some(dir) if dir.ends_with('/') || Path::new(dir).is_dir() => {
                Path::new(dir).join(filename)
            }
            Some(path) => PathBuf::from(path),
            None => PathBuf::from(filename),
        };

        let url = registry_url(
            &self.api,
            &self.reference.package,
            "blobs",
            &self.reference.tag,
        );

        let dl = Download::new(url)
            .output(output.to_string_lossy())
            .overwrite(self.overwrite)
            .ghcr_blob();

        let dl = if let Some(ref cb) = self.on_progress {
            let cb = cb.clone();
            dl.progress(move |p| cb(p))
        } else {
            dl
        };

        let path = dl.execute()?;

        verify_layer_digest(&path, &self.reference.tag)?;

        Ok(vec![path])
    }

    /// Downloads a single OCI layer blob to the given file path, advancing the
    /// cumulative `downloaded` byte counter by this layer's bytes and emitting
    /// progress events against it if configured.
    ///
    /// # Parameters
    ///
    /// - `layer`: The manifest layer to download.
    /// - `path`: Destination filesystem path for the downloaded blob.
    /// - `downloaded`: Cumulative byte counter for the batch the layer belongs to.
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
        let counter = AtomicU64::new(*downloaded);
        let result = download_layer_impl(
            &self.api,
            &self.reference,
            layer,
            path,
            self.on_progress.as_ref(),
            &counter,
            total_size,
        );
        *downloaded = counter.into_inner();
        result
    }
}

/// Locks `mutex`, carrying on with its contents if a worker panicked while
/// holding it, so one failed layer is reported rather than losing the rest.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Downloads a single OCI layer blob to the given file path with resume support, progress reporting, and post-download handling.
///
/// This function:
/// - Resumes partially downloaded blobs when usable resume metadata exists (uses Range and If-Range headers).
/// - Appends to the existing file when a partial response continuing it is returned, otherwise starts over.
/// - Periodically persists resume metadata while downloading (every 1 MiB).
/// - Advances `downloaded`, the counter shared by every layer of the batch, and reports it in `Progress::Chunk`.
/// - Marks the file executable (0o755) if it appears to be an ELF binary and removes any resume metadata on success.
///
/// Returns `Ok(())` on success or a `DownloadError` on failure.
fn download_layer_impl(
    api: &str,
    reference: &OciReference,
    layer: &OciLayer,
    path: &Path,
    on_progress: Option<&Arc<dyn Fn(Progress) + Send + Sync>>,
    downloaded: &AtomicU64,
    total_size: u64,
) -> Result<(), DownloadError> {
    let url = registry_url(api, &reference.package, "blobs", &layer.digest);

    trace!(
        digest = layer.digest,
        size = layer.size,
        path = %path.display(),
        "downloading layer"
    );

    let mut resume_info = read_usable_resume(path);
    let resp = loop {
        let mut req = SHARED_AGENT.get(&url).header(AUTHORIZATION, "Bearer QQ==");
        if let Some(info) = &resume_info {
            trace!(
                resume_from = info.downloaded,
                "attempting to resume download"
            );
            req = req.header(RANGE, &format!("bytes={}-", info.downloaded));
            if let Some(tag) = &info.etag {
                req = req.header(IF_RANGE, tag);
            }
        }

        let resp = match req.call() {
            Err(ureq::Error::StatusCode(416)) if resume_info.is_some() => {
                warn!(
                    digest = layer.digest,
                    "registry rejected the resume range, restarting"
                );
                resume_info = None;
                continue;
            }
            resp => resp.map_err(|err| DownloadError::request(&url, err))?,
        };

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

        match &resume_info {
            Some(info) if resp.status() != 206 || check_partial_response(&resp, info).is_err() => {
                debug!(
                    digest = layer.digest,
                    "response does not continue the file, restarting"
                );
                resume_info = None;
            }
            _ => break resp,
        }
    };

    let resume_from = resume_info.as_ref().map(|info| info.downloaded);
    let mut file = open_output(path, resume_from)?;

    if let Some(resume_offset) = resume_from {
        let current = downloaded.fetch_add(resume_offset, Ordering::Relaxed) + resume_offset;
        if let Some(cb) = on_progress {
            cb(Progress::Resuming {
                current,
                total: total_size,
            });
        }
    }

    let new_etag = resp
        .headers()
        .get(ETAG)
        .and_then(|h| h.to_str().ok())
        .map(String::from);
    let mut reader = resp.into_body().into_reader();
    let mut buffer = [0u8; 8192];
    let mut layer_downloaded = resume_from.unwrap_or(0);
    let mut last_checkpoint = layer_downloaded / (1024 * 1024);

    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }

        file.write_all(&buffer[..n])?;
        layer_downloaded += n as u64;

        let current = downloaded.fetch_add(n as u64, Ordering::Relaxed) + n as u64;

        let checkpoint = layer_downloaded / (1024 * 1024);
        if checkpoint > last_checkpoint {
            last_checkpoint = checkpoint;
            if let Err(err) = write_resume(
                path,
                &ResumeInfo {
                    downloaded: layer_downloaded,
                    total: layer.size,
                    etag: new_etag.clone(),
                    last_modified: None,
                },
            ) {
                trace!(%err, "failed to save resume checkpoint");
            }
        }

        if let Some(cb) = on_progress {
            cb(Progress::Chunk {
                current,
                total: total_size,
            });
        }
    }

    verify_layer_digest(path, &layer.digest)?;

    if is_elf(path) {
        trace!(path = %path.display(), "setting executable permissions on ELF binary");
        file.set_permissions(Permissions::from_mode(0o755))?;
    }

    if let Err(err) = remove_resume(path) {
        warn!(path = %path.display(), %err, "failed to clear resume state");
    }
    trace!(
        path = %path.display(),
        bytes = layer_downloaded,
        "layer download complete"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_layer_digest_accepts_correct_and_rejects_wrong() {
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("layer.bin");
        {
            let mut f = File::create(&path).unwrap();
            f.write_all(b"layer-bytes").unwrap();
        }

        let mut hasher = Sha256::new();
        hasher.update(b"layer-bytes");
        let hex: String = hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();

        assert!(verify_layer_digest(&path, &format!("sha256:{hex}")).is_ok());
        assert!(path.exists());

        match verify_layer_digest(&path, "sha256:00ff") {
            Err(DownloadError::DigestMismatch {
                ..
            }) => {}
            other => panic!("expected DigestMismatch, got {other:?}"),
        }
        assert!(!path.exists(), "file should be removed on digest mismatch");
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    fn layer(title: &str, bytes: &[u8]) -> OciLayer {
        OciLayer {
            media_type: "application/octet-stream".into(),
            digest: format!("sha256:{}", sha256_hex(bytes)),
            size: bytes.len() as u64,
            annotations: [("org.opencontainers.image.title".into(), title.into())].into(),
        }
    }

    /// Serves `layers` as blobs of `org/repo` and returns a downloader for it
    /// that records every `Progress::Chunk` position.
    fn serve_layers(layers: &[(&OciLayer, &[u8])]) -> (OciDownload, Arc<Mutex<Vec<u64>>>) {
        let routes = layers
            .iter()
            .map(|(layer, bytes)| {
                (
                    format!("/v2/org/repo/blobs/{}", layer.digest),
                    bytes.to_vec(),
                )
            })
            .collect();
        let base = crate::test_server::serve(routes);
        let seen = Arc::new(Mutex::new(Vec::new()));
        let record = seen.clone();
        let dl = OciDownload::new("org/repo:tag")
            .api(format!("{base}/v2"))
            .progress(move |p| {
                if let Progress::Chunk {
                    current, ..
                } = p
                {
                    record.lock().unwrap().push(current);
                }
            });
        (dl, seen)
    }

    #[test]
    fn sequential_progress_rises_across_layers() {
        let first = vec![1u8; 20_000];
        let second = vec![2u8; 30_000];
        let (l1, l2) = (layer("one", &first), layer("two", &second));
        let (dl, seen) = serve_layers(&[(&l1, &first), (&l2, &second)]);
        let dir = tempfile::tempdir().unwrap();

        dl.download_layers_sequential(&[&l1, &l2], dir.path())
            .unwrap();

        let seen = seen.lock().unwrap();
        assert!(seen.windows(2).all(|w| w[0] <= w[1]), "{seen:?}");
        assert_eq!(seen.last(), Some(&50_000));
        assert_eq!(std::fs::read(dir.path().join("two")).unwrap(), second);
    }

    #[test]
    fn a_cached_layer_counts_toward_progress() {
        let first = vec![1u8; 20_000];
        let second = vec![2u8; 30_000];
        let (l1, l2) = (layer("one", &first), layer("two", &second));
        let (dl, seen) = serve_layers(&[(&l1, &first), (&l2, &second)]);
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("one"), &first).unwrap();

        dl.download_layers_sequential(&[&l1, &l2], dir.path())
            .unwrap();

        let seen = seen.lock().unwrap();
        assert_eq!(seen.first(), Some(&20_000));
        assert!(seen.windows(2).all(|w| w[0] <= w[1]), "{seen:?}");
        assert_eq!(seen.last(), Some(&50_000));
    }

    #[test]
    fn a_tampered_cached_layer_is_fetched_again() {
        let first = vec![1u8; 20_000];
        let l1 = layer("one", &first);
        let (dl, _) = serve_layers(&[(&l1, &first)]);
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("one"), vec![9u8; 20_000]).unwrap();

        dl.download_layers_sequential(&[&l1], dir.path()).unwrap();
        assert_eq!(std::fs::read(dir.path().join("one")).unwrap(), first);
    }

    #[test]
    fn a_symlink_planted_at_a_layer_path_is_not_followed() {
        let first = vec![1u8; 20_000];
        let l1 = layer("one", &first);
        let (dl, _) = serve_layers(&[(&l1, &first)]);
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        std::os::unix::fs::symlink(&target, dir.path().join("one")).unwrap();

        assert!(dl.download_layers_sequential(&[&l1], dir.path()).is_err());
        assert!(!target.exists(), "the layer was written through the link");
    }

    fn manifest(layers: &[&OciLayer]) -> Vec<u8> {
        let layers: Vec<_> = layers
            .iter()
            .map(|l| {
                serde_json::json!({
                    "mediaType": l.media_type,
                    "digest": l.digest,
                    "size": l.size,
                    "annotations": l.annotations,
                })
            })
            .collect();
        serde_json::to_vec(&serde_json::json!({
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "config": {"mediaType": "application/vnd.oci.image.config.v1+json", "digest": "sha256:00", "size": 2},
            "layers": layers,
        }))
        .unwrap()
    }

    /// Serves an image of `org/repo:tag` holding `layers`, with its manifest.
    fn serve_image(layers: &[(&OciLayer, &[u8])]) -> String {
        let mut routes: Vec<(String, Vec<u8>)> = layers
            .iter()
            .map(|(layer, bytes)| {
                (
                    format!("/v2/org/repo/blobs/{}", layer.digest),
                    bytes.to_vec(),
                )
            })
            .collect();
        let only: Vec<_> = layers.iter().map(|(l, _)| *l).collect();
        routes.push(("/v2/org/repo/manifests/tag".into(), manifest(&only)));
        crate::test_server::serve(routes)
    }

    fn image_layers() -> Vec<(OciLayer, Vec<u8>)> {
        [
            ("tool", 1u8, 40_000),
            ("tool.png", 2, 9_000),
            ("tool.log", 3, 12_000),
            ("extra", 4, 30_000),
        ]
        .into_iter()
        .map(|(title, byte, len)| {
            let bytes = vec![byte; len];
            (layer(title, &bytes), bytes)
        })
        .collect()
    }

    fn run_image(parallel: usize, filter: Option<&str>) {
        let layers = image_layers();
        let served: Vec<_> = layers.iter().map(|(l, b)| (l, b.as_slice())).collect();
        let base = serve_image(&served);
        let dir = tempfile::tempdir().unwrap();

        let seen = Arc::new(Mutex::new(Vec::new()));
        let record = seen.clone();
        let mut dl = OciDownload::new("org/repo:tag")
            .api(format!("{base}/v2"))
            .output(dir.path().to_string_lossy())
            .parallel(parallel)
            .progress(move |p| record.lock().unwrap().push(p));
        if let Some(glob) = filter {
            dl = dl.filter(Filter {
                globs: vec![glob.to_string()],
                ..Default::default()
            });
        }

        let mut paths = dl.execute().unwrap();
        paths.sort();

        let wanted: Vec<_> = layers
            .iter()
            .filter(|(l, _)| filter.is_none_or(|g| fast_glob::glob_match(g, l.title().unwrap())))
            .collect();
        let mut expected: Vec<_> = wanted
            .iter()
            .map(|(l, _)| dir.path().join(l.title().unwrap()))
            .collect();
        expected.sort();
        assert_eq!(paths, expected);
        for (l, bytes) in &wanted {
            assert_eq!(
                &std::fs::read(dir.path().join(l.title().unwrap())).unwrap(),
                bytes
            );
        }
        for (l, _) in layers
            .iter()
            .filter(|(l, _)| !wanted.iter().any(|(w, _)| w.digest == l.digest))
        {
            assert!(!dir.path().join(l.title().unwrap()).exists());
        }

        let total: u64 = wanted.iter().map(|(l, _)| l.size).sum();
        let seen = seen.lock().unwrap();
        let chunks: Vec<u64> = seen
            .iter()
            .filter_map(|p| {
                match p {
                    Progress::Chunk {
                        current, ..
                    } => Some(*current),
                    _ => None,
                }
            })
            .collect();
        assert_eq!(chunks.iter().max(), Some(&total));
        assert!(matches!(seen.last(), Some(Progress::Complete { total: t }) if *t == total));
    }

    #[test]
    fn an_image_downloads_sequentially() {
        run_image(1, None);
    }

    #[test]
    fn an_image_downloads_in_parallel() {
        run_image(3, None);
    }

    #[test]
    fn a_filter_picks_layers_in_parallel() {
        run_image(4, Some("tool*"));
    }

    #[test]
    fn a_filter_matching_nothing_is_layer_not_found() {
        let layers = image_layers();
        let served: Vec<_> = layers.iter().map(|(l, b)| (l, b.as_slice())).collect();
        let base = serve_image(&served);
        let dir = tempfile::tempdir().unwrap();
        let err = OciDownload::new("org/repo:tag")
            .api(format!("{base}/v2"))
            .output(dir.path().to_string_lossy())
            .filter(Filter {
                globs: vec!["nothing*".into()],
                ..Default::default()
            })
            .execute()
            .unwrap_err();
        assert!(matches!(err, DownloadError::LayerNotFound), "{err:?}");
    }

    #[test]
    fn a_parallel_failure_reports_the_layer_and_keeps_the_rest() {
        let layers = image_layers();
        let (bad, _) = &layers[1];
        let served: Vec<_> = layers
            .iter()
            .filter(|(l, _)| l.digest != bad.digest)
            .map(|(l, b)| (l, b.as_slice()))
            .collect();
        let mut routes: Vec<(String, Vec<u8>)> = served
            .iter()
            .map(|(l, b)| (format!("/v2/org/repo/blobs/{}", l.digest), b.to_vec()))
            .collect();
        let all: Vec<_> = layers.iter().map(|(l, _)| l).collect();
        routes.push(("/v2/org/repo/manifests/tag".into(), manifest(&all)));
        let base = crate::test_server::serve(routes);
        let dir = tempfile::tempdir().unwrap();

        let err = OciDownload::new("org/repo:tag")
            .api(format!("{base}/v2"))
            .output(dir.path().to_string_lossy())
            .parallel(4)
            .execute()
            .unwrap_err();
        match err {
            DownloadError::Multiple {
                errors,
                transient,
            } => {
                assert_eq!(errors.len(), 1, "{errors:?}");
                assert!(errors[0].starts_with("tool.png:"), "{errors:?}");
                assert!(!transient, "a 404 is not worth retrying");
            }
            other => panic!("expected Multiple, got {other:?}"),
        }
        assert!(dir.path().join("tool").exists());
    }

    #[test]
    fn a_parallel_batch_that_failed_on_the_server_is_retried() {
        let layers = image_layers();
        let all: Vec<_> = layers.iter().map(|(l, _)| l).collect();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let body = manifest(&all);
        thread::spawn(move || {
            use std::io::{BufRead, BufReader};
            for mut stream in listener.incoming().flatten() {
                let mut line = String::new();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                reader.read_line(&mut line).unwrap();
                while reader.read_line(&mut String::new()).unwrap_or(0) > 2 {}
                let reply = if line.contains("/manifests/") {
                    let mut reply = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    )
                    .into_bytes();
                    reply.extend_from_slice(&body);
                    reply
                } else {
                    b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()
                };
                let _ = stream.write_all(&reply);
            }
        });
        let dir = tempfile::tempdir().unwrap();

        let err = OciDownload::new("org/repo:tag")
            .api(format!("http://{addr}/v2"))
            .output(dir.path().to_string_lossy())
            .parallel(4)
            .execute()
            .unwrap_err();
        assert!(
            matches!(
                err,
                DownloadError::Multiple {
                    transient: true,
                    ..
                }
            ),
            "{err:?}"
        );
        assert!(err.is_transient());
    }

    #[test]
    fn a_layer_resumes_from_its_checkpoint() {
        let bytes: Vec<u8> = (0..50_000u32).map(|i| (i % 253) as u8).collect();
        let l1 = layer("one", &bytes);
        let (dl, _) = serve_layers(&[(&l1, &bytes)]);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("one");
        std::fs::write(&path, &bytes[..30_000]).unwrap();
        let info = ResumeInfo {
            downloaded: 20_000,
            total: bytes.len() as u64,
            etag: None,
            last_modified: None,
        };
        if write_resume(&path, &info).is_err() {
            eprintln!("skipping: filesystem lacks user xattrs");
            return;
        }

        let resumed = Arc::new(Mutex::new(None));
        let seen = resumed.clone();
        let dl = dl.progress(move |p| {
            if let Progress::Resuming {
                current, ..
            } = p
            {
                *seen.lock().unwrap() = Some(current);
            }
        });
        let mut downloaded = 0;
        dl.download_layer(&l1, &path, &mut downloaded, l1.size)
            .unwrap();

        assert_eq!(*resumed.lock().unwrap(), Some(20_000));
        assert_eq!(downloaded, 50_000);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert!(crate::xattr::read_resume(&path).is_none());
    }

    #[test]
    fn a_blob_lands_in_an_output_directory_under_the_package_name() {
        let bytes = vec![5u8; 25_000];
        let digest = format!("sha256:{}", sha256_hex(&bytes));
        let base = crate::test_server::serve(vec![(
            format!("/v2/org/repo/blobs/{digest}"),
            bytes.clone(),
        )]);
        let dir = tempfile::tempdir().unwrap();

        let paths = OciDownload::new(format!("org/repo@{digest}").as_str())
            .api(format!("{base}/v2"))
            .output(dir.path().to_string_lossy())
            .overwrite(OverwriteMode::Force)
            .execute()
            .unwrap();
        assert_eq!(paths, vec![dir.path().join("repo")]);
        assert_eq!(std::fs::read(&paths[0]).unwrap(), bytes);
    }

    #[test]
    fn a_blob_that_fails_its_digest_is_removed() {
        let bytes = vec![5u8; 25_000];
        let digest = format!("sha256:{}", sha256_hex(b"something else"));
        let base = crate::test_server::serve(vec![(format!("/v2/org/repo/blobs/{digest}"), bytes)]);
        let dir = tempfile::tempdir().unwrap();

        let err = OciDownload::new(format!("org/repo@{digest}").as_str())
            .api(format!("{base}/v2"))
            .output(dir.path().to_string_lossy())
            .overwrite(OverwriteMode::Force)
            .execute()
            .unwrap_err();
        assert!(
            matches!(err, DownloadError::DigestMismatch { .. }),
            "{err:?}"
        );
        assert!(!dir.path().join("repo").exists());
    }

    #[test]
    fn a_missing_manifest_is_an_http_error() {
        let base = crate::test_server::serve(vec![]);
        let err = OciDownload::new("org/repo:tag")
            .api(format!("{base}/v2"))
            .fetch_manifest()
            .unwrap_err();
        assert!(
            matches!(
                err,
                DownloadError::HttpError {
                    status: 404,
                    ..
                }
            ),
            "{err:?}"
        );
    }

    #[test]
    fn a_truncated_manifest_names_the_parse_error() {
        let base = crate::test_server::serve(vec![(
            "/v2/org/repo/manifests/tag".into(),
            br#"{"mediaType": "x", "layers": ["#.to_vec(),
        )]);
        let err = OciDownload::new("org/repo:tag")
            .api(format!("{base}/v2"))
            .fetch_manifest()
            .unwrap_err();
        match err {
            DownloadError::InvalidJson {
                reason, ..
            } => assert!(reason.contains("EOF"), "{reason}"),
            other => panic!("expected InvalidJson, got {other:?}"),
        }
    }

    #[test]
    fn registry_url_keeps_segments_whole() {
        assert_eq!(
            registry_url("https://ghcr.io/v2/", "org/repo", "manifests", "1.0"),
            "https://ghcr.io/v2/org/repo/manifests/1.0"
        );
        assert_eq!(
            registry_url(
                "https://ghcr.io/v2",
                "org/re po",
                "blobs",
                "sha256:ab/../x?y#z"
            ),
            "https://ghcr.io/v2/org/re%20po/blobs/sha256:ab%2F..%2Fx%3Fy%23z"
        );
    }

    #[test]
    fn test_oci_reference_with_registry_port() {
        let reference = OciReference::from("localhost:5000/repo");
        assert_eq!(reference.registry, "localhost:5000");
        assert_eq!(reference.package, "repo");
        assert_eq!(reference.tag, "latest");

        let reference = OciReference::from("localhost:5000/org/repo:v1");
        assert_eq!(reference.registry, "localhost:5000");
        assert_eq!(reference.package, "org/repo");
        assert_eq!(reference.tag, "v1");

        let dl = OciDownload::new("registry.example.com/org/repo:v1");
        assert_eq!(dl.api, "https://registry.example.com/v2");
    }

    #[test]
    fn safe_layer_path_keeps_plain_name_inside_output_dir() {
        let out = Path::new("/tmp/soar-out");
        let path = safe_layer_path(out, "tool.AppImage").unwrap();
        assert_eq!(path, out.join("tool.AppImage"));
    }

    #[test]
    fn safe_layer_path_strips_traversal_and_absolute_titles() {
        let out = Path::new("/tmp/soar-out");
        // `..`, nested escapes, and absolute paths all collapse to the final
        // component joined under the output dir.
        assert_eq!(
            safe_layer_path(out, "../../etc/passwd").unwrap(),
            out.join("passwd")
        );
        assert_eq!(
            safe_layer_path(out, "/etc/cron.d/evil").unwrap(),
            out.join("evil")
        );
        assert_eq!(safe_layer_path(out, "a/b/c").unwrap(), out.join("c"));
    }

    #[test]
    fn safe_layer_path_rejects_titles_without_a_final_component() {
        let out = Path::new("/tmp/soar-out");
        for bad in ["", "..", "foo/..", "/", "."] {
            assert!(
                matches!(
                    safe_layer_path(out, bad),
                    Err(DownloadError::UnsafeLayerPath { .. })
                ),
                "expected {bad:?} to be rejected"
            );
        }
    }

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
