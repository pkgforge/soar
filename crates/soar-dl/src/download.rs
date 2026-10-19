use std::{
    fs::{self, File, Permissions},
    io::{Read as _, Write as _},
    os::unix::fs::PermissionsExt as _,
    path::{Path, PathBuf},
    sync::Arc,
};

use soar_utils::fs::is_elf;
use tracing::{debug, trace, warn};
use ureq::{
    Body,
    http::{
        Response,
        header::{CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_RANGE, ETAG},
    },
};

use crate::{
    error::DownloadError,
    http::Http,
    types::{OverwriteMode, Progress, ResumeInfo},
    utils::{filename_from_header, filename_from_url, open_output, resolve_output_path},
    xattr::{read_usable_resume, remove_resume, write_resume},
};

#[derive(Clone)]
pub struct Download {
    pub url: String,
    pub output: Option<String>,
    pub overwrite: OverwriteMode,
    pub extract: bool,
    pub extract_to: Option<PathBuf>,
    pub on_progress: Option<Arc<dyn Fn(Progress) + Send + Sync>>,
    pub ghcr_blob: bool,
    pub expected_checksum: Option<String>,
}

impl Download {
    /// Creates a new `Download` configured for the given URL with sensible defaults.
    ///
    /// The returned builder defaults to:
    /// - no explicit output path (downloaded filename will be resolved automatically),
    /// - `OverwriteMode::Prompt` for existing files,
    /// - extraction disabled,
    /// - no extraction destination,
    /// - no progress callback.
    ///
    /// # Examples
    ///
    /// ```
    /// use soar_dl::download::Download;
    ///
    /// let dl = Download::new("https://example.com/archive.tar.gz")
    ///     .output("archive.tar.gz");
    /// // `dl` is ready to call `execute()`
    /// ```
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            output: None,
            overwrite: OverwriteMode::Prompt,
            extract: false,
            extract_to: None,
            on_progress: None,
            ghcr_blob: false,
            expected_checksum: None,
        }
    }

    /// Sets the expected blake3 checksum (hex) to verify the downloaded file
    /// against before it is made executable or extracted.
    ///
    /// # Examples
    ///
    /// ```
    /// use soar_dl::download::Download;
    ///
    /// let _ = Download::new("https://example.com/file").checksum("abcdef123456");
    /// ```
    pub fn checksum(mut self, checksum: impl Into<String>) -> Self {
        self.expected_checksum = Some(checksum.into());
        self
    }

    /// Turns on GHCR blob support.
    ///
    /// When enabled, the `Authorization` header is set to `Bearer QQ==`
    ///
    /// # Examples
    ///
    /// ```
    /// use soar_dl::download::Download;
    ///
    /// let dl = Download::new("https://example.com/archive.tar.gz")
    ///     .ghcr_blob();
    /// ```
    pub fn ghcr_blob(mut self) -> Self {
        self.ghcr_blob = true;
        self
    }

    /// Sets the download output destination.
    ///
    /// The `output` value may be a filesystem path or `"-"` to write to stdout.
    ///
    /// # Returns
    ///
    /// The modified `Download` builder.
    ///
    /// # Examples
    ///
    /// ```
    /// use soar_dl::download::Download;
    ///
    /// let _ = Download::new("https://example.com/file").output("path/to/file");
    /// ```
    pub fn output(mut self, output: impl Into<String>) -> Self {
        self.output = Some(output.into());
        self
    }

    /// Sets how existing destination files are handled when performing the download.
    ///
    /// Sets the overwrite mode that determines what to do if the resolved output path already exists. The method returns the modified `Download` builder to allow chaining.
    ///
    /// # Examples
    ///
    /// ```
    /// use soar_dl::download::Download;
    /// use soar_dl::types::OverwriteMode;
    ///
    /// let d = Download::new("https://example.com/file")
    ///     .overwrite(OverwriteMode::Force)
    ///     .output("file.bin");
    /// ```
    pub fn overwrite(mut self, overwrite: OverwriteMode) -> Self {
        self.overwrite = overwrite;
        self
    }

    /// Enable or disable extraction of the downloaded archive after a successful download.
    ///
    /// When set to `true`, the downloader will extract the downloaded archive to the configured
    /// extraction directory (or to the output file's parent directory if no extraction target was set).
    ///
    /// # Examples
    ///
    /// ```
    /// use soar_dl::download::Download;
    ///
    /// let dl = Download::new("https://example.com/archive.tar.gz")
    ///     .extract(true);
    /// ```
    pub fn extract(mut self, extract: bool) -> Self {
        self.extract = extract;
        self
    }

    /// Set the directory to extract a downloaded archive into.
    ///
    /// When `extract` is enabled on the downloader, the downloaded archive will be
    /// extracted into this path instead of the default (the downloaded file's
    /// parent directory).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use soar_dl::download::Download;
    ///
    /// let dl = Download::new("https://example.com/archive.tar.gz")
    ///     .extract(true)
    ///     .extract_to("/tmp/my-extract-dir");
    /// ```
    pub fn extract_to(mut self, extract_to: impl Into<PathBuf>) -> Self {
        self.extract_to = Some(extract_to.into());
        self
    }

    /// Registers a progress callback that will be invoked with `Progress` events during the download lifecycle.
    ///
    /// The provided closure is stored and called for events such as `Progress::Starting`, `Progress::Chunk`, and `Progress::Complete`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use soar_dl::download::Download;
    /// use soar_dl::types::Progress;
    ///
    /// let _dl = Download::new("https://example.com/file")
    ///     .progress(|event: Progress| match event {
    ///         Progress::Starting { total } => eprintln!("starting, total={}", total),
    ///         Progress::Chunk { total, current } => eprintln!("downloaded {} (+{})", total, current),
    ///         Progress::Complete { total } => eprintln!("complete, total={}", total),
    ///         _ => {}
    ///     });
    /// ```
    pub fn progress<F>(mut self, on_progress: F) -> Self
    where
        F: Fn(Progress) + Send + Sync + 'static,
    {
        self.on_progress = Some(Arc::new(on_progress));
        self
    }

    /// Performs the configured download and returns the final output path.
    ///
    /// The method downloads the URL configured in this `Download` instance to the resolved
    /// output location (or to stdout when the configured output is `"-"`). It creates parent
    /// directories as needed, respects the configured overwrite mode (skip, force, or prompt),
    /// supports resuming interrupted downloads when resume metadata is available, and persists
    /// resume state during an active download. After a successful download, it clears any
    /// stored resume metadata, sets the executable bit on ELF binaries, and—if extraction was
    /// requested—extracts the archive into the configured destination directory.
    ///
    /// # Returns
    ///
    /// `Ok(PathBuf)` containing the filesystem path to the downloaded file (or `PathBuf::from("-")`
    /// when written to stdout), or `Err(DownloadError)` on failure.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use soar_dl::download::Download;
    ///
    /// let dl = Download::new("https://example.com/archive.tar.gz")
    ///     .output("archive.tar.gz")
    ///     .extract(true);
    /// let path = dl.execute().expect("download failed");
    /// assert!(path.ends_with("archive.tar.gz"));
    /// ```
    pub fn execute(self) -> Result<PathBuf, DownloadError> {
        debug!(url = self.url, "starting download");

        if let Some(ref cb) = self.on_progress {
            cb(Progress::Preparing);
        }

        if self.output.as_deref() == Some("-") {
            trace!("output is stdout");
            if self.expected_checksum.is_some() {
                return Err(DownloadError::ChecksumOnStdout);
            }
            return self.download_to_stdout();
        }

        let needs_head = match self.output.as_deref() {
            None => true,
            Some("-") => false,
            Some(p) => {
                let path = Path::new(p);
                p.ends_with("/") || path.is_dir()
            }
        };

        let mut head_size = None;
        let (header_filename, url_filename) = if needs_head {
            trace!("performing HEAD request for filename");
            let resp = Http::head(&self.url)?;
            head_size = content_length(&resp);
            (
                resp.headers()
                    .get(CONTENT_DISPOSITION)
                    .and_then(filename_from_header),
                filename_from_url(&self.url),
            )
        } else {
            (None, filename_from_url(&self.url))
        };

        let output_path =
            resolve_output_path(self.output.as_deref(), url_filename, header_filename)?;
        debug!(path = %output_path.display(), "resolved output path");

        let mut resume_info = read_usable_resume(&output_path);
        if resume_info.is_some() {
            trace!("found resume information from previous download");
        }

        if output_path.is_file() {
            match self.overwrite {
                OverwriteMode::Skip => {
                    // Only skip if there's no resume info (complete download)
                    // If resume info exists, it's a partial download that should continue
                    if resume_info.is_none() {
                        if self.is_complete(&output_path, head_size) {
                            debug!(path = %output_path.display(), "file exists, skipping download");
                            return Ok(output_path);
                        }
                        warn!(path = %output_path.display(), "existing file is not a complete download, re-downloading");
                        fs::remove_file(&output_path)?;
                    } else {
                        debug!(path = %output_path.display(), "file exists but is partial, resuming download");
                    }
                }
                OverwriteMode::Force => {
                    debug!(path = %output_path.display(), "file exists, forcing overwrite");
                    fs::remove_file(&output_path)?;
                    resume_info = None;
                }
                OverwriteMode::Prompt => {
                    if resume_info.is_none() {
                        if !prompt_overwrite(&output_path)? {
                            debug!(path = %output_path.display(), "user declined overwrite");
                            return Ok(output_path);
                        }
                        fs::remove_file(&output_path)?;
                        resume_info = None;
                    }
                }
            }
        }

        if let Some(parent) = output_path.parent() {
            trace!(path = %parent.display(), "creating parent directories");
            std::fs::create_dir_all(parent)?;
        }

        let file = self.download_to_file(&output_path, resume_info)?;

        if let Err(e) = self.verify_checksum(&output_path) {
            fs::remove_file(&output_path).ok();
            return Err(e);
        }

        if is_elf(&output_path) {
            trace!(path = %output_path.display(), "detected ELF binary, setting executable permissions");
            file.set_permissions(Permissions::from_mode(0o755))?;
        }

        if let Err(err) = remove_resume(&output_path) {
            warn!(path = %output_path.display(), %err, "failed to clear resume state");
        }

        // Extraction is driven by what the file actually is, not by what the
        // caller guessed it would be. compak detects by magic number, so an
        // ELF (an AppImage, say) is never mistaken for an archive even when
        // extraction was requested.
        if self.extract {
            match compak::detect_from_file(&output_path) {
                Err(
                    err @ compak::error::ArchiveError::Io {
                        ..
                    },
                ) => return Err(err.into()),
                Ok(format) => {
                    let extract_dir = self.extract_to.unwrap_or_else(|| {
                        output_path
                            .parent()
                            .map(PathBuf::from)
                            .unwrap_or_else(|| PathBuf::from("."))
                    });
                    debug!(archive = %output_path.display(), dest = %extract_dir.display(),
                           ?format, "extracting archive");
                    // A failure here fails the install. A bare .gz of a single
                    // file shares its magic number with the tar-wrapped form,
                    // but detection decompresses far enough to tell the two
                    // apart, so what is left is a download that really is
                    // broken, and swallowing that installs an empty package.
                    compak::extract_archive(&output_path, &extract_dir)?;
                }
                Err(_) => {
                    trace!(path = %output_path.display(),
                           "not an archive, installing as-is");
                }
            }
        }

        debug!(path = %output_path.display(), "download completed successfully");
        Ok(output_path)
    }

    /// Whether the file already at `path` is this download, finished: it
    /// matches the expected checksum, or, without one, the size the server
    /// reports. Existence alone proves nothing, since an interrupted download
    /// on a filesystem without xattrs leaves no resume state behind.
    fn is_complete(&self, path: &Path, head_size: Option<u64>) -> bool {
        if self.expected_checksum.is_some() {
            return self.verify_checksum(path).is_ok();
        }
        let remote_size =
            head_size.or_else(|| Http::head(&self.url).ok().as_ref().and_then(content_length));
        remote_size.is_some_and(|size| fs::metadata(path).is_ok_and(|meta| meta.len() == size))
    }

    fn verify_checksum(&self, path: &Path) -> Result<(), DownloadError> {
        let Some(ref expected) = self.expected_checksum else {
            return Ok(());
        };
        let actual = soar_utils::hash::calculate_checksum(path)
            .map_err(|e| DownloadError::Io(std::io::Error::other(e.to_string())))?;
        if actual.eq_ignore_ascii_case(expected) {
            Ok(())
        } else {
            Err(DownloadError::ChecksumMismatch {
                expected: expected.clone(),
                got: actual,
            })
        }
    }

    /// Streams the HTTP response body for this download's URL to standard output.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    ///
    /// // The function returns a PathBuf `"-"` to indicate stdout was used.
    /// let stdout_path = PathBuf::from("-");
    /// assert_eq!(stdout_path.to_str(), Some("-"));
    /// ```
    ///
    /// # Returns
    ///
    /// `PathBuf::from("-")` on success.
    fn download_to_stdout(&self) -> Result<PathBuf, DownloadError> {
        let resp = Http::fetch(&self.url, None, None, self.ghcr_blob)?;
        let mut stdout = std::io::stdout();
        let mut reader = resp.into_body().into_reader();

        std::io::copy(&mut reader, &mut stdout)?;
        stdout.flush()?;

        Ok(PathBuf::from("-"))
    }

    /// Download the HTTP response body into the given file path, using resume metadata when available and emitting progress events.
    ///
    /// When `resume_info` is provided the method asks for the bytes past the recorded offset. The download restarts from the
    /// beginning when the server answers with anything but a partial response starting at that offset, or rejects the range.
    /// Progress callbacks (if configured on `self`) are invoked with `Progress::Starting`, `Progress::Chunk`, and `Progress::Complete`.
    ///
    /// # Parameters
    ///
    /// - `path`: destination filesystem path to write the downloaded bytes to. If resuming, the file is opened for append; otherwise it is (re)created.
    /// - `resume_info`: optional resume metadata describing previously downloaded bytes and a prior `ETag`; when present the method will request a ranged response starting at `resume_info.downloaded`.
    ///
    /// # Returns
    ///
    /// The written file on successful completion, or `Err(DownloadError)` on IO, HTTP, or resume-state persistence failures.
    fn download_to_file(
        &self,
        path: &Path,
        resume_info: Option<ResumeInfo>,
    ) -> Result<File, DownloadError> {
        let (resume_from, etag) = resume_info
            .as_ref()
            .map(|r| (Some(r.downloaded), r.etag.as_deref()))
            .unwrap_or((None, None));

        if let Some(offset) = resume_from {
            debug!(offset = offset, "attempting to resume download");
        }

        let resp = match Http::fetch(&self.url, resume_from, etag, self.ghcr_blob) {
            Err(DownloadError::HttpError {
                status: 416, ..
            }) if resume_from.is_some() => {
                warn!("server rejected the resume range, restarting download");
                return self.download_to_file(path, None);
            }
            resp => resp?,
        };

        let status = resp.status();
        trace!(status = status.as_u16(), "received HTTP response");

        if let Some(info) = &resume_info {
            if status != 206 {
                warn!(
                    "server doesn't support resume (status {}), restarting download",
                    status
                );
                return self.download_to_file(path, None);
            }
            if let Err(err) = check_partial_response(&resp, info) {
                warn!(%err, "partial response does not continue the file, restarting download");
                return self.download_to_file(path, None);
            }
        }

        let total = content_length(&resp).unwrap_or(0);
        let new_etag = resp
            .headers()
            .get(ETAG)
            .and_then(|h| h.to_str().ok())
            .map(String::from);
        trace!(total = total, etag = ?new_etag, "parsed response headers");

        let mut file = match open_output(path, resume_from) {
            Err(DownloadError::ResumeMismatch) => {
                warn!(path = %path.display(), "file shrank below its resume offset, restarting download");
                return self.download_to_file(path, None);
            }
            file => file?,
        };

        if let Some(ref cb) = self.on_progress {
            match resume_from {
                Some(current) => {
                    cb(Progress::Resuming {
                        current,
                        total,
                    })
                }
                None => {
                    cb(Progress::Starting {
                        total,
                    })
                }
            }
        }

        let mut reader = resp.into_body().into_reader();
        let mut buffer = [0u8; 8192];
        let mut downloaded = resume_from.unwrap_or(0);
        let mut last_checkpoint = downloaded / (1024 * 1024);

        loop {
            let n = reader.read(&mut buffer)?;
            if n == 0 {
                break;
            }

            file.write_all(&buffer[..n])?;
            downloaded += n as u64;

            let checkpoint = downloaded / (1024 * 1024);
            if checkpoint > last_checkpoint {
                last_checkpoint = checkpoint;
                trace!(downloaded = downloaded, "saving resume checkpoint");
                // don't fail on filesystems without xattr support
                if let Err(err) = write_resume(
                    path,
                    &ResumeInfo {
                        downloaded,
                        total,
                        etag: new_etag.clone(),
                        last_modified: None,
                    },
                ) {
                    trace!(%err, "failed to save resume checkpoint");
                }
            }

            if let Some(ref cb) = self.on_progress {
                cb(Progress::Chunk {
                    current: downloaded,
                    total,
                });
            }
        }

        debug!(downloaded = downloaded, "download transfer completed");

        if let Some(ref cb) = self.on_progress {
            cb(Progress::Complete {
                total,
            });
        }

        Ok(file)
    }
}

