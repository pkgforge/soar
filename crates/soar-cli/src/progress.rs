use std::{
    collections::{HashMap, HashSet},
    sync::{mpsc::Receiver, Arc, LazyLock},
    time::Duration,
};

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use nu_ansi_term::Color::{Cyan, Green, Red};
use soar_dl::types::Progress;
use soar_events::{
    InstallStage, OperationId, RemoveStage, SoarEvent, SyncStage, UpdateCleanupStage, VerifyStage,
};

use crate::utils::{display_settings, progress_enabled};

/// Shared MultiProgress instance for suspend/stop from other modules.
static MULTI: LazyLock<Arc<MultiProgress>> = LazyLock::new(|| Arc::new(MultiProgress::new()));

/// Pause progress display, run the closure, then resume.
pub fn suspend<F: FnOnce()>(f: F) {
    MULTI.suspend(f);
}

/// Stop and clear all progress bars.
pub fn stop() {
    MULTI.clear().ok();
}

/// Handle returned by [`spawn_event_handler`] that owns the background progress thread.
///
/// Call [`finish`](ProgressGuard::finish) after dropping the [`SoarContext`] to join the
/// handler thread and clean up.
pub struct ProgressGuard {
    handle: Option<std::thread::JoinHandle<()>>,
}

impl ProgressGuard {
    /// Wait for the event handler thread to drain remaining events, then clean up.
    ///
    /// The [`SoarContext`] (which holds the channel sender) **must** be dropped before
    /// calling this, otherwise the thread will block forever waiting for more events.
    pub fn finish(mut self) {
        if let Some(handle) = self.handle.take() {
            handle.join().ok();
        }
    }
}

fn download_style() -> ProgressStyle {
    ProgressStyle::with_template(
        "{spinner:.cyan} {prefix}  {wide_bar:.cyan/dim}  {bytes}/{total_bytes}  {bytes_per_sec}  {eta}",
    )
    .unwrap()
    .progress_chars("━━─")
}

/// Format a colored prefix: pkg_name in cyan, #pkg_id in dim.
fn colored_prefix(pkg_name: &str, pkg_id: &str) -> String {
    format!(
        "{}{}",
        Cyan.paint(pkg_name),
        nu_ansi_term::Style::new()
            .dimmed()
            .paint(format!("#{pkg_id}"))
    )
}

fn spinner_style() -> ProgressStyle {
    ProgressStyle::with_template("{spinner:.cyan} {msg}").unwrap()
}

/// Create a download progress bar with a progress bar, bytes, and ETA.
pub fn create_download_job(prefix: &str) -> ProgressBar {
    let pb = if progress_enabled() {
        MULTI.add(ProgressBar::new(0))
    } else {
        MULTI.add(ProgressBar::hidden())
    };
    pb.set_style(download_style());
    pb.set_prefix(prefix.to_string());
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
}

/// Create a spinner job.
pub fn create_spinner_job(message: &str) -> ProgressBar {
    let pb = if progress_enabled() && display_settings().spinners() {
        MULTI.add(ProgressBar::new_spinner())
    } else {
        MULTI.add(ProgressBar::hidden())
    };
    pb.set_style(spinner_style());
    pb.set_message(message.to_string());
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
}

/// Handle download progress events and update a progress bar.
pub fn handle_download_progress(state: Progress, pb: &ProgressBar) {
    match state {
        Progress::Starting {
            total,
        } => {
            pb.set_length(total);
        }
        Progress::Resuming {
            current,
            total,
        } => {
            pb.set_length(total);
            pb.set_position(current);
        }
        Progress::Chunk {
            current, ..
        } => {
            pb.set_position(current);
        }
        Progress::Complete {
            ..
        } => {
            pb.finish_and_clear();
        }
        _ => {}
    }
}

/// Create a spinner-style progress bar for an operation.
fn create_op_spinner(msg: &str) -> ProgressBar {
    let pb = if progress_enabled() && display_settings().spinners() {
        MULTI.add(ProgressBar::new_spinner())
    } else {
        MULTI.add(ProgressBar::hidden())
    };
    pb.set_style(spinner_style());
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
}

