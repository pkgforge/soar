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

/// Reject a zsync endpoint that is not served over TLS.
///
/// A control file names both where the artifact lives and the checksums it is
/// then verified against, so whoever can rewrite one in flight decides what
/// gets installed. Unlike repository metadata there is no signature to fall
/// back on, which leaves https as the only thing authenticating an update.
fn ensure_https(url: &str, what: &str) -> Result<(), DownloadError> {
    if url
        .trim_start()
        .to_ascii_lowercase()
        .starts_with("https://")
    {
        return Ok(());
    }
    Err(DownloadError::Zsync(format!(
        "{what} must be served over https: {url}"
    )))
}

/// Reject a control file that points its artifact at a cleartext URL.
///
/// A relative entry is resolved against the control file's own location, which
/// is checked before the file is fetched, so only an absolute entry can escape
/// to another scheme.
fn ensure_artifact_urls_https(urls: &[String]) -> Result<(), DownloadError> {
    urls.iter()
        .filter(|url| url.contains("://"))
        .try_for_each(|url| ensure_https(url, "zsync artifact"))
}

/// The directory part of a URL, as zsync resolves relative artifact entries
/// against it.
fn base_url_of(control_url: &str) -> &str {
    control_url
        .rfind('/')
        .map(|i| &control_url[..=i])
        .unwrap_or("")
}

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
        let candidate = match self.urls.first() {
            Some(url) if url.contains("://") => url.clone(),
            Some(url) => format!("{base}/{url}"),
            None => control_url.strip_suffix(".zsync")?.to_string(),
        };
        // What comes back from here is downloaded and installed, so it answers
        // to the same rule as the control file that named it.
        ensure_https(&candidate, "zsync artifact").ok()?;
        Some(candidate)
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
    // A feed that cannot be authenticated is worse than no feed: without one
    // the caller downloads the release in full over its own https URL.
    ensure_https(&feed, "zsync control file").ok()?;
    crate::http::Http::head(&feed).ok().map(|_| feed)
}