/// The total size of the resource a response carries, from the total of its
/// `Content-Range` or else its `Content-Length`, where either says.
pub(crate) fn content_length(resp: &Response<Body>) -> Option<u64> {
    resp.headers()
        .get(CONTENT_RANGE)
        .and_then(|h| h.to_str().ok())
        .and_then(|range| range.rsplit_once('/').and_then(|(_, tot)| tot.parse().ok()))
        .or_else(|| {
            resp.headers()
                .get(CONTENT_LENGTH)
                .and_then(|h| h.to_str().ok())
                .and_then(|len| len.parse::<u64>().ok())
        })
}

/// Checks that a partial response picks up where `info` left off: it starts at
/// the recorded offset and, where both are known, describes a resource of the
/// recorded size.
pub(crate) fn check_partial_response(
    resp: &Response<Body>,
    info: &ResumeInfo,
) -> Result<(), DownloadError> {
    let range = resp
        .headers()
        .get(CONTENT_RANGE)
        .and_then(|h| h.to_str().ok())
        .and_then(|range| range.trim().strip_prefix("bytes "))
        .ok_or(DownloadError::ResumeMismatch)?;
    let (span, total) = range.split_once('/').ok_or(DownloadError::ResumeMismatch)?;
    let start = span
        .split_once('-')
        .and_then(|(start, _)| start.parse::<u64>().ok());
    let total = total.parse::<u64>().ok();

    let same_size = match total {
        Some(total) => info.total == 0 || info.total == total,
        None => true,
    };
    if start == Some(info.downloaded) && same_size {
        Ok(())
    } else {
        Err(DownloadError::ResumeMismatch)
    }
}

