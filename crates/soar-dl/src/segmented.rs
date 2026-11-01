//! Segmented, multi-connection downloads.
//!
//! A file the server serves in byte ranges is cut into chunks smaller than a
//! connection's share and fetched in parallel, each written at its offset in
//! the output file. Where one connection is throttled well below the path's
//! capacity, several in parallel add up; where connections run at uneven
//! speeds, the queue keeps every worker fed so a fast one picks up a slow
//! one's share instead of the whole download waiting on it.

use std::{
    fs::File,
    io::Read as _,
    os::unix::fs::FileExt as _,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    thread,
};

use tracing::{debug, trace};

use crate::{
    error::DownloadError,
    http::Http,
    types::{Progress, ResumeInfo, SegmentProgress},
    xattr::write_resume,
};

/// The smallest chunk a download is cut into. Below this, another connection
/// adds handshake overhead without adding throughput.
pub(crate) const MIN_SEGMENT_SIZE: u64 = 4 * 1024 * 1024;

/// A file smaller than this is never segmented, whatever the connection count
/// allows.
pub(crate) const MIN_SEGMENTED_TOTAL: u64 = 2 * MIN_SEGMENT_SIZE;

/// Chunks queued per connection. More chunks than workers keeps a worker
/// whose chunk ran out from going idle while others still have work.
const CHUNKS_PER_CONNECTION: usize = 4;

/// Upper bound on chunks per download, keeping the resume state small.
const MAX_CHUNKS: usize = 128;

/// Bytes one worker reads from the network before writing them out.
const CHUNK_BUFFER: usize = 256 * 1024;

/// Aggregate bytes between two resume checkpoints and progress reports.
const REPORT_INTERVAL: u64 = 1024 * 1024;

/// Times one chunk may fail on the network before the download gives up and
/// leaves its resume state for the next run.
const MAX_ATTEMPTS: u32 = 3;

/// Cuts `total` bytes into contiguous, inclusive chunks: a queue of
/// [`CHUNKS_PER_CONNECTION`] per connection, capped by the per-chunk minimum
/// and [`MAX_CHUNKS`]. The last chunk absorbs the remainder.
pub(crate) fn plan(total: u64, connections: usize) -> Vec<(u64, u64)> {
    let max_chunks = (total / MIN_SEGMENT_SIZE).max(1) as usize;
    let parts = connections
        .max(1)
        .saturating_mul(CHUNKS_PER_CONNECTION)
        .min(max_chunks)
        .min(MAX_CHUNKS);
    let chunk = total / parts as u64;
    let mut ranges = Vec::with_capacity(parts);
    let mut start = 0;
    for i in 0..parts {
        let len = if i == parts - 1 { total - start } else { chunk };
        ranges.push((start, start + len - 1));
        start += len;
    }
    ranges
}

/// The chunk plan and progress a usable resume xattr carries.
pub(crate) struct ResumedSegments {
    /// Inclusive byte range of every chunk.
    pub ranges: Vec<(u64, u64)>,
    /// Bytes of each chunk already on disk.
    pub written: Vec<u64>,
    /// Bytes on disk across all chunks.
    pub current: u64,
}

/// Validates the chunk state a resume xattr carries against the resource it
/// describes. `None` when the state cannot describe this resource.
pub(crate) fn plan_from_resume(
    total: u64,
    segments: &[SegmentProgress],
) -> Option<ResumedSegments> {
    if segments.is_empty() {
        return None;
    }
    let mut resumed = ResumedSegments {
        ranges: Vec::with_capacity(segments.len()),
        written: Vec::with_capacity(segments.len()),
        current: 0,
    };
    // The chunks must tile the resource exactly: sorted, contiguous from
    // zero, through the last byte. Anything else would leave holes that no
    // request ever fills while the download still reports success.
    let mut expected_start = 0;
    for seg in segments {
        if seg.start != expected_start || seg.end < seg.start || seg.end >= total {
            return None;
        }
        if seg.written > seg.end - seg.start + 1 {
            return None;
        }
        resumed.ranges.push((seg.start, seg.end));
        resumed.written.push(seg.written);
        resumed.current += seg.written;
        expected_start = seg.end + 1;
    }
    if expected_start != total {
        return None;
    }
    Some(resumed)
}

