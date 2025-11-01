use std::{
    fs::{self, File, OpenOptions, Permissions},
    io::{Read as _, Write as _},
    os::unix::fs::PermissionsExt as _,
    path::{Path, PathBuf},
};

use soar_utils::fs::is_elf;
use ureq::{
    http::{
        header::{CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_RANGE, ETAG},
        Response,
    },
    Body,
};

use crate::{
    error::DownloadError,
    http::Http,
    types::{OverwriteMode, Progress, ResumeInfo},
    utils::{filename_from_header, filename_from_url, resolve_output_path},
    xattr::{read_resume, remove_resume, write_resume},
};

pub struct Download {
    pub url: String,
    pub output: Option<String>,
    pub overwrite: OverwriteMode,
    pub extract: bool,
    pub extract_to: Option<PathBuf>,
    pub on_progress: Option<Box<dyn Fn(Progress) + Send + Sync>>,
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
        }
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
    ///     });
    /// ```
    pub fn progress<F>(mut self, on_progress: F) -> Self
    where
        F: Fn(Progress) + Send + Sync + 'static,
    {
        self.on_progress = Some(Box::new(on_progress));
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
        if self.output.as_deref() == Some("-") {
            return self.download_to_stdout();
        }

        let resp = Http::head(&self.url)?;

        let header_filename = resp
            .headers()
            .get(CONTENT_DISPOSITION)
            .and_then(filename_from_header);
        let url_filename = filename_from_url(&self.url);

        let output_path =
            resolve_output_path(self.output.as_deref(), url_filename, header_filename)?;

        let resume_info = if output_path.is_file() {
            match self.overwrite {
                OverwriteMode::Skip => return Ok(output_path),
                OverwriteMode::Force => {
                    fs::remove_file(&output_path)?;
                    None
                }
                OverwriteMode::Prompt => {
                    if !prompt_overwrite(&output_path)? {
                        return Ok(output_path);
                    }
                    fs::remove_file(&output_path)?;
                    None
                }
            }
        } else {
            read_resume(&output_path)
        };

        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        self.download_to_file(&output_path, resume_info)?;

        if is_elf(&output_path) {
            std::fs::set_permissions(&output_path, Permissions::from_mode(0o755))?;
        }

        remove_resume(&output_path)?;

        if self.extract {
            let extract_dir = self.extract_to.unwrap_or_else(|| {
                output_path
                    .parent()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("."))
            });

            compak::extract_archive(&output_path, &extract_dir)?;
        }

        Ok(output_path)
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
        let resp = Http::fetch(&self.url, None, None)?;
        let mut stdout = std::io::stdout();
        let mut reader = resp.into_body().into_reader();

        std::io::copy(&mut reader, &mut stdout)?;
        stdout.flush()?;

        Ok(PathBuf::from("-"))
    }

    /// Download the HTTP response body into the given file path, using resume metadata when available and emitting progress events.
    ///
    /// When `resume_info` is provided the method attempts to resume the download from the recorded byte offset and validates server support
    /// for ranged requests; if the server does not return a partial-content (206) response, the download restarts from the beginning.
    /// Progress callbacks (if configured on `self`) are invoked with `Progress::Starting`, `Progress::Chunk`, and `Progress::Complete`.
    ///
    /// # Parameters
    ///
    /// - `path`: destination filesystem path to write the downloaded bytes to. If resuming, the file is opened for append; otherwise it is (re)created.
    /// - `resume_info`: optional resume metadata describing previously downloaded bytes and a prior `ETag`; when present the method will request a ranged response starting at `resume_info.downloaded`.
    ///
    /// # Returns
    ///
    /// `Ok(())` on successful completion of the download and file write, or `Err(DownloadError)` on IO, HTTP, or resume-state persistence failures.
    fn download_to_file(
        &self,
        path: &Path,
        resume_info: Option<ResumeInfo>,
    ) -> Result<(), DownloadError> {
        let (resume_from, etag) = resume_info
            .as_ref()
            .map(|r| (Some(r.downloaded), r.etag.as_deref()))
            .unwrap_or((None, None));

        let resp = Http::fetch(&self.url, resume_from, etag)?;

        let status = resp.status();
        if resume_from.is_some() && status != 206 {
            // Server doesn't support resume, start from the beginning
            return self.download_to_file(path, None);
        }

        let total = Self::parse_content_length(&resp);
        let new_etag = resp
            .headers()
            .get(ETAG)
            .and_then(|h| h.to_str().ok())
            .map(String::from);

        if let Some(ref cb) = self.on_progress {
            cb(Progress::Starting {
                total,
            });
        }

        let mut file = if resume_from.is_some() {
            OpenOptions::new().append(true).open(path)?
        } else {
            File::create(path)?
        };

        let mut reader = resp.into_body().into_reader();
        let mut buffer = [0u8; 8192];
        let mut downloaded = resume_from.unwrap_or(0);

        loop {
            let n = reader.read(&mut buffer)?;
            if n == 0 {
                break;
            }

            file.write_all(&buffer[..n])?;
            downloaded += n as u64;

            if downloaded % (1024 * 1024) == 0 {
                write_resume(
                    path,
                    &ResumeInfo {
                        downloaded,
                        total,
                        etag: new_etag.clone(),
                        last_modified: None,
                    },
                )?;
            }

            if let Some(ref cb) = self.on_progress {
                cb(Progress::Chunk {
                    current: downloaded,
                    total,
                });
            }
        }

        if let Some(ref cb) = self.on_progress {
            cb(Progress::Complete {
                total,
            });
        }

        Ok(())
    }

    /// Determine the total size of the response body from HTTP headers.
    ///
    /// Checks the `Content-Range` header first (parsing the value after the final '/'),
    /// and falls back to the `Content-Length` header. If neither header yields a valid
    /// size, returns 0.
    ///
    /// # Returns
    ///
    /// `u64` total size in bytes if present in the response headers, `0` otherwise.
    fn parse_content_length(resp: &Response<Body>) -> u64 {
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
            .unwrap_or(0)
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