/// Prompts the user to confirm overwriting the specified path.
///
/// Reads a line from stdin after printing "Overwrite <path>? [y/N] " and interprets
/// a case-insensitive `"y"` or `"yes"` as confirmation.
///
/// # Returns
///
/// `true` if the user entered `"y"` or `"yes"` (case-insensitive), `false` otherwise.
fn prompt_overwrite(path: &Path) -> std::io::Result<bool> {
    print!("Overwrite {}? [y/N] ", path.display());
    std::io::stdout().flush()?;

    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;

    Ok(matches!(line.trim().to_lowercase().as_str(), "y" | "yes"))
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn temp_with(contents: &[u8]) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(contents).unwrap();
        f.flush().unwrap();
        f
    }

    #[test]
    fn verify_checksum_ok_when_absent_or_matching() {
        let f = temp_with(b"hello soar");
        let expected = soar_utils::hash::calculate_checksum(f.path()).unwrap();

        let dl = Download::new("https://example.com/x");
        assert!(dl.verify_checksum(f.path()).is_ok());

        let dl = Download::new("https://example.com/x").checksum(expected.to_uppercase());
        assert!(dl.verify_checksum(f.path()).is_ok());
    }

    fn body() -> Vec<u8> {
        (0..20_000u32).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    fn skip_without_checksum_redownloads_a_partial_file() {
        let base = crate::test_server::serve(vec![("/file".into(), body())]);
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("file");
        std::fs::write(&out, &body()[..5_000]).unwrap();

        Download::new(format!("{base}/file"))
            .output(out.to_string_lossy())
            .overwrite(OverwriteMode::Skip)
            .execute()
            .unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), body());
    }

    #[test]
    fn skip_without_checksum_keeps_a_file_of_the_served_size() {
        let base = crate::test_server::serve(vec![("/file".into(), body())]);
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("file");
        let local = vec![7u8; body().len()];
        std::fs::write(&out, &local).unwrap();

        Download::new(format!("{base}/file"))
            .output(out.to_string_lossy())
            .overwrite(OverwriteMode::Skip)
            .execute()
            .unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), local);
    }

    #[test]
    fn resume_past_the_end_of_a_short_file_starts_over() {
        let base = crate::test_server::serve(vec![("/file".into(), body())]);
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("file");
        std::fs::write(&out, &body()[..100]).unwrap();
        let stale = ResumeInfo {
            downloaded: 10_000,
            total: body().len() as u64,
            etag: None,
            last_modified: None,
        };
        if write_resume(&out, &stale).is_err() {
            eprintln!("skipping: filesystem lacks user xattrs");
            return;
        }

        Download::new(format!("{base}/file"))
            .output(out.to_string_lossy())
            .overwrite(OverwriteMode::Skip)
            .execute()
            .unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), body());
    }

    #[test]
    fn resume_continues_from_the_recorded_offset() {
        let base = crate::test_server::serve(vec![("/file".into(), body())]);
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("file");
        std::fs::write(&out, &body()[..12_000]).unwrap();
        let info = ResumeInfo {
            downloaded: 8_000,
            total: body().len() as u64,
            etag: None,
            last_modified: None,
        };
        if write_resume(&out, &info).is_err() {
            eprintln!("skipping: filesystem lacks user xattrs");
            return;
        }

        let resumed = Arc::new(std::sync::Mutex::new(None));
        let seen = resumed.clone();
        Download::new(format!("{base}/file"))
            .output(out.to_string_lossy())
            .overwrite(OverwriteMode::Skip)
            .progress(move |p| {
                if let Progress::Resuming {
                    current, ..
                } = p
                {
                    *seen.lock().unwrap() = Some(current);
                }
            })
            .execute()
            .unwrap();
        assert_eq!(*resumed.lock().unwrap(), Some(8_000));
        assert_eq!(std::fs::read(&out).unwrap(), body());
    }

    #[test]
    fn a_symlink_planted_at_the_output_is_not_followed() {
        let base = crate::test_server::serve(vec![("/file".into(), body())]);
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        let out = dir.path().join("file");
        std::os::unix::fs::symlink(&target, &out).unwrap();

        let result = Download::new(format!("{base}/file"))
            .output(out.to_string_lossy())
            .overwrite(OverwriteMode::Skip)
            .execute();
        assert!(result.is_err());
        assert!(
            !target.exists(),
            "the download was written through the link"
        );
    }

    #[test]
    fn a_checksum_is_refused_on_stdout() {
        let result = Download::new("http://127.0.0.1:9/file")
            .output("-")
            .checksum("abc")
            .execute();
        assert!(matches!(result, Err(DownloadError::ChecksumOnStdout)));
    }

    #[test]
    fn verify_checksum_rejects_mismatch() {
        let f = temp_with(b"hello soar");
        let dl = Download::new("https://example.com/x").checksum("deadbeef");
        match dl.verify_checksum(f.path()) {
            Err(DownloadError::ChecksumMismatch {
                expected, ..
            }) => {
                assert_eq!(expected, "deadbeef");
            }
            other => panic!("expected ChecksumMismatch, got {other:?}"),
        }
    }

    #[test]
    fn preparing_is_reported_once() {
        let base = crate::test_server::serve(vec![("/file".into(), body())]);
        let dir = tempfile::tempdir().unwrap();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let record = seen.clone();

        Download::new(format!("{base}/file"))
            .output(dir.path().join("file").to_string_lossy())
            .progress(move |p| record.lock().unwrap().push(p))
            .execute()
            .unwrap();

        let seen = seen.lock().unwrap();
        let preparing = seen
            .iter()
            .filter(|p| matches!(p, Progress::Preparing))
            .count();
        assert_eq!(preparing, 1);
        assert!(matches!(seen[0], Progress::Preparing));
    }
}
