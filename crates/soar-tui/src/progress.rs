//! Live per-package progress, folded from the events operations emit.
//!
//! The tracker is pure state with no I/O: [`Tracker::apply`] folds events in
//! against a timestamp, and rendering reads rows out. The activity strip and
//! the job detail list the rows once they render.

#![allow(dead_code)]

use std::time::{Duration, Instant};

use soar_events::{
    BuildStage, HookStage, InstallStage, OperationId, RemoveStage, RunStage, SoarEvent, SyncStage,
    UpdateCleanupStage, VerifyStage,
};

/// Where one operation is in its work.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    /// Known but untouched: its first event carried no stage of its own.
    Waiting,
    /// Bytes are moving, with how many arrived and how many to expect.
    Downloading {
        /// Bytes received so far.
        current: u64,
        /// Bytes to receive in total.
        total: u64,
    },
    /// Between downloads, showing what the operation is doing now.
    Step(String),
    /// Finished cleanly.
    Done,
    /// Stopped by an error, carrying it for display.
    Failed(String),
    /// Passed over without running, such as by an abort.
    Skipped,
}

/// One operation's progress inside a job, shown as one line.
#[derive(Debug)]
pub(crate) struct OpRow {
    /// The operation the row belongs to.
    pub(crate) op_id: OperationId,
    /// The package the operation works on.
    pub(crate) name: String,
    /// Where the operation is in its work.
    pub(crate) phase: Phase,
    /// The smoothed download speed in bytes per second, while downloading.
    pub(crate) speed: Option<f64>,
    /// The last line a hook or build command printed.
    pub(crate) last_output: Option<String>,
    /// The previous byte count and when it was seen, for the speed.
    last_sample: Option<(Instant, u64)>,
}

impl OpRow {
    /// Folds a new byte count into the smoothed download speed. Samples
    /// less than 200 ms apart are too noisy to divide.
    fn sample(&mut self, current: u64, now: Instant) {
        let Some((at, bytes)) = self.last_sample else {
            self.last_sample = Some((now, current));
            return;
        };
        let dt = now.duration_since(at);
        if dt < Duration::from_millis(200) || current < bytes {
            return;
        }
        let inst = (current - bytes) as f64 / dt.as_secs_f64();
        self.speed = Some(match self.speed {
            Some(smoothed) => 0.3 * inst + 0.7 * smoothed,
            None => inst,
        });
        self.last_sample = Some((now, current));
    }

    /// The time left until the download finishes, when the size is known
    /// and the smoothed speed is high enough to divide by.
    pub(crate) fn eta(&self) -> Option<Duration> {
        let Phase::Downloading {
            current,
            total,
        } = self.phase
        else {
            return None;
        };
        let speed = self.speed?;
        if total == 0 || speed <= 1.0 {
            return None;
        }
        Some(Duration::from_secs_f64(
            total.saturating_sub(current) as f64 / speed,
        ))
    }
}

/// Every operation row of one job or of background work, plus the overall
/// batch and sync progress.
#[derive(Debug, Default)]
pub(crate) struct Tracker {
    /// One row per operation, in the order their first event arrived.
    pub(crate) ops: Vec<OpRow>,
    /// The batch counts as completed, total, and failed.
    pub(crate) batch: Option<(u32, u32, u32)>,
    /// Each repository's sync stage, in first-seen order.
    pub(crate) sync: Vec<(String, String)>,
}