/// Spawn a background thread that maps [`SoarEvent`]s to indicatif progress bars.
///
/// Each operation (`op_id`) gets a **single bar** for its entire lifecycle: it starts as a
/// download progress bar and is converted to a spinner for verification / install stages.
/// The bar is cleared on terminal events (`OperationComplete` / `OperationFailed`).
pub fn spawn_event_handler(receiver: Receiver<SoarEvent>) -> ProgressGuard {
    let handle = std::thread::spawn(move || {
        let mut jobs: HashMap<OperationId, ProgressBar> = HashMap::new();
        let mut sync_jobs: HashMap<String, ProgressBar> = HashMap::new();
        let mut batch_job: Option<ProgressBar> = None;
        let mut batch_msg: Option<String> = None;
        let mut remove_ops: HashSet<OperationId> = HashSet::new();

        // Ensure the batch progress job stays at the bottom of the job list
        // by removing and recreating it after new download jobs are added.
        macro_rules! reposition_batch {
            ($batch_job:expr, $batch_msg:expr) => {
                if let Some(old) = $batch_job.take() {
                    old.finish_and_clear();
                    if let Some(ref msg) = $batch_msg {
                        let new = MULTI.add(ProgressBar::new_spinner());
                        new.set_style(spinner_style());
                        new.set_message(msg.clone());
                        new.enable_steady_tick(Duration::from_millis(100));
                        $batch_job = Some(new);
                    }
                }
            };
        }

        while let Ok(event) = receiver.recv() {
            match event {
                // ── Download lifecycle ──────────────────────────────────
                SoarEvent::DownloadStarting {
                    op_id,
                    pkg_name,
                    pkg_id,
                    total,
                } => {
                    let pb = MULTI.add(ProgressBar::new(total));
                    pb.set_style(download_style());
                    pb.set_prefix(colored_prefix(&pkg_name, &pkg_id));
                    pb.enable_steady_tick(Duration::from_millis(100));
                    jobs.insert(op_id, pb);
                    reposition_batch!(batch_job, batch_msg);
                }
                SoarEvent::DownloadResuming {
                    op_id,
                    pkg_name,
                    pkg_id,
                    current,
                    total,
                } => {
                    let is_new = !jobs.contains_key(&op_id);
                    let pb = jobs.entry(op_id).or_insert_with(|| {
                        let pb = MULTI.add(ProgressBar::new(0));
                        pb.set_style(download_style());
                        pb.set_prefix(colored_prefix(&pkg_name, &pkg_id));
                        pb.enable_steady_tick(Duration::from_millis(100));
                        pb
                    });
                    pb.set_length(total);
                    pb.set_position(current);
                    if is_new {
                        reposition_batch!(batch_job, batch_msg);
                    }
                }
                SoarEvent::DownloadProgress {
                    op_id,
                    current,
                    ..
                } => {
                    if let Some(pb) = jobs.get(&op_id) {
                        pb.set_position(current);
                    }
                }
                SoarEvent::DownloadComplete {
                    op_id,
                    pkg_name,
                    pkg_id,
                    ..
                } => {
                    if let Some(pb) = jobs.get(&op_id) {
                        pb.set_style(spinner_style());
                        pb.set_message(format!("{pkg_name}#{pkg_id}: downloaded"));
                    }
                }
                SoarEvent::DownloadRetry {
                    op_id, ..
                } => {
                    if let Some(pb) = jobs.get(&op_id) {
                        pb.set_position(0);
                    }
                }
                SoarEvent::DownloadAborted {
                    op_id, ..
                } => {
                    if let Some(pb) = jobs.remove(&op_id) {
                        pb.finish_and_clear();
                    }
                }
                SoarEvent::DownloadRecovered {
                    op_id,
                    pkg_name,
                    pkg_id,
                } => {
                    let is_new = !jobs.contains_key(&op_id);
                    jobs.entry(op_id).or_insert_with(|| {
                        let pb = MULTI.add(ProgressBar::new(0));
                        pb.set_style(download_style());
                        pb.set_prefix(colored_prefix(&pkg_name, &pkg_id));
                        pb.enable_steady_tick(Duration::from_millis(100));
                        pb
                    });
                    if is_new {
                        reposition_batch!(batch_job, batch_msg);
                    }
                }

                // ── Verification ───────────────────────────────────────
                SoarEvent::Verifying {
                    op_id,
                    pkg_name,
                    pkg_id,
                    stage,
                } => {
                    match stage {
                        VerifyStage::Checksum | VerifyStage::Signature => {
                            let msg = match stage {
                                VerifyStage::Checksum => {
                                    format!("{pkg_name}#{pkg_id}: verifying checksum")
                                }
                                VerifyStage::Signature => {
                                    format!("{pkg_name}#{pkg_id}: verifying signature")
                                }
                                _ => unreachable!(),
                            };
                            let pb = jobs.entry(op_id).or_insert_with(|| create_op_spinner(&msg));
                            pb.set_style(spinner_style());
                            pb.set_message(msg);
                        }
                        VerifyStage::Passed => {}
                        VerifyStage::Failed(_) => {
                            if let Some(pb) = jobs.remove(&op_id) {
                                pb.finish_and_clear();
                            }
                        }
                    }
                }

                // ── Installation stages ────────────────────────────────
                SoarEvent::Installing {
                    op_id,
                    pkg_name,
                    pkg_id,
                    stage,
                } => {
                    if stage != InstallStage::Complete {
                        let msg = match &stage {
                            InstallStage::Extracting => {
                                format!("{pkg_name}#{pkg_id}: extracting")
                            }
                            InstallStage::ExtractingNested => {
                                format!("{pkg_name}#{pkg_id}: extracting nested")
                            }
                            InstallStage::LinkingBinaries => {
                                format!("{pkg_name}#{pkg_id}: linking binaries")
                            }
                            InstallStage::DesktopIntegration => {
                                format!("{pkg_name}#{pkg_id}: desktop integration")
                            }
                            InstallStage::SetupPortable => {
                                format!("{pkg_name}#{pkg_id}: setting up portable")
                            }
                            InstallStage::RecordingDatabase => {
                                format!("{pkg_name}#{pkg_id}: recording to db")
                            }
                            InstallStage::RunningHook(hook) => {
                                format!("{pkg_name}#{pkg_id}: running {hook}")
                            }
                            InstallStage::Complete => unreachable!(),
                        };
                        let pb = jobs.entry(op_id).or_insert_with(|| create_op_spinner(&msg));
                        pb.set_style(spinner_style());
                        pb.set_message(msg);
                    }
                }

                // ── Removal stages ─────────────────────────────────────
                SoarEvent::Removing {
                    op_id,
                    pkg_name,
                    pkg_id,
                    stage,
                } => {
                    remove_ops.insert(op_id);
                    if !matches!(stage, RemoveStage::Complete { .. }) {
                        let msg = match &stage {
                            RemoveStage::RunningHook(hook) => {
                                format!("{pkg_name}#{pkg_id}: running {hook}")
                            }
                            RemoveStage::UnlinkingBinaries => {
                                format!("{pkg_name}#{pkg_id}: unlinking binaries")
                            }
                            RemoveStage::UnlinkingDesktop => {
                                format!("{pkg_name}#{pkg_id}: unlinking desktop")
                            }
                            RemoveStage::UnlinkingIcons => {
                                format!("{pkg_name}#{pkg_id}: unlinking icons")
                            }
                            RemoveStage::RemovingDirectory => {
                                format!("{pkg_name}#{pkg_id}: removing files")
                            }
                            RemoveStage::CleaningDatabase => {
                                format!("{pkg_name}#{pkg_id}: cleaning db")
                            }
                            RemoveStage::Complete {
                                ..
                            } => unreachable!(),
                        };
                        let pb = jobs.entry(op_id).or_insert_with(|| create_op_spinner(&msg));
                        pb.set_message(msg);
                    }
                }

                // ── Update cleanup (separate op_ids, no OperationComplete) ─
                SoarEvent::UpdateCleanup {
                    op_id,
                    pkg_name,
                    pkg_id,
                    stage,
                    ..
                } => {
                    if matches!(
                        stage,
                        UpdateCleanupStage::Complete { .. } | UpdateCleanupStage::Kept
                    ) {
                        if let Some(pb) = jobs.remove(&op_id) {
                            pb.finish_and_clear();
                        }
                    } else {
                        let msg = format!("{pkg_name}#{pkg_id}: cleaning old version");
                        let pb = jobs.entry(op_id).or_insert_with(|| create_op_spinner(&msg));
                        pb.set_message(msg);
                    }
                }

                // ── Repository sync ────────────────────────────────────
                SoarEvent::SyncProgress {
                    repo_name,
                    stage,
                } => {
                    match stage {
                        SyncStage::Complete {
                            ..
                        }
                        | SyncStage::UpToDate => {
                            if let Some(pb) = sync_jobs.remove(&repo_name) {
                                pb.finish_and_clear();
                            }
                            let status = if matches!(stage, SyncStage::UpToDate) {
                                "up to date"
                            } else {
                                "synced"
                            };
                            MULTI.suspend(|| {
                                eprintln!(
                                    " {} {}: {}",
                                    Green.paint("✓"),
                                    Cyan.paint(&repo_name),
                                    nu_ansi_term::Style::new().dimmed().paint(status)
                                );
                            });
                        }
                        _ => {
                            let msg = match &stage {
                                SyncStage::Fetching => format!("{repo_name}: fetching metadata"),
                                SyncStage::Decompressing => format!("{repo_name}: decompressing"),
                                SyncStage::WritingDatabase => format!("{repo_name}: writing db"),
                                SyncStage::Validating => format!("{repo_name}: validating"),
                                _ => unreachable!(),
                            };
                            let pb = sync_jobs
                                .entry(repo_name)
                                .or_insert_with(|| create_op_spinner(&msg));
                            pb.set_message(msg);
                        }
                    }
                }

                // ── Batch progress (aggregated "Installing X/Y") ─────
                SoarEvent::BatchProgress {
                    completed,
                    total,
                    failed,
                } => {
                    let fail_msg = if failed > 0 {
                        format!(" ({failed} failed)")
                    } else {
                        String::new()
                    };
                    let msg = format!("Progress: {completed}/{total}{fail_msg}");
                    batch_msg = Some(msg.clone());
                    let pb = batch_job.get_or_insert_with(|| {
                        let pb = MULTI.add(ProgressBar::new_spinner());
                        pb.set_style(spinner_style());
                        pb.enable_steady_tick(Duration::from_millis(100));
                        pb
                    });
                    pb.set_message(msg);
                }

                // ── Terminal events ────────────────────────────────────
                SoarEvent::OperationComplete {
                    op_id,
                    pkg_name,
                    pkg_id,
                } => {
                    if !remove_ops.remove(&op_id) {
                        MULTI.suspend(|| {
                            eprintln!(
                                " {} {}#{}: {}",
                                Green.paint("✓"),
                                Cyan.paint(&pkg_name),
                                Cyan.paint(&pkg_id),
                                Green.paint("installed")
                            );
                        });
                    }
                    if let Some(pb) = jobs.remove(&op_id) {
                        pb.finish_and_clear();
                    }
                }
                SoarEvent::OperationFailed {
                    op_id,
                    pkg_name,
                    pkg_id,
                    error,
                } => {
                    remove_ops.remove(&op_id);
                    MULTI.suspend(|| {
                        eprintln!(
                            " {} {}#{}: {}",
                            Red.paint("✗"),
                            Cyan.paint(&pkg_name),
                            Cyan.paint(&pkg_id),
                            Red.paint(&error)
                        );
                    });
                    if let Some(pb) = jobs.remove(&op_id) {
                        pb.finish_and_clear();
                    }
                }

                _ => {}
            }
        }

        // Clean up remaining bars.
        if let Some(pb) = batch_job.take() {
            pb.finish_and_clear();
        }
        for (_, pb) in jobs {
            pb.finish_and_clear();
        }
        for (_, pb) in sync_jobs {
            pb.finish_and_clear();
        }
    });

    ProgressGuard {
        handle: Some(handle),
    }
}