/// One segmented transfer over an open, pre-allocated output file.
pub(crate) struct Job<'a> {
    pub url: &'a str,
    pub ghcr_blob: bool,
    pub etag: Option<&'a str>,
    pub path: &'a Path,
    pub file: &'a File,
    pub total: u64,
    /// How many workers to run; each pulls chunks from the shared queue.
    pub connections: usize,
    /// Inclusive byte range per chunk.
    pub ranges: Vec<(u64, u64)>,
    /// Bytes of each chunk already on disk, from a resume state.
    pub written: Vec<u64>,
    pub on_progress: Option<&'a Arc<dyn Fn(Progress) + Send + Sync>>,
}

enum Outcome {
    Done,
    Cancelled,
    Failed(DownloadError),
}

struct Shared {
    cancel: AtomicBool,
    /// Bytes written across all chunks.
    aggregate: AtomicU64,
    /// The aggregate the last checkpoint and progress report carried.
    reported: AtomicU64,
    /// Bytes written per chunk, counted from the chunk's start.
    chunk_written: Box<[AtomicU64]>,
    /// The full chunk plan, for resume checkpoints.
    ranges: Arc<Vec<(u64, u64)>>,
    /// The next chunk index a free worker may claim.
    cursor: AtomicUsize,
    /// Chunks an interrupted run left half-written. A worker finishes one of
    /// these before reaching for the queue, so the bytes already on disk for
    /// it are kept rather than refetched.
    partial: Mutex<Vec<usize>>,
    /// One slot per chunk: a worker claimed it and no other may.
    claimed: Mutex<Vec<bool>>,
}

impl Shared {
    fn chunk(&self, index: usize) -> (u64, u64, u64) {
        let (start, end) = self.ranges[index];
        (
            start,
            end,
            self.chunk_written[index].load(Ordering::Relaxed),
        )
    }

    /// Marks a chunk as this worker's, unless another worker already has it.
    fn claim(&self, index: usize) -> bool {
        let mut claimed = self.claimed.lock().unwrap();
        if claimed[index] {
            return false;
        }
        claimed[index] = true;
        true
    }
}

/// Runs the chunk workers to completion.
///
/// Workers share the output file, each writing only inside chunks it claimed
/// from the queue. One worker's failure cancels the rest; a server that
/// ignores a range request surfaces as [`DownloadError::RangeUnsupported`] so
/// the caller can fall back to a single stream.
pub(crate) fn run(job: Job) -> Result<(), DownloadError> {
    let initial: u64 = job.written.iter().sum();
    let ranges = Arc::new(job.ranges.clone());
    let partial = (0..job.ranges.len())
        .filter(|&i| {
            let (start, end) = job.ranges[i];
            let written = job.written[i];
            written > 0 && written < end - start + 1
        })
        .collect();
    let shared = Arc::new(Shared {
        cancel: AtomicBool::new(false),
        aggregate: AtomicU64::new(initial),
        reported: AtomicU64::new(initial),
        chunk_written: job
            .written
            .iter()
            .map(|w| AtomicU64::new(*w))
            .collect::<Vec<_>>()
            .into_boxed_slice(),
        ranges: ranges.clone(),
        cursor: AtomicUsize::new(0),
        partial: Mutex::new(partial),
        claimed: Mutex::new(vec![false; job.ranges.len()]),
    });

    if let Some(cb) = job.on_progress {
        if initial > 0 {
            cb(Progress::Resuming {
                current: initial,
                total: job.total,
            });
        } else {
            cb(Progress::Starting {
                total: job.total,
            });
        }
    }

    debug!(
        workers = job.connections.min(job.ranges.len()).max(1),
        chunks = job.ranges.len(),
        total = job.total,
        "segmented download starting"
    );

    let workers = job.connections.min(job.ranges.len()).max(1);
    let mut outcomes = Vec::with_capacity(workers);

    thread::scope(|scope| {
        let mut handles = Vec::with_capacity(workers);
        for _ in 0..workers {
            let worker = Worker {
                shared: shared.clone(),
                url: job.url,
                ghcr_blob: job.ghcr_blob,
                etag: job.etag,
                path: job.path,
                total: job.total,
                file: job.file.try_clone().expect("cloning the output file"),
                progress: job.on_progress.cloned(),
            };
            handles.push(scope.spawn(move || worker.run()));
        }

        for handle in handles {
            match handle.join() {
                Ok(outcome) => outcomes.push(outcome),
                Err(_) => {
                    outcomes.push(Outcome::Failed(DownloadError::Io(std::io::Error::other(
                        "chunk worker panicked",
                    ))))
                }
            }
        }
    });

    // A server that ignored one range request is treated as ignoring them all;
    // the caller falls back to a single stream, which needs no ranges.
    if let Some(Outcome::Failed(err)) = outcomes
        .iter()
        .find(|o| matches!(o, Outcome::Failed(DownloadError::RangeUnsupported { .. })))
    {
        let url = match err {
            DownloadError::RangeUnsupported {
                url,
            } => url.clone(),
            _ => unreachable!(),
        };
        return Err(DownloadError::RangeUnsupported {
            url,
        });
    }

    for outcome in outcomes {
        match outcome {
            Outcome::Done | Outcome::Cancelled => {}
            Outcome::Failed(err) => return Err(err),
        }
    }

    if let Some(cb) = job.on_progress {
        cb(Progress::Chunk {
            current: shared.aggregate.load(Ordering::Relaxed),
            total: job.total,
        });
    }

    Ok(())
}