impl Tracker {
    /// Folds one operation event in. A new operation appends a row at the
    /// end; a known one updates in place, so rows never jump around.
    pub(crate) fn apply(&mut self, event: &SoarEvent, now: Instant) {
        match event {
            SoarEvent::DownloadPreparing {
                op_id,
                pkg_name,
            } => {
                self.row(*op_id, pkg_name).phase = Phase::Step("connecting".into());
            }
            SoarEvent::DownloadStarting {
                op_id,
                pkg_name,
                total,
            } => {
                self.row(*op_id, pkg_name).phase = Phase::Downloading {
                    current: 0,
                    total: *total,
                };
            }
            SoarEvent::DownloadResuming {
                op_id,
                pkg_name,
                current,
                total,
            } => {
                self.row(*op_id, pkg_name).phase = Phase::Downloading {
                    current: *current,
                    total: *total,
                };
            }
            SoarEvent::DownloadProgress {
                op_id,
                pkg_name,
                current,
                total,
            } => {
                let row = self.row(*op_id, pkg_name);
                row.phase = Phase::Downloading {
                    current: *current,
                    total: *total,
                };
                row.sample(*current, now);
            }
            SoarEvent::DownloadComplete {
                op_id,
                pkg_name,
                ..
            } => {
                self.row(*op_id, pkg_name).phase = Phase::Step("downloaded".into());
            }
            SoarEvent::DownloadRetry {
                op_id,
                pkg_name,
            } => {
                self.row(*op_id, pkg_name).phase = Phase::Step("retrying".into());
            }
            SoarEvent::DownloadAborted {
                op_id,
                pkg_name,
            } => {
                self.row(*op_id, pkg_name).phase = Phase::Failed("download aborted".into());
            }
            SoarEvent::DownloadRecovered {
                op_id,
                pkg_name,
            } => {
                self.row(*op_id, pkg_name);
            }
            SoarEvent::Verifying {
                op_id,
                pkg_name,
                stage,
            } => {
                self.row(*op_id, pkg_name).phase = match stage {
                    VerifyStage::Checksum => Phase::Step("verifying checksum".into()),
                    VerifyStage::Signature => Phase::Step("verifying signature".into()),
                    VerifyStage::Passed => Phase::Step("verified".into()),
                    VerifyStage::Failed(error) => Phase::Failed(error.clone()),
                };
            }
            SoarEvent::Installing {
                op_id,
                pkg_name,
                stage,
            } => {
                self.row(*op_id, pkg_name).phase = match stage {
                    InstallStage::Extracting | InstallStage::ExtractingNested => {
                        Phase::Step("extracting".into())
                    }
                    InstallStage::LinkingBinaries => Phase::Step("linking binaries".into()),
                    InstallStage::DesktopIntegration => Phase::Step("desktop integration".into()),
                    InstallStage::FetchingSet(set) => Phase::Step(format!("fetching {set}")),
                    InstallStage::SetupPortable => Phase::Step("portable directories".into()),
                    InstallStage::RecordingDatabase => Phase::Step("recording".into()),
                    InstallStage::RunningHook(hook) => Phase::Step(format!("running {hook}")),
                    InstallStage::Complete => Phase::Done,
                };
            }
            SoarEvent::Removing {
                op_id,
                pkg_name,
                stage,
            } => {
                self.row(*op_id, pkg_name).phase = match stage {
                    RemoveStage::RunningHook(hook) => Phase::Step(format!("running {hook}")),
                    RemoveStage::UnlinkingBinaries
                    | RemoveStage::UnlinkingDesktop
                    | RemoveStage::UnlinkingIcons => Phase::Step("unlinking".into()),
                    RemoveStage::RemovingDirectory => Phase::Step("deleting files".into()),
                    RemoveStage::CleaningDatabase => Phase::Step("recording".into()),
                    RemoveStage::Complete {
                        ..
                    } => Phase::Done,
                };
            }
            SoarEvent::UpdateCleanup {
                op_id,
                pkg_name,
                old_version,
                stage,
            } => {
                let row = self.row(*op_id, pkg_name);
                if let UpdateCleanupStage::Removing = stage {
                    row.phase = Phase::Step(format!("removing {old_version}"));
                }
            }
            SoarEvent::Hook {
                op_id,
                pkg_name,
                hook_name,
                stage,
            } => {
                let row = self.row(*op_id, pkg_name);
                match stage {
                    HookStage::Starting => {
                        row.phase = Phase::Step(format!("running {hook_name}"));
                    }
                    HookStage::Failed {
                        exit_code,
                    } => {
                        row.phase = Phase::Step(match exit_code {
                            Some(code) => format!("{hook_name} failed ({code})"),
                            None => format!("{hook_name} failed"),
                        });
                    }
                    HookStage::Complete => {}
                }
            }
            SoarEvent::Building {
                op_id,
                pkg_name,
                stage,
            } => {
                let row = self.row(*op_id, pkg_name);
                match stage {
                    BuildStage::Sandboxing => {
                        row.phase = Phase::Step("preparing sandbox".into());
                    }
                    BuildStage::Running {
                        command_index,
                        total_commands,
                    } => {
                        row.phase = Phase::Step(format!(
                            "building {}/{}",
                            command_index + 1,
                            total_commands
                        ));
                    }
                    BuildStage::CommandComplete {
                        ..
                    } => {}
                }
            }
            SoarEvent::Running {
                op_id,
                pkg_name,
                stage,
            } => {
                self.row(*op_id, pkg_name).phase = match stage {
                    RunStage::CacheHit => Phase::Step("cached".into()),
                    RunStage::Downloading => Phase::Step("downloading".into()),
                    RunStage::Executing => Phase::Step("running".into()),
                    RunStage::Complete {
                        ..
                    } => Phase::Done,
                };
            }
            SoarEvent::ProcessOutput {
                op_id,
                pkg_name,
                line,
            } => {
                self.row(*op_id, pkg_name).last_output = Some(line.clone());
            }
            SoarEvent::OperationComplete {
                op_id,
                pkg_name,
            } => {
                self.row(*op_id, pkg_name).phase = Phase::Done;
            }
            SoarEvent::OperationFailed {
                op_id,
                pkg_name,
                error,
            } => {
                self.row(*op_id, pkg_name).phase = Phase::Failed(error.clone());
            }
            SoarEvent::OperationCancelled {
                op_id,
                pkg_name,
            } => {
                self.row(*op_id, pkg_name).phase = Phase::Skipped;
            }
            SoarEvent::BatchProgress {
                completed,
                total,
                failed,
            } => {
                self.batch = Some((*completed, *total, *failed));
            }
            SoarEvent::SyncProgress {
                repo_name,
                stage,
            } => {
                let label = match stage {
                    SyncStage::Fetching => "fetching",
                    SyncStage::UpToDate => "up to date",
                    SyncStage::Decompressing => "decompressing",
                    SyncStage::WritingDatabase => "writing",
                    SyncStage::Validating => "validating",
                    SyncStage::Complete {
                        ..
                    } => "done",
                };
                match self.sync.iter_mut().find(|(repo, _)| repo == repo_name) {
                    Some((_, known)) => *known = label.to_string(),
                    None => self.sync.push((repo_name.clone(), label.to_string())),
                }
            }
            SoarEvent::UpdateCheck {
                ..
            }
            | SoarEvent::ApplyComplete {
                ..
            }
            | SoarEvent::Log {
                ..
            } => {}
        }
    }

