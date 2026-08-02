//! Delta downloads over zsync.
//!
//! A zsync control file describes a remote artifact block by block, which
//! answers two questions cheaply: whether the artifact differs from the copy
//! already installed, and which parts of it have to be fetched to catch up.
//! Both matter for an AppImage, where a release changes a fraction of a file
//! measured in tens of megabytes.

use std::{fs::File, path::Path};

use tracing::debug;
use zsync_rs::{checksum::calc_sha1_stream, ControlFile, HttpClient, ZsyncAssembly};

use crate::{error::DownloadError, types::Progress};

/// What a control file says the remote artifact is.
#[derive(Debug, Clone)]
pub struct ZsyncTarget {
    /// SHA-1 of the whole artifact, which is what tells two builds apart.
    pub sha1: Option<String>,
    /// Length of the artifact in bytes.
    pub length: u64,
    /// Filename the artifact is published under, where one is recorded.
    pub filename: Option<String>,
    /// When it was published, in HTTP-date form.
    pub mtime: Option<String>,
    /// Where the artifact itself is published, as the control file records it.
    /// Relative entries are resolved against the control file's own location.
    pub urls: Vec<String>,
}

impl ZsyncTarget {
    /// Where the artifact this describes can be downloaded from.
    ///
    /// A control file names its artifact, but by convention it also sits
    /// beside it under the same name, so a feed that names nothing still
    /// resolves.
    pub fn artifact_url(&self, control_url: &str) -> Option<String> {
        let base = control_url.rsplit_once('/').map(|(dir, _)| dir)?;
        match self.urls.first() {
            Some(url) if url.starts_with("http://") || url.starts_with("https://") => {
                Some(url.clone())
            }
            Some(url) => Some(format!("{base}/{url}")),
            None => control_url.strip_suffix(".zsync").map(str::to_string),
        }
    }
}

impl From<ControlFile> for ZsyncTarget {
    fn from(control: ControlFile) -> Self {
        Self {
            sha1: control.sha1,
            length: control.length,
            filename: control.filename,
            mtime: control.mtime,
            urls: control.urls,
        }
    }
}

/// The zsync feed published beside an artifact, where there is one.
///
/// A publisher that offers zsync puts the control file next to the artifact
/// under the same name, so asking for it is how to find out.
pub fn feed_beside(artifact_url: &str) -> Option<String> {
    let feed = format!("{artifact_url}.zsync");
    crate::http::Http::head(&feed).ok().map(|_| feed)
}

/// Read the control file at `url` without downloading the artifact.
pub fn fetch_target(url: &str) -> Result<ZsyncTarget, DownloadError> {
    let http = HttpClient::new();
    let control = http
        .fetch_control_file(url)
        .map_err(|e| DownloadError::Zsync(format!("fetching zsync control file: {e}")))?;
    Ok(control.into())
}

/// The SHA-1 of a file already on disk, in the same hex form a control file
/// records.
pub fn file_sha1(path: impl AsRef<Path>) -> Result<String, DownloadError> {
    let mut file = File::open(path)?;
    let digest = calc_sha1_stream(&mut file)?;
    Ok(digest.iter().map(|b| format!("{b:02x}")).collect())
}

/// Whether the artifact the control file describes differs from `installed`.
///
/// A control file without a SHA-1 leaves nothing to compare, so the artifact
/// is treated as changed rather than silently assumed current.
pub fn differs_from(target: &ZsyncTarget, installed: impl AsRef<Path>) -> bool {
    let Some(ref remote) = target.sha1 else {
        return true;
    };
    match file_sha1(installed) {
        Ok(local) => !local.eq_ignore_ascii_case(remote),
        Err(_) => true,
    }
}

/// Build `output` from the remote artifact, reusing every block `seed` already
/// holds and fetching only the rest.
///
/// The result is verified against the control file's checksums before it is
/// moved into place, so a mismatched or truncated transfer fails here rather
/// than producing a broken package.
pub fn download<F>(
    url: &str,
    seed: &Path,
    output: &Path,
    on_progress: Option<F>,
) -> Result<(), DownloadError>
where
    F: Fn(Progress) + Send + Sync + 'static,
{
    let mut assembly = ZsyncAssembly::from_url(url, output)
        .map_err(|e| DownloadError::Zsync(format!("reading zsync control file: {e}")))?;

    if let Some(callback) = on_progress {
        let total = 0;
        callback(Progress::Starting {
            total,
        });
        assembly.set_progress_callback(move |done, total| {
            callback(Progress::Chunk {
                total,
                current: done,
            });
        });
    }

    // Everything the installed copy already holds is taken from disk; only
    // what it does not is fetched.
    if seed.exists() {
        assembly
            .submit_source_file(seed)
            .map_err(|e| DownloadError::Zsync(format!("reading {}: {e}", seed.display())))?;
        let (reused, total) = assembly.block_stats();
        debug!("zsync: {reused}/{total} blocks taken from the installed copy");
    }

    while !assembly.is_complete() {
        let fetched = assembly
            .download_missing_blocks()
            .map_err(|e| DownloadError::Zsync(format!("fetching blocks: {e}")))?;
        // No progress and still incomplete means the remote will not serve
        // what is missing, and looping would spin forever.
        if fetched == 0 {
            return Err(DownloadError::Zsync(
                "zsync transfer stalled with blocks still missing".to_string(),
            ));
        }
    }

    assembly
        .complete()
        .map_err(|e| DownloadError::Zsync(format!("verifying zsync result: {e}")))
}