/// Read the control file at `url` without downloading the artifact.
pub fn fetch_target(url: &str) -> Result<ZsyncTarget, DownloadError> {
    ensure_https(url, "zsync control file")?;
    let http = HttpClient::new();
    let control = http
        .fetch_control_file(url)
        .map_err(|e| DownloadError::Zsync(format!("fetching zsync control file: {e}")))?;
    ensure_artifact_urls_https(&control.urls)?;
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
/// than producing a broken package. Those checksums are only as trustworthy as
/// the control file carrying them, which is why the feed and the artifact both
/// have to be served over https; a caller holding a checksum of its own should
/// still check the result against it.
pub fn download<F>(
    url: &str,
    seed: &Path,
    output: &Path,
    on_progress: Option<F>,
) -> Result<(), DownloadError>
where
    F: Fn(Progress) + Send + Sync + 'static,
{
    ensure_https(url, "zsync control file")?;
    assemble(url, seed, output, on_progress)
}

/// The transfer itself, with the scheme already settled by the caller.
///
/// Split out so the assembly can be exercised against a local server, which
/// [`download`] would refuse for want of TLS.
fn assemble<F>(
    url: &str,
    seed: &Path,
    output: &Path,
    on_progress: Option<F>,
) -> Result<(), DownloadError>
where
    F: Fn(Progress) + Send + Sync + 'static,
{
    if let Some(ref callback) = on_progress {
        callback(Progress::Preparing);
    }

    // Fetched here rather than left to `ZsyncAssembly::from_url` so the control
    // file that is checked is the same one the blocks are fetched against: a
    // second request could be answered differently.
    let control = HttpClient::new()
        .fetch_control_file(url)
        .map_err(|e| DownloadError::Zsync(format!("reading zsync control file: {e}")))?;
    ensure_artifact_urls_https(&control.urls)?;

    let mut assembly = ZsyncAssembly::with_base_url(control, output, Some(base_url_of(url)))
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

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::{BufRead as _, BufReader, Read as _, Seek as _, SeekFrom, Write as _},
        net::{TcpListener, TcpStream},
        thread,
    };

    use super::*;

    fn target(urls: &[&str]) -> ZsyncTarget {
        ZsyncTarget {
            sha1: None,
            length: 0,
            filename: None,
            mtime: None,
            urls: urls.iter().map(|u| u.to_string()).collect(),
        }
    }

    #[test]
    fn ensure_https_accepts_only_tls() {
        assert!(ensure_https("https://e.test/a.zsync", "feed").is_ok());
        assert!(ensure_https("HTTPS://e.test/a.zsync", "feed").is_ok());
        assert!(ensure_https("http://e.test/a.zsync", "feed").is_err());
        assert!(ensure_https("ftp://e.test/a.zsync", "feed").is_err());
        assert!(ensure_https("//e.test/a.zsync", "feed").is_err());
    }

    #[test]
    fn artifact_urls_are_checked_only_when_absolute() {
        // Relative entries resolve against the control file's own location.
        assert!(ensure_artifact_urls_https(&["App.AppImage".to_string()]).is_ok());
        assert!(ensure_artifact_urls_https(&["https://e.test/App".to_string()]).is_ok());
        assert!(ensure_artifact_urls_https(&["http://e.test/App".to_string()]).is_err());
    }

    #[test]
    fn a_cleartext_mirror_anywhere_in_the_list_is_rejected() {
        let urls = vec![
            "https://e.test/App".to_string(),
            "http://mirror.test/App".to_string(),
        ];
        assert!(ensure_artifact_urls_https(&urls).is_err());
    }

    #[test]
    fn artifact_url_resolves_relative_against_the_feed() {
        let t = target(&["App-x86_64.AppImage"]);
        assert_eq!(
            t.artifact_url("https://e.test/rel/App.AppImage.zsync"),
            Some("https://e.test/rel/App-x86_64.AppImage".to_string())
        );
    }

    #[test]
    fn artifact_url_keeps_an_absolute_https_entry() {
        let t = target(&["https://cdn.test/App.AppImage"]);
        assert_eq!(
            t.artifact_url("https://e.test/App.AppImage.zsync"),
            Some("https://cdn.test/App.AppImage".to_string())
        );
    }

    #[test]
    fn artifact_url_refuses_to_redirect_to_cleartext() {
        let t = target(&["http://cdn.test/App.AppImage"]);
        assert_eq!(t.artifact_url("https://e.test/App.AppImage.zsync"), None);
    }

    #[test]
    fn artifact_url_falls_back_to_the_feed_without_its_suffix() {
        let t = target(&[]);
        assert_eq!(
            t.artifact_url("https://e.test/App.AppImage.zsync"),
            Some("https://e.test/App.AppImage".to_string())
        );
        // The fallback inherits the feed's scheme, so a cleartext feed cannot
        // smuggle a cleartext artifact through it.
        assert_eq!(t.artifact_url("http://e.test/App.AppImage.zsync"), None);
    }

    #[test]
    fn base_url_keeps_the_trailing_separator() {
        assert_eq!(
            base_url_of("https://e.test/rel/App.zsync"),
            "https://e.test/rel/"
        );
        assert_eq!(base_url_of("no-separator"), "");
    }

    #[test]
    fn feed_beside_declines_a_cleartext_artifact() {
        // Rejected on scheme before any request is made.
        assert_eq!(feed_beside("http://e.test/App.AppImage"), None);
    }

    #[test]
    fn fetch_target_rejects_a_cleartext_feed_before_fetching() {
        // The scheme is refused up front, so this never resolves the host: a
        // network attempt would report DNS rather than the rule below.
        let err = fetch_target("http://evil.test/App.AppImage.zsync").unwrap_err();
        assert!(
            matches!(&err, DownloadError::Zsync(m) if m.contains("https")),
            "expected a scheme error, got {err:?}"
        );
    }

    #[test]
    fn download_rejects_a_cleartext_feed_before_fetching() {
        let out = std::path::PathBuf::from("/nonexistent/soar-zsync-test");
        let err = download(
            "http://e.test/App.AppImage.zsync",
            Path::new("/nonexistent/seed"),
            &out,
            None::<fn(Progress)>,
        )
        .unwrap_err();
        assert!(
            matches!(&err, DownloadError::Zsync(m) if m.contains("https")),
            "expected a scheme error, got {err:?}"
        );
        assert!(!out.exists());
    }

    // What follows drives a real transfer against a local server: a control
    // file published beside its artifact, a relative `URL:` entry resolved
    // against the feed's own location, and blocks fetched by range. The scheme
    // rule is covered above; this covers the transfer that rule guards.

    /// Bytes that repeat rarely, so blocks stay distinguishable.
    fn artifact_bytes(len: usize, seed: u64) -> Vec<u8> {
        let mut state = seed;
        (0..len)
            .map(|_| {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                (state >> 33) as u8
            })
            .collect()
    }

    /// Serve `dir` with the range support zsync needs. Returns the bound port.
    fn serve(dir: &Path) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let root = dir.to_path_buf();

        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let root = root.clone();
                thread::spawn(move || serve_one(stream, &root));
            }
        });

        port
    }

    fn serve_one(mut stream: TcpStream, root: &Path) -> std::io::Result<()> {
        let mut reader = BufReader::new(stream.try_clone()?);

        let mut request_line = String::new();
        reader.read_line(&mut request_line)?;
        let Some(target) = request_line.split_whitespace().nth(1) else {
            return Ok(());
        };
        let target = target.trim_start_matches('/').to_string();

        let mut range = None;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line)? == 0 || line.trim().is_empty() {
                break;
            }
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("range:") {
                if let Some((start, end)) =
                    value.trim().trim_start_matches("bytes=").split_once('-')
                {
                    range = Some((
                        start.trim().parse::<u64>().unwrap_or(0),
                        end.trim().parse::<u64>().ok(),
                    ));
                }
            }
        }

        let path = root.join(&target);
        if !path.starts_with(root) || !path.is_file() {
            stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")?;
            return Ok(());
        }

        let total = fs::metadata(&path)?.len();
        let mut file = fs::File::open(&path)?;

        match range {
            Some((start, end)) => {
                let end = end.unwrap_or(total - 1).min(total - 1);
                let len = end.saturating_sub(start) + 1;
                file.seek(SeekFrom::Start(start))?;
                let mut body = vec![0u8; len as usize];
                file.read_exact(&mut body)?;
                stream.write_all(
                    format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{end}/{total}\r\nContent-Length: {len}\r\nAccept-Ranges: bytes\r\n\r\n"
                    )
                    .as_bytes(),
                )?;
                stream.write_all(&body)?;
            }
            None => {
                let mut body = Vec::new();
                file.read_to_end(&mut body)?;
                stream.write_all(
                    format!("HTTP/1.1 200 OK\r\nContent-Length: {total}\r\nAccept-Ranges: bytes\r\n\r\n")
                        .as_bytes(),
                )?;
                stream.write_all(&body)?;
            }
        }
        stream.flush()
    }

    /// Publish an artifact and a control file naming it by a relative URL, the
    /// way a release does.
    fn publish(dir: &Path, name: &str, bytes: &[u8]) {
        let artifact = dir.join(name);
        fs::write(&artifact, bytes).unwrap();

        let mut source = fs::File::open(&artifact).unwrap();
        let control = ControlFile::generate(&mut source, name, name, Some(2048)).unwrap();

        let mut out = fs::File::create(dir.join(format!("{name}.zsync"))).unwrap();
        control.write(&mut out).unwrap();
    }

    #[test]
    fn rebuilds_an_artifact_from_a_relative_feed() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = artifact_bytes(512 * 1024, 42);
        publish(dir.path(), "App-x86_64.AppImage", &bytes);

        let port = serve(dir.path());
        let feed = format!("http://127.0.0.1:{port}/App-x86_64.AppImage.zsync");

        // No seed, so every block comes off the wire: this is what proves the
        // relative entry resolved against the right base.
        let out = dir.path().join("rebuilt.AppImage");
        assemble(&feed, Path::new("/nonexistent"), &out, None::<fn(Progress)>).unwrap();

        assert_eq!(fs::read(&out).unwrap(), bytes, "rebuilt artifact differs");
    }

    #[test]
    fn reuses_the_installed_copy_and_fetches_only_the_difference() {
        let dir = tempfile::tempdir().unwrap();

        let mut new = artifact_bytes(512 * 1024, 7);
        let seed = dir.path().join("installed.AppImage");
        fs::write(&seed, &new).unwrap();

        // A new release differing only at the front.
        new[..4096].copy_from_slice(&artifact_bytes(4096, 99));
        publish(dir.path(), "App-x86_64.AppImage", &new);

        let port = serve(dir.path());
        let feed = format!("http://127.0.0.1:{port}/App-x86_64.AppImage.zsync");

        let out = dir.path().join("rebuilt.AppImage");
        assemble(&feed, &seed, &out, None::<fn(Progress)>).unwrap();

        assert_eq!(fs::read(&out).unwrap(), new, "rebuilt artifact differs");
    }

    #[test]
    fn a_substituted_artifact_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = artifact_bytes(128 * 1024, 3);
        publish(dir.path(), "App-x86_64.AppImage", &bytes);

        // Publish the control file, then swap the artifact underneath it.
        fs::write(
            dir.path().join("App-x86_64.AppImage"),
            artifact_bytes(128 * 1024, 4),
        )
        .unwrap();

        let port = serve(dir.path());
        let feed = format!("http://127.0.0.1:{port}/App-x86_64.AppImage.zsync");

        let out = dir.path().join("rebuilt.AppImage");
        let result = assemble(&feed, Path::new("/nonexistent"), &out, None::<fn(Progress)>);
        assert!(
            result.is_err(),
            "a substituted artifact must not be accepted"
        );
    }
}