    /// The rows still working: waiting, downloading, or between stages.
    /// Finished, failed, and skipped rows drop out.
    pub(crate) fn active(&self) -> impl Iterator<Item = &OpRow> {
        self.ops
            .iter()
            .filter(|row| !matches!(row.phase, Phase::Done | Phase::Failed(_) | Phase::Skipped))
    }

    /// The repositories whose sync is still moving, in first-seen order.
    /// Repositories that reached their final stage drop out, so the
    /// activity strip stops listing them once their work ends.
    pub(crate) fn active_sync(&self) -> impl Iterator<Item = &(String, String)> {
        self.sync
            .iter()
            .filter(|(_, stage)| !matches!(stage.as_str(), "done" | "up to date"))
    }

    /// The row of one operation, appended at the end when it is new.
    fn row(&mut self, op_id: OperationId, name: &str) -> &mut OpRow {
        let index = match self.ops.iter().position(|row| row.op_id == op_id) {
            Some(index) => index,
            None => {
                self.ops.push(OpRow {
                    op_id,
                    name: name.to_string(),
                    phase: Phase::Waiting,
                    speed: None,
                    last_sample: None,
                    last_output: None,
                });
                self.ops.len() - 1
            }
        };
        &mut self.ops[index]
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use soar_events::{
        BuildStage, HookStage, InstallStage, RemoveStage, RunStage, SoarEvent, SyncStage,
        UpdateCheckStatus, UpdateCleanupStage, VerifyStage,
    };

    use super::{Phase, Tracker};

    fn preparing(op: u64, name: &str) -> SoarEvent {
        SoarEvent::DownloadPreparing {
            op_id: op,
            pkg_name: name.into(),
        }
    }

    fn progress(op: u64, current: u64, total: u64) -> SoarEvent {
        SoarEvent::DownloadProgress {
            op_id: op,
            pkg_name: "bat".into(),
            current,
            total,
        }
    }

    fn verifying(op: u64, stage: VerifyStage) -> SoarEvent {
        SoarEvent::Verifying {
            op_id: op,
            pkg_name: "bat".into(),
            stage,
        }
    }

    fn installing(op: u64, stage: InstallStage) -> SoarEvent {
        SoarEvent::Installing {
            op_id: op,
            pkg_name: "bat".into(),
            stage,
        }
    }

    fn removing(op: u64, stage: RemoveStage) -> SoarEvent {
        SoarEvent::Removing {
            op_id: op,
            pkg_name: "bat".into(),
            stage,
        }
    }

    fn running(op: u64, stage: RunStage) -> SoarEvent {
        SoarEvent::Running {
            op_id: op,
            pkg_name: "bat".into(),
            stage,
        }
    }

    fn building(op: u64, stage: BuildStage) -> SoarEvent {
        SoarEvent::Building {
            op_id: op,
            pkg_name: "bat".into(),
            stage,
        }
    }

    fn sync_progress(repo: &str, stage: SyncStage) -> SoarEvent {
        SoarEvent::SyncProgress {
            repo_name: repo.into(),
            stage,
        }
    }

    /// Asserts a smoothed speed within one byte per second of the expected
    /// one.
    fn assert_speed(actual: Option<f64>, expected: f64) {
        let actual = actual.unwrap();
        assert!(
            (actual - expected).abs() < 1.0,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn a_download_replays_from_connecting_to_done() {
        let mut tracker = Tracker::default();
        let start = Instant::now();
        tracker.apply(&preparing(1, "bat"), start);
        assert_eq!(tracker.ops[0].phase, Phase::Step("connecting".into()));
        tracker.apply(
            &SoarEvent::DownloadStarting {
                op_id: 1,
                pkg_name: "bat".into(),
                total: 1000,
            },
            start,
        );
        assert_eq!(
            tracker.ops[0].phase,
            Phase::Downloading {
                current: 0,
                total: 1000
            }
        );
        tracker.apply(&progress(1, 500, 1000), start + Duration::from_millis(500));
        assert_eq!(
            tracker.ops[0].phase,
            Phase::Downloading {
                current: 500,
                total: 1000
            }
        );
        assert_eq!(tracker.ops[0].speed, None);
        tracker.apply(
            &progress(1, 1000, 1000),
            start + Duration::from_millis(1000),
        );
        assert_speed(tracker.ops[0].speed, 1000.0);
        tracker.apply(&verifying(1, VerifyStage::Checksum), start);
        assert_eq!(
            tracker.ops[0].phase,
            Phase::Step("verifying checksum".into())
        );
        tracker.apply(&installing(1, InstallStage::LinkingBinaries), start);
        assert_eq!(tracker.ops[0].phase, Phase::Step("linking binaries".into()));
        tracker.apply(&installing(1, InstallStage::Complete), start);
        assert_eq!(tracker.ops[0].phase, Phase::Done);
    }

    #[test]
    fn speed_smooths_over_the_previous_sample() {
        let mut tracker = Tracker::default();
        let start = Instant::now();
        tracker.apply(&progress(1, 0, 10_000), start);
        tracker.apply(&progress(1, 1_000, 10_000), start + Duration::from_secs(1));
        tracker.apply(&progress(1, 3_000, 10_000), start + Duration::from_secs(2));
        assert_speed(tracker.ops[0].speed, 1300.0);
    }

    #[test]
    fn samples_closer_than_200ms_do_not_move_the_speed() {
        let mut tracker = Tracker::default();
        let start = Instant::now();
        tracker.apply(&progress(1, 0, 10_000), start);
        tracker.apply(&progress(1, 1_000, 10_000), start + Duration::from_secs(1));
        assert_speed(tracker.ops[0].speed, 1000.0);
        tracker.apply(
            &progress(1, 1_500, 10_000),
            start + Duration::from_millis(1100),
        );
        assert_speed(tracker.ops[0].speed, 1000.0);
        tracker.apply(
            &progress(1, 2_000, 10_000),
            start + Duration::from_millis(1500),
        );
        assert_speed(tracker.ops[0].speed, 1300.0);
    }

    #[test]
    fn interleaved_operations_keep_first_seen_order() {
        let mut tracker = Tracker::default();
        let start = Instant::now();
        tracker.apply(&preparing(1, "bat"), start);
        tracker.apply(&preparing(2, "fd"), start);
        tracker.apply(&progress(2, 100, 500), start);
        tracker.apply(&progress(1, 300, 500), start);
        assert_eq!(tracker.ops.len(), 2);
        assert_eq!(tracker.ops[0].op_id, 1);
        assert_eq!(tracker.ops[0].name, "bat");
        assert_eq!(
            tracker.ops[0].phase,
            Phase::Downloading {
                current: 300,
                total: 500
            }
        );
        assert_eq!(tracker.ops[1].op_id, 2);
        assert_eq!(tracker.ops[1].name, "fd");
        assert_eq!(
            tracker.ops[1].phase,
            Phase::Downloading {
                current: 100,
                total: 500
            }
        );
    }

    #[test]
    fn an_operation_failure_records_the_message() {
        let mut tracker = Tracker::default();
        let start = Instant::now();
        tracker.apply(&preparing(1, "bat"), start);
        tracker.apply(
            &SoarEvent::OperationFailed {
                op_id: 1,
                pkg_name: "bat".into(),
                error: "checksum mismatch".into(),
            },
            start,
        );
        assert_eq!(
            tracker.ops[0].phase,
            Phase::Failed("checksum mismatch".into())
        );
        assert_eq!(tracker.active().count(), 0);
    }

    #[test]
    fn a_cancelled_operation_is_skipped_and_inactive() {
        let mut tracker = Tracker::default();
        let start = Instant::now();
        tracker.apply(&preparing(1, "bat"), start);
        tracker.apply(
            &SoarEvent::OperationCancelled {
                op_id: 1,
                pkg_name: "bat".into(),
            },
            start,
        );
        assert_eq!(tracker.ops[0].phase, Phase::Skipped);
        assert_eq!(tracker.active().count(), 0);
    }

    #[test]
    fn batch_progress_records_the_counts_without_making_a_row() {
        let mut tracker = Tracker::default();
        let start = Instant::now();
        tracker.apply(
            &SoarEvent::BatchProgress {
                completed: 1,
                total: 3,
                failed: 1,
            },
            start,
        );
        tracker.apply(&preparing(1, "bat"), start);
        assert_eq!(tracker.batch, Some((1, 3, 1)));
        assert_eq!(tracker.ops.len(), 1);
    }

    #[test]
    fn sync_progress_upserts_repos_in_first_seen_order() {
        let mut tracker = Tracker::default();
        let start = Instant::now();
        tracker.apply(&sync_progress("bincache", SyncStage::Fetching), start);
        tracker.apply(&sync_progress("pkgforge", SyncStage::Decompressing), start);
        tracker.apply(
            &sync_progress(
                "bincache",
                SyncStage::Complete {
                    package_count: Some(1200),
                },
            ),
            start,
        );
        tracker.apply(&sync_progress("pkgforge", SyncStage::UpToDate), start);
        assert_eq!(
            tracker.sync,
            vec![
                ("bincache".to_string(), "done".to_string()),
                ("pkgforge".to_string(), "up to date".to_string()),
            ]
        );
    }

    #[test]
    fn process_output_keeps_the_phase_and_holds_the_line() {
        let mut tracker = Tracker::default();
        let start = Instant::now();
        tracker.apply(
            &installing(1, InstallStage::RunningHook("post_install".into())),
            start,
        );
        tracker.apply(
            &SoarEvent::ProcessOutput {
                op_id: 1,
                pkg_name: "bat".into(),
                line: "Created ~/.config/bat".into(),
            },
            start,
        );
        assert_eq!(
            tracker.ops[0].phase,
            Phase::Step("running post_install".into())
        );
        assert_eq!(
            tracker.ops[0].last_output.as_deref(),
            Some("Created ~/.config/bat")
        );
    }

    #[test]
    fn update_checks_touch_no_row() {
        let mut tracker = Tracker::default();
        let start = Instant::now();
        tracker.apply(&preparing(1, "bat"), start);
        tracker.apply(
            &SoarEvent::UpdateCheck {
                pkg_name: "bat".into(),
                status: UpdateCheckStatus::UpToDate {
                    version: "1.0.0".into(),
                },
            },
            start,
        );
        assert_eq!(tracker.ops.len(), 1);
        assert_eq!(tracker.batch, None);
        assert!(tracker.sync.is_empty());
    }

    #[test]
    fn stages_show_their_labels() {
        let cases: Vec<(SoarEvent, &str)> = vec![
            (verifying(1, VerifyStage::Signature), "verifying signature"),
            (verifying(1, VerifyStage::Passed), "verified"),
            (installing(1, InstallStage::Extracting), "extracting"),
            (installing(1, InstallStage::ExtractingNested), "extracting"),
            (
                installing(1, InstallStage::DesktopIntegration),
                "desktop integration",
            ),
            (
                installing(1, InstallStage::FetchingSet("onelf".into())),
                "fetching onelf",
            ),
            (
                installing(1, InstallStage::SetupPortable),
                "portable directories",
            ),
            (installing(1, InstallStage::RecordingDatabase), "recording"),
            (
                installing(1, InstallStage::RunningHook("post_download".into())),
                "running post_download",
            ),
            (
                removing(1, RemoveStage::RunningHook("pre_remove".into())),
                "running pre_remove",
            ),
            (removing(1, RemoveStage::UnlinkingBinaries), "unlinking"),
            (removing(1, RemoveStage::UnlinkingDesktop), "unlinking"),
            (removing(1, RemoveStage::UnlinkingIcons), "unlinking"),
            (
                removing(1, RemoveStage::RemovingDirectory),
                "deleting files",
            ),
            (removing(1, RemoveStage::CleaningDatabase), "recording"),
            (building(1, BuildStage::Sandboxing), "preparing sandbox"),
            (running(1, RunStage::CacheHit), "cached"),
            (running(1, RunStage::Downloading), "downloading"),
            (running(1, RunStage::Executing), "running"),
        ];
        for (event, label) in cases {
            let mut tracker = Tracker::default();
            tracker.apply(&event, Instant::now());
            assert_eq!(tracker.ops[0].phase, Phase::Step(label.into()), "{event:?}");
        }
    }

    #[test]
    fn failures_completions_and_resumes_map_their_phases() {
        let cases: Vec<(SoarEvent, Phase)> = vec![
            (
                verifying(1, VerifyStage::Failed("bad signature".into())),
                Phase::Failed("bad signature".into()),
            ),
            (
                SoarEvent::DownloadAborted {
                    op_id: 1,
                    pkg_name: "bat".into(),
                },
                Phase::Failed("download aborted".into()),
            ),
            (
                SoarEvent::DownloadRetry {
                    op_id: 1,
                    pkg_name: "bat".into(),
                },
                Phase::Step("retrying".into()),
            ),
            (
                SoarEvent::DownloadComplete {
                    op_id: 1,
                    pkg_name: "bat".into(),
                    total: 500,
                },
                Phase::Step("downloaded".into()),
            ),
            (
                SoarEvent::DownloadResuming {
                    op_id: 1,
                    pkg_name: "bat".into(),
                    current: 200,
                    total: 500,
                },
                Phase::Downloading {
                    current: 200,
                    total: 500,
                },
            ),
            (
                running(
                    1,
                    RunStage::Complete {
                        exit_code: 0,
                    },
                ),
                Phase::Done,
            ),
            (
                removing(
                    1,
                    RemoveStage::Complete {
                        size_freed: None,
                    },
                ),
                Phase::Done,
            ),
            (
                SoarEvent::OperationComplete {
                    op_id: 1,
                    pkg_name: "bat".into(),
                },
                Phase::Done,
            ),
            (
                SoarEvent::Hook {
                    op_id: 1,
                    pkg_name: "bat".into(),
                    hook_name: "post_install".into(),
                    stage: HookStage::Failed {
                        exit_code: Some(3),
                    },
                },
                Phase::Step("post_install failed (3)".into()),
            ),
            (
                SoarEvent::UpdateCleanup {
                    op_id: 1,
                    pkg_name: "bat".into(),
                    old_version: "0.23.0".into(),
                    stage: UpdateCleanupStage::Removing,
                },
                Phase::Step("removing 0.23.0".into()),
            ),
            (
                building(
                    1,
                    BuildStage::Running {
                        command_index: 1,
                        total_commands: 4,
                    },
                ),
                Phase::Step("building 2/4".into()),
            ),
        ];
        for (event, phase) in cases {
            let mut tracker = Tracker::default();
            tracker.apply(&event, Instant::now());
            assert_eq!(tracker.ops[0].phase, phase, "{event:?}");
        }
    }

    #[test]
    fn recovery_and_cleanup_ends_keep_the_phase() {
        let mut tracker = Tracker::default();
        let start = Instant::now();
        tracker.apply(&progress(1, 300, 500), start);
        tracker.apply(
            &SoarEvent::DownloadRecovered {
                op_id: 1,
                pkg_name: "bat".into(),
            },
            start,
        );
        tracker.apply(
            &SoarEvent::UpdateCleanup {
                op_id: 1,
                pkg_name: "bat".into(),
                old_version: "0.23.0".into(),
                stage: UpdateCleanupStage::Kept,
            },
            start,
        );
        tracker.apply(
            &SoarEvent::UpdateCleanup {
                op_id: 1,
                pkg_name: "bat".into(),
                old_version: "0.23.0".into(),
                stage: UpdateCleanupStage::Complete {
                    size_freed: None,
                },
            },
            start,
        );
        tracker.apply(
            &SoarEvent::Hook {
                op_id: 1,
                pkg_name: "bat".into(),
                hook_name: "post_install".into(),
                stage: HookStage::Complete,
            },
            start,
        );
        tracker.apply(
            &building(
                1,
                BuildStage::CommandComplete {
                    command_index: 0,
                },
            ),
            start,
        );
        assert_eq!(
            tracker.ops[0].phase,
            Phase::Downloading {
                current: 300,
                total: 500
            }
        );
    }

    #[test]
    fn eta_needs_a_known_size_and_a_useful_speed() {
        let mut tracker = Tracker::default();
        let start = Instant::now();
        tracker.apply(
            &SoarEvent::DownloadStarting {
                op_id: 1,
                pkg_name: "bat".into(),
                total: 10_000,
            },
            start,
        );
        tracker.apply(&progress(1, 1_000, 10_000), start);
        assert_eq!(tracker.ops[0].eta(), None);
        tracker.apply(&progress(1, 3_000, 10_000), start + Duration::from_secs(1));
        assert_eq!(tracker.ops[0].eta(), Some(Duration::from_millis(3500)));
        tracker.apply(&installing(1, InstallStage::Extracting), start);
        assert_eq!(tracker.ops[0].eta(), None);
    }

    #[test]
    fn eta_stays_empty_while_the_speed_is_a_crawl() {
        let mut tracker = Tracker::default();
        let start = Instant::now();
        tracker.apply(
            &SoarEvent::DownloadStarting {
                op_id: 1,
                pkg_name: "bat".into(),
                total: 1_000,
            },
            start,
        );
        tracker.apply(&progress(1, 0, 1_000), start);
        tracker.apply(&progress(1, 1, 1_000), start + Duration::from_secs(1));
        assert_speed(tracker.ops[0].speed, 1.0);
        assert_eq!(tracker.ops[0].eta(), None);
    }
}