struct Worker<'a> {
    shared: Arc<Shared>,
    url: &'a str,
    ghcr_blob: bool,
    etag: Option<&'a str>,
    path: &'a Path,
    total: u64,
    file: File,
    progress: Option<Arc<dyn Fn(Progress) + Send + Sync>>,
}

impl Worker<'_> {
    fn run(self) -> Outcome {
        let mut worker = self;
        loop {
            if worker.shared.cancel.load(Ordering::Relaxed) {
                return Outcome::Cancelled;
            }
            match worker.next_chunk() {
                None => return Outcome::Done,
                Some((index, start, end, written)) => {
                    if let Err(err) = worker.download_chunk(index, start, end, written) {
                        // Cancellation is fallout from another worker's
                        // failure; that worker's error is the one worth
                        // reporting.
                        if worker.shared.cancel.load(Ordering::Relaxed) {
                            return Outcome::Cancelled;
                        }
                        worker.shared.cancel.store(true, Ordering::Relaxed);
                        return Outcome::Failed(err);
                    }
                }
            }
        }
    }

    /// Claims this worker's next chunk: half-written ones first, then the
    /// queue, skipping chunks a previous run already completed or another
    /// worker owns. `None` when no work is left.
    fn next_chunk(&self) -> Option<(usize, u64, u64, u64)> {
        let mut partial = self.shared.partial.lock().unwrap();
        while let Some(index) = partial.pop() {
            let (start, end, written) = self.shared.chunk(index);
            if written < end - start + 1 && self.shared.claim(index) {
                return Some((index, start, end, written));
            }
        }
        drop(partial);
        loop {
            let index = self.shared.cursor.fetch_add(1, Ordering::Relaxed);
            if index >= self.shared.ranges.len() {
                return None;
            }
            let (start, end, written) = self.shared.chunk(index);
            if written == end - start + 1 {
                continue;
            }
            if self.shared.claim(index) {
                return Some((index, start, end, written));
            }
        }
    }

    fn download_chunk(
        &mut self,
        index: usize,
        start: u64,
        end: u64,
        written: u64,
    ) -> Result<(), DownloadError> {
        let mut attempts = 0;
        let mut cursor = start + written;
        loop {
            if cursor > end {
                return Ok(());
            }
            attempts += 1;
            match self.fetch_and_write(index, start, end, &mut cursor) {
                Ok(()) => continue,
                Err(err) => {
                    if self.shared.cancel.load(Ordering::Relaxed) {
                        return Err(err);
                    }
                    if err.is_transient() && attempts < MAX_ATTEMPTS {
                        debug!(chunk = index, attempt = attempts, %err, "chunk failed, retrying");
                        continue;
                    }
                    return Err(err);
                }
            }
        }
    }

    fn fetch_and_write(
        &mut self,
        index: usize,
        start: u64,
        end: u64,
        cursor: &mut u64,
    ) -> Result<(), DownloadError> {
        let resp = Http::fetch_range(self.url, *cursor, end, self.etag, self.ghcr_blob)?;

        if resp.status() != 206 {
            let status = resp.status().as_u16();
            if status == 200 {
                trace!(url = self.url, "server ignored the range request");
                return Err(DownloadError::RangeUnsupported {
                    url: self.url.to_string(),
                });
            }
            return Err(DownloadError::HttpError {
                status,
                url: self.url.to_string(),
            });
        }

        // A range that does not start where it was asked to start, or that
        // describes a different resource than the one being downloaded,
        // means the server's idea of the file has diverged from ours; refuse
        // it entirely rather than splice bytes out of place.
        let content_range = resp
            .headers()
            .get(ureq::http::header::CONTENT_RANGE)
            .and_then(|h| h.to_str().ok())
            .and_then(|range| range.trim().strip_prefix("bytes "))
            .and_then(|range| range.split_once('/'));
        let Some((span, total)) = content_range else {
            return Err(DownloadError::RangeUnsupported {
                url: self.url.to_string(),
            });
        };
        let matches = span
            .split_once('-')
            .and_then(|(start, _)| start.trim().parse::<u64>().ok())
            .is_some_and(|start| start == *cursor)
            && total.trim().parse::<u64>() == Ok(self.total);
        if !matches {
            return Err(DownloadError::RangeUnsupported {
                url: self.url.to_string(),
            });
        }

        let mut reader = resp.into_body().into_reader();
        let mut buffer = vec![0u8; CHUNK_BUFFER];
        loop {
            if *cursor > end {
                return Ok(());
            }
            // Never read past the chunk's last byte, whatever the server
            // sends; a body that overshoots its Content-Range must not leak
            // into the neighboring chunks.
            let want = (end + 1 - *cursor).min(buffer.len() as u64) as usize;
            let n = reader
                .read(&mut buffer[..want])
                .map_err(DownloadError::Io)?;
            if n == 0 {
                // The connection ended before this chunk's last byte.
                return Err(DownloadError::Io(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "chunk closed before its range was complete",
                )));
            }
            if self.shared.cancel.load(Ordering::Relaxed) {
                return Err(DownloadError::Io(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "cancelled",
                )));
            }
            self.file.write_all_at(&buffer[..n], *cursor)?;
            *cursor += n as u64;
            self.shared.chunk_written[index].store(*cursor - start, Ordering::Relaxed);
            let aggregate = self.shared.aggregate.fetch_add(n as u64, Ordering::Relaxed) + n as u64;
            self.maybe_report(aggregate);
        }
    }

    /// Checkpoints resume state and reports progress once per
    /// [`REPORT_INTERVAL`] aggregate bytes; one worker wins that right per
    /// interval, so the xattr and the bar advance in step.
    fn maybe_report(&self, aggregate: u64) {
        let reported = self.shared.reported.load(Ordering::Relaxed);
        if aggregate.saturating_sub(reported) < REPORT_INTERVAL {
            return;
        }
        if self
            .shared
            .reported
            .compare_exchange(reported, aggregate, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {
            return;
        }

        if let Some(cb) = &self.progress {
            cb(Progress::Chunk {
                current: aggregate,
                total: self.total,
            });
        }

        let segments = self
            .shared
            .ranges
            .iter()
            .enumerate()
            .map(|(index, &(start, end))| {
                SegmentProgress {
                    start,
                    end,
                    written: self.shared.chunk_written[index].load(Ordering::Relaxed),
                }
            })
            .collect();
        let info = ResumeInfo {
            downloaded: aggregate,
            total: self.total,
            etag: self.etag.map(String::from),
            last_modified: None,
            segments: Some(segments),
        };
        if let Err(err) = write_resume(self.path, &info) {
            trace!(%err, "failed to save resume checkpoint");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::{
        download::Download,
        test_server::{self, ServeOptions},
        types::OverwriteMode,
    };

    fn body(mib: usize) -> Vec<u8> {
        ((0..mib * 1024 * 1024).map(|i| (i % 251) as u8)).collect()
    }

    fn ranges_are_contiguous(ranges: &[(u64, u64)], total: u64) -> bool {
        let mut next = 0;
        for &(start, end) in ranges {
            if start != next || end < start {
                return false;
            }
            next = end + 1;
        }
        next == total
    }

    #[test]
    fn plan_queues_chunks_per_connection() {
        assert_eq!(plan(1024 * 1024 * 1024, 8).len(), 32);
        assert_eq!(plan(4 * 1024 * 1024 * 1024, 32).len(), 128);
        assert_eq!(plan(2 * 1024 * 1024 * 1024, 1).len(), 4);
    }

    #[test]
    fn plan_splits_evenly_and_covers_the_file() {
        let total = 100 * 1024 * 1024;
        let ranges = plan(total, 8);
        assert_eq!(ranges.len(), 25);
        assert!(ranges_are_contiguous(&ranges, total));
    }

    #[test]
    fn plan_respects_the_segment_minimum() {
        let total = 8 * 1024 * 1024;
        let ranges = plan(total, 8);
        assert_eq!(ranges.len(), 2);
        assert!(ranges_are_contiguous(&ranges, total));
    }

    #[test]
    fn plan_of_a_tiny_file_is_one_segment() {
        let ranges = plan(1024, 8);
        assert_eq!(ranges, vec![(0, 1023)]);
    }

    #[test]
    fn resume_state_that_cannot_describe_the_resource_is_refused() {
        let seg = |start, end, written| {
            SegmentProgress {
                start,
                end,
                written,
            }
        };
        let total = 100;

        assert!(plan_from_resume(total, &[]).is_none());
        assert!(plan_from_resume(total, &[seg(0, 100, 50)]).is_none());
        assert!(plan_from_resume(total, &[seg(0, 99, 101)]).is_none());
        assert!(plan_from_resume(total, &[seg(60, 10, 5)]).is_none());

        let resumed = plan_from_resume(total, &[seg(0, 49, 10), seg(50, 99, 50)]).unwrap();
        assert_eq!(resumed.ranges, vec![(0, 49), (50, 99)]);
        assert_eq!(resumed.written, vec![10, 50]);
        assert_eq!(resumed.current, 60);
    }

    #[test]
    fn a_large_download_is_fetched_in_segments() {
        let content = body(16);
        let base = test_server::serve(vec![("/file".into(), content.clone())]);
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("file");

        Download::new(format!("{base}/file"))
            .output(out.to_string_lossy())
            .overwrite(OverwriteMode::Force)
            .execute()
            .unwrap();

        assert_eq!(std::fs::read(&out).unwrap(), content);
        assert!(read_resume_segments(&out).is_none(), "resume state kept");
    }

    #[test]
    fn a_multi_chunk_download_reassembles() {
        let content = body(96);
        let (base, requests) = test_server::serve_with(
            vec![("/file".into(), content.clone())],
            ServeOptions::default(),
        );
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("file");

        Download::new(format!("{base}/file"))
            .output(out.to_string_lossy())
            .overwrite(OverwriteMode::Force)
            .connections(8)
            .execute()
            .unwrap();

        assert_eq!(std::fs::read(&out).unwrap(), content);
        let requests = requests.lock().unwrap();
        let gets = requests.iter().filter(|r| r.starts_with("GET")).count();
        assert_eq!(gets, 24, "96 MiB over 24 queued chunks: {requests:?}");
    }

    #[test]
    fn the_planned_number_of_connections_is_used() {
        let content = body(16);
        let (base, requests) = test_server::serve_with(
            vec![("/file".into(), content.clone())],
            ServeOptions::default(),
        );
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("file");

        Download::new(format!("{base}/file"))
            .output(out.to_string_lossy())
            .overwrite(OverwriteMode::Force)
            .connections(8)
            .execute()
            .unwrap();

        assert_eq!(std::fs::read(&out).unwrap(), content);
        let requests = requests.lock().unwrap();
        let gets: Vec<_> = requests.iter().filter(|r| r.starts_with("GET")).collect();
        assert_eq!(gets.len(), 4, "16 MiB over 4 MiB chunks: {requests:?}");
        let ranged = gets.iter().filter(|r| r.contains("Some")).count();
        assert_eq!(ranged, 4, "every chunk on its own range: {requests:?}");
    }

    #[test]
    fn a_server_without_ranges_downloads_in_one_stream() {
        let content = body(16);
        let (base, requests) = test_server::serve_with(
            vec![("/file".into(), content.clone())],
            ServeOptions {
                ignore_ranges: true,
                ..Default::default()
            },
        );
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("file");

        Download::new(format!("{base}/file"))
            .output(out.to_string_lossy())
            .overwrite(OverwriteMode::Force)
            .execute()
            .unwrap();

        assert_eq!(std::fs::read(&out).unwrap(), content);
        let requests = requests.lock().unwrap();
        let probed = |r: &String| r.contains("range=Some((0, Some(0)))");
        assert!(
            requests
                .iter()
                .filter(|r| r.starts_with("GET"))
                .all(|r| probed(r) || r.ends_with("range=None")),
            "only the one-byte probe asked for a range: {requests:?}"
        );
    }

    #[test]
    fn a_server_that_lies_about_ranges_falls_back_to_one_stream() {
        let content = body(16);
        let (base, requests) = test_server::serve_with(
            vec![("/file".into(), content.clone())],
            ServeOptions {
                lie_about_ranges: true,
                ..Default::default()
            },
        );
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("file");

        Download::new(format!("{base}/file"))
            .output(out.to_string_lossy())
            .overwrite(OverwriteMode::Force)
            .execute()
            .unwrap();

        assert_eq!(std::fs::read(&out).unwrap(), content);
        let requests = requests.lock().unwrap();
        assert!(
            requests
                .iter()
                .any(|r| r.starts_with("GET") && r.ends_with("range=None")),
            "the fallback fetched without a range: {requests:?}"
        );
    }

    #[test]
    fn a_server_that_refuses_head_still_downloads_in_segments() {
        let content = body(16);
        let (base, requests) = test_server::serve_with(
            vec![("/file".into(), content.clone())],
            ServeOptions {
                refuse_head: true,
                ..Default::default()
            },
        );
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("file");

        Download::new(format!("{base}/file"))
            .output(out.to_string_lossy())
            .overwrite(OverwriteMode::Force)
            .execute()
            .unwrap();

        assert_eq!(std::fs::read(&out).unwrap(), content);
        let requests = requests.lock().unwrap();
        let gets: Vec<_> = requests.iter().filter(|r| r.starts_with("GET")).collect();
        assert_eq!(gets.len(), 5, "the probe plus 4 chunks: {requests:?}");
    }

    #[test]
    fn a_server_that_refuses_head_and_ranges_downloads_in_one_stream() {
        let content = body(16);
        let (base, requests) = test_server::serve_with(
            vec![("/file".into(), content.clone())],
            ServeOptions {
                refuse_head: true,
                ignore_ranges: true,
                ..Default::default()
            },
        );
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("file");

        Download::new(format!("{base}/file"))
            .output(out.to_string_lossy())
            .overwrite(OverwriteMode::Force)
            .execute()
            .unwrap();

        assert_eq!(std::fs::read(&out).unwrap(), content);
        let requests = requests.lock().unwrap();
        let probed = |r: &String| r.contains("range=Some((0, Some(0)))");
        assert!(
            requests
                .iter()
                .filter(|r| r.starts_with("GET"))
                .all(|r| probed(r) || r.ends_with("range=None")),
            "only the one-byte probe asked for a range: {requests:?}"
        );
    }

    #[test]
    fn an_interrupted_segmented_download_resumes() {
        let content = body(16);
        let interrupted = test_server::serve_with(
            vec![("/file".into(), content.clone())],
            ServeOptions {
                close_after_bytes: Some(1024 * 1024),
                ..Default::default()
            },
        )
        .0;
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("file");
        let target = format!("{interrupted}/file");

        let first = Download::new(&target)
            .output(out.to_string_lossy())
            .overwrite(OverwriteMode::Skip)
            .execute();
        assert!(first.is_err(), "the interrupted transfer must fail");

        let segments = read_resume_segments(&out).expect("resume state kept after failure");
        assert_eq!(segments.len(), 4);
        assert!(
            segments.iter().any(|s| s.written > 0),
            "the interruption left progress behind: {segments:?}"
        );

        let healthy = test_server::serve(vec![("/file".into(), content.clone())]);
        let target = format!("{healthy}/file");
        Download::new(&target)
            .output(out.to_string_lossy())
            .overwrite(OverwriteMode::Skip)
            .execute()
            .unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), content);
    }

    #[test]
    fn a_mixed_state_resume_fetches_only_what_is_missing() {
        let content = body(32);
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("file");

        // Plant a half-finished 8-chunk download: two chunks complete, three
        // half written, three untouched.
        let ranges = plan(content.len() as u64, 8);
        assert_eq!(ranges.len(), 8);
        let file = std::fs::File::create(&out).unwrap();
        file.set_len(content.len() as u64).unwrap();
        let segments: Vec<SegmentProgress> = ranges
            .iter()
            .enumerate()
            .map(|(i, &(start, end))| {
                let len = end - start + 1;
                let written = match i {
                    0 | 1 => len,
                    2..=4 => len / 2,
                    _ => 0,
                };
                if written > 0 {
                    file.write_all_at(&content[start as usize..(start + written) as usize], start)
                        .unwrap();
                }
                SegmentProgress {
                    start,
                    end,
                    written,
                }
            })
            .collect();
        let current: u64 = segments.iter().map(|s| s.written).sum();
        crate::xattr::write_resume(
            &out,
            &ResumeInfo {
                downloaded: current,
                total: content.len() as u64,
                etag: None,
                last_modified: None,
                segments: Some(segments),
            },
        )
        .unwrap();
        drop(file);

        let (base, requests) = test_server::serve_with(
            vec![("/file".into(), content.clone())],
            ServeOptions::default(),
        );
        let seen = Arc::new(Mutex::new(Vec::new()));
        let record = seen.clone();
        Download::new(format!("{base}/file"))
            .output(out.to_string_lossy())
            .overwrite(OverwriteMode::Skip)
            .progress(move |p| record.lock().unwrap().push(p))
            .execute()
            .unwrap();

        assert_eq!(std::fs::read(&out).unwrap(), content);
        let requests = requests.lock().unwrap();
        let gets = requests.iter().filter(|r| r.starts_with("GET")).count();
        assert_eq!(
            gets, 6,
            "three partial and three untouched chunks: {requests:?}"
        );
        let seen = seen.lock().unwrap();
        assert!(
            seen.iter().any(
                |p| matches!(p, Progress::Resuming { current, .. } if *current == 14 * 1024 * 1024)
            ),
            "the resume point was not reported: {seen:?}"
        );
    }

    #[test]
    fn plan_tolerates_a_huge_connection_count() {
        assert_eq!(plan(1024, usize::MAX).len(), 1);
    }

    #[test]
    fn resume_state_with_gaps_or_disorder_is_refused() {
        let seg = |start, end, written| {
            SegmentProgress {
                start,
                end,
                written,
            }
        };
        let total = 100;

        // A gap no request would ever fill.
        assert!(plan_from_resume(total, &[seg(0, 49, 10), seg(60, 99, 0)]).is_none());
        // Out of order.
        assert!(plan_from_resume(total, &[seg(50, 99, 0), seg(0, 49, 10)]).is_none());
        // Coverage short of the resource.
        assert!(plan_from_resume(total, &[seg(0, 49, 10)]).is_none());
        // An exact tiling is still accepted.
        assert!(plan_from_resume(total, &[seg(0, 49, 10), seg(50, 99, 50)]).is_some());
    }

    #[test]
    fn a_chunk_interrupted_twice_completes_on_its_final_attempt() {
        let content = body(16);
        let (base, requests) = test_server::serve_with(
            vec![("/file".into(), content.clone())],
            ServeOptions {
                // Chunks are 4 MiB here; a GET starting within the first
                // 1 MiB of a chunk is cut, so every chunk's first two
                // attempts (offsets +0, +1 MiB) drop and only its third
                // (+2 MiB) carries it home, whatever the worker schedule.
                truncate_periodic: Some((4 * 1024 * 1024, 1024 * 1024)),
                ..Default::default()
            },
        );
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("file");

        let result = Download::new(format!("{base}/file"))
            .output(out.to_string_lossy())
            .overwrite(OverwriteMode::Force)
            .connections(8)
            .execute();
        if let Err(err) = &result {
            eprintln!("ERR {err} REQUESTS {:?}", requests.lock().unwrap());
        }
        result.unwrap();

        assert_eq!(std::fs::read(&out).unwrap(), content);
        let requests = requests.lock().unwrap();
        let gets = requests.iter().filter(|r| r.starts_with("GET")).count();
        assert_eq!(
            gets, 12,
            "every chunk interrupted twice, whole on its third attempt: {requests:?}"
        );
    }

    #[test]
    fn an_oversending_server_yields_the_exact_resource() {
        let content = body(16);
        let (base, _requests) = test_server::serve_with(
            vec![("/file".into(), content.clone())],
            ServeOptions {
                oversend: Some(8 * 1024 * 1024),
                ..Default::default()
            },
        );
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("file");

        Download::new(format!("{base}/file"))
            .output(out.to_string_lossy())
            .overwrite(OverwriteMode::Force)
            .execute()
            .unwrap();

        let downloaded = std::fs::read(&out).unwrap();
        assert_eq!(
            downloaded.len(),
            content.len(),
            "no overshoot bytes on disk"
        );
        assert_eq!(downloaded, content);
    }

    #[test]
    fn a_resume_state_from_a_changed_resource_is_discarded() {
        let stale = body(16);
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("file");

        // Plant a half-finished 4-chunk download of a 16 MiB resource.
        let ranges = plan(stale.len() as u64, 8);
        assert_eq!(ranges.len(), 4);
        let plant = || {
            let file = std::fs::File::create(&out).unwrap();
            file.set_len(stale.len() as u64).unwrap();
            file.write_all_at(&stale[..ranges[0].1 as usize + 1], 0)
                .unwrap();
            let segments: Vec<SegmentProgress> = ranges
                .iter()
                .enumerate()
                .map(|(i, &(start, end))| {
                    SegmentProgress {
                        start,
                        end,
                        written: if i == 0 { end - start + 1 } else { 0 },
                    }
                })
                .collect();
            crate::xattr::write_resume(
                &out,
                &ResumeInfo {
                    downloaded: segments[0].written,
                    total: stale.len() as u64,
                    etag: None,
                    last_modified: None,
                    segments: Some(segments),
                },
            )
            .unwrap();
            drop(file);
        };

        // The resource grew since the state was written.
        let grew: Vec<u8> = body(32).into_iter().map(|b| !b).collect();
        plant();
        let base = test_server::serve(vec![("/file".into(), grew.clone())]);
        Download::new(format!("{base}/file"))
            .output(out.to_string_lossy())
            .overwrite(OverwriteMode::Skip)
            .execute()
            .unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), grew);

        // The resource shrank since the state was written.
        let shrank: Vec<u8> = body(8).into_iter().map(|b| !b).collect();
        plant();
        let base = test_server::serve(vec![("/file".into(), shrank.clone())]);
        Download::new(format!("{base}/file"))
            .output(out.to_string_lossy())
            .overwrite(OverwriteMode::Skip)
            .execute()
            .unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), shrank);
    }

    fn read_resume_segments(path: &std::path::Path) -> Option<Vec<SegmentProgress>> {
        crate::xattr::read_resume(path).and_then(|info| info.segments)
    }
}
