mod event;
mod sink;

use std::sync::Arc;

pub use event::*;
pub use sink::*;

/// Unique identifier for a running operation.
pub type OperationId = u64;

/// Shared handle to an event sink.
pub type EventSinkHandle = Arc<dyn EventSink>;

#[cfg(test)]
mod tests {
    use std::{
        io::{self, BufWriter, Write},
        sync::Mutex,
    };

    use super::*;

    /// A writer whose bytes stay readable after the sink takes it.
    #[derive(Clone, Default)]
    struct Shared(Arc<Mutex<Vec<u8>>>);

    impl Shared {
        fn text(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    impl Write for Shared {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn json_lines_wire_format() {
        let out = Shared::default();
        let sink = JsonLinesSink::new(out.clone());
        sink.emit(SoarEvent::DownloadProgress {
            op_id: 1,
            pkg_name: "a".into(),
            current: 5,
            total: 10,
        });
        sink.emit(SoarEvent::Installing {
            op_id: 1,
            pkg_name: "a".into(),
            stage: InstallStage::Extracting,
        });
        sink.emit(SoarEvent::Verifying {
            op_id: 1,
            pkg_name: "a".into(),
            stage: VerifyStage::Failed("bad".into()),
        });
        sink.emit(SoarEvent::Removing {
            op_id: 2,
            pkg_name: "b".into(),
            stage: RemoveStage::Complete {
                size_freed: Some(3),
            },
        });
        sink.emit(SoarEvent::Log {
            level: LogLevel::Warning,
            message: "m".into(),
        });
        sink.emit(SoarEvent::ProcessOutput {
            op_id: 1,
            pkg_name: "a".into(),
            line: "hello from hook".into(),
        });

        assert_eq!(
            out.text(),
            concat!(
                r#"{"type":"download_progress","op_id":1,"pkg_name":"a","current":5,"total":10}"#,
                "\n",
                r#"{"type":"installing","op_id":1,"pkg_name":"a","stage":"extracting"}"#,
                "\n",
                r#"{"type":"verifying","op_id":1,"pkg_name":"a","stage":{"failed":"bad"}}"#,
                "\n",
                r#"{"type":"removing","op_id":2,"pkg_name":"b","stage":{"complete":{"size_freed":3}}}"#,
                "\n",
                r#"{"type":"log","level":"warning","message":"m"}"#,
                "\n",
                r#"{"type":"process_output","op_id":1,"pkg_name":"a","line":"hello from hook"}"#,
                "\n",
            )
        );
    }

    #[test]
    fn json_lines_are_flushed_per_event() {
        let out = Shared::default();
        let sink = JsonLinesSink::new(BufWriter::new(out.clone()));
        sink.emit(SoarEvent::OperationComplete {
            op_id: 1,
            pkg_name: "a".into(),
        });
        assert!(out.text().ends_with('\n'), "event still buffered");
    }

    #[test]
    fn json_lines_stay_whole_across_threads() {
        let out = Shared::default();
        let sink = Arc::new(JsonLinesSink::new(out.clone()));
        let threads: Vec<_> = (0..8)
            .map(|t| {
                let sink = sink.clone();
                std::thread::spawn(move || {
                    for i in 0..200 {
                        sink.emit(SoarEvent::Log {
                            level: LogLevel::Info,
                            message: format!("{t}-{i}"),
                        });
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }

        let text = out.text();
        let lines: Vec<_> = text.lines().collect();
        assert_eq!(lines.len(), 8 * 200);
        for line in lines {
            let value: serde_json::Value = serde_json::from_str(line).unwrap();
            assert_eq!(value["type"], "log");
        }
    }

    #[test]
    fn test_null_sink() {
        let sink = NullSink;
        sink.emit(SoarEvent::Log {
            level: LogLevel::Info,
            message: "test".to_string(),
        });
    }

    #[test]
    fn test_channel_sink() {
        let (sink, rx) = ChannelSink::new();
        sink.emit(SoarEvent::DownloadStarting {
            op_id: 1,
            pkg_name: "test-pkg".to_string(),
            total: 1024,
        });
        sink.emit(SoarEvent::DownloadProgress {
            op_id: 1,
            pkg_name: "test-pkg".to_string(),
            current: 512,
            total: 1024,
        });
        sink.emit(SoarEvent::DownloadComplete {
            op_id: 1,
            pkg_name: "test-pkg".to_string(),
            total: 1024,
        });

        let events: Vec<_> = rx.try_iter().collect();
        assert_eq!(events.len(), 3);

        assert!(matches!(
            &events[0],
            SoarEvent::DownloadStarting {
                total: 1024,
                ..
            }
        ));
        assert!(matches!(
            &events[1],
            SoarEvent::DownloadProgress {
                current: 512,
                ..
            }
        ));
        assert!(matches!(&events[2], SoarEvent::DownloadComplete { .. }));
    }

    #[test]
    fn test_channel_sink_receiver_dropped() {
        let (sink, rx) = ChannelSink::new();
        drop(rx);
        sink.emit(SoarEvent::Log {
            level: LogLevel::Info,
            message: "orphaned".to_string(),
        });
    }

    #[test]
    fn test_collector_sink() {
        let sink = CollectorSink::default();
        assert!(sink.is_empty());

        sink.emit(SoarEvent::SyncProgress {
            repo_name: "bincache".to_string(),
            stage: SyncStage::Fetching,
        });
        sink.emit(SoarEvent::SyncProgress {
            repo_name: "bincache".to_string(),
            stage: SyncStage::Complete {
                package_count: Some(100),
            },
        });

        assert_eq!(sink.len(), 2);
        let events = sink.events();
        assert!(matches!(
            &events[0],
            SoarEvent::SyncProgress {
                stage: SyncStage::Fetching,
                ..
            }
        ));
        assert!(matches!(
            &events[1],
            SoarEvent::SyncProgress {
                stage: SyncStage::Complete { .. },
                ..
            }
        ));
    }

    #[test]
    fn test_event_sink_handle() {
        let sink: EventSinkHandle = Arc::new(NullSink);
        sink.emit(SoarEvent::BatchProgress {
            completed: 5,
            total: 10,
            failed: 0,
        });

        let collector = Arc::new(CollectorSink::default());
        let sink: EventSinkHandle = collector.clone();
        sink.emit(SoarEvent::OperationComplete {
            op_id: 42,
            pkg_name: "pkg".to_string(),
        });
        assert_eq!(collector.len(), 1);
    }

    #[test]
    fn test_event_sink_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<NullSink>();
        assert_send_sync::<ChannelSink>();
        assert_send_sync::<CollectorSink>();
    }

    /// One value of every event variant.
    fn every_variant() -> Vec<SoarEvent> {
        vec![
            // Download lifecycle
            SoarEvent::DownloadPreparing {
                op_id: 1,
                pkg_name: "a".into(),
            },
            SoarEvent::DownloadStarting {
                op_id: 1,
                pkg_name: "a".into(),
                total: 100,
            },
            SoarEvent::DownloadResuming {
                op_id: 1,
                pkg_name: "a".into(),
                current: 50,
                total: 100,
            },
            SoarEvent::DownloadProgress {
                op_id: 1,
                pkg_name: "a".into(),
                current: 75,
                total: 100,
            },
            SoarEvent::DownloadComplete {
                op_id: 1,
                pkg_name: "a".into(),
                total: 100,
            },
            SoarEvent::DownloadRetry {
                op_id: 2,
                pkg_name: "b".into(),
            },
            SoarEvent::DownloadAborted {
                op_id: 2,
                pkg_name: "b".into(),
            },
            SoarEvent::DownloadRecovered {
                op_id: 3,
                pkg_name: "c".into(),
            },
            // Verification
            SoarEvent::Verifying {
                op_id: 1,
                pkg_name: "a".into(),
                stage: VerifyStage::Checksum,
            },
            SoarEvent::Verifying {
                op_id: 1,
                pkg_name: "a".into(),
                stage: VerifyStage::Signature,
            },
            SoarEvent::Verifying {
                op_id: 1,
                pkg_name: "a".into(),
                stage: VerifyStage::Passed,
            },
            SoarEvent::Verifying {
                op_id: 1,
                pkg_name: "a".into(),
                stage: VerifyStage::Failed("bad checksum".into()),
            },
            // Installation stages
            SoarEvent::Installing {
                op_id: 1,
                pkg_name: "a".into(),
                stage: InstallStage::Extracting,
            },
            SoarEvent::Installing {
                op_id: 1,
                pkg_name: "a".into(),
                stage: InstallStage::ExtractingNested,
            },
            SoarEvent::Installing {
                op_id: 1,
                pkg_name: "a".into(),
                stage: InstallStage::LinkingBinaries,
            },
            SoarEvent::Installing {
                op_id: 1,
                pkg_name: "a".into(),
                stage: InstallStage::DesktopIntegration,
            },
            SoarEvent::Installing {
                op_id: 1,
                pkg_name: "a".into(),
                stage: InstallStage::FetchingSet("onelf".into()),
            },
            SoarEvent::Installing {
                op_id: 1,
                pkg_name: "a".into(),
                stage: InstallStage::SetupPortable,
            },
            SoarEvent::Installing {
                op_id: 1,
                pkg_name: "a".into(),
                stage: InstallStage::RecordingDatabase,
            },
            SoarEvent::Installing {
                op_id: 1,
                pkg_name: "a".into(),
                stage: InstallStage::RunningHook("post_install".into()),
            },
            SoarEvent::Installing {
                op_id: 1,
                pkg_name: "a".into(),
                stage: InstallStage::Complete,
            },
            // Removal stages
            SoarEvent::Removing {
                op_id: 5,
                pkg_name: "e".into(),
                stage: RemoveStage::RunningHook("pre_remove".into()),
            },
            SoarEvent::Removing {
                op_id: 5,
                pkg_name: "e".into(),
                stage: RemoveStage::UnlinkingBinaries,
            },
            SoarEvent::Removing {
                op_id: 5,
                pkg_name: "e".into(),
                stage: RemoveStage::UnlinkingDesktop,
            },
            SoarEvent::Removing {
                op_id: 5,
                pkg_name: "e".into(),
                stage: RemoveStage::UnlinkingIcons,
            },
            SoarEvent::Removing {
                op_id: 5,
                pkg_name: "e".into(),
                stage: RemoveStage::RemovingDirectory,
            },
            SoarEvent::Removing {
                op_id: 5,
                pkg_name: "e".into(),
                stage: RemoveStage::CleaningDatabase,
            },
            SoarEvent::Removing {
                op_id: 5,
                pkg_name: "e".into(),
                stage: RemoveStage::Complete {
                    size_freed: Some(1024 * 1024),
                },
            },
            // Update checks
            SoarEvent::UpdateCheck {
                pkg_name: "f".into(),
                status: UpdateCheckStatus::Available {
                    current_version: "1.0.0".into(),
                    new_version: "2.0.0".into(),
                },
            },
            SoarEvent::UpdateCheck {
                pkg_name: "g".into(),
                status: UpdateCheckStatus::UpToDate {
                    version: "1.0.0".into(),
                },
            },
            SoarEvent::UpdateCheck {
                pkg_name: "h".into(),
                status: UpdateCheckStatus::Skipped {
                    reason: "pinned".into(),
                },
            },
            // Update cleanup
            SoarEvent::UpdateCleanup {
                op_id: 1,
                pkg_name: "a".into(),
                old_version: "1.0.0".into(),
                stage: UpdateCleanupStage::Removing,
            },
            SoarEvent::UpdateCleanup {
                op_id: 1,
                pkg_name: "a".into(),
                old_version: "1.0.0".into(),
                stage: UpdateCleanupStage::Complete {
                    size_freed: Some(512),
                },
            },
            SoarEvent::UpdateCleanup {
                op_id: 1,
                pkg_name: "a".into(),
                old_version: "1.0.0".into(),
                stage: UpdateCleanupStage::Kept,
            },
            // Hook execution
            SoarEvent::Hook {
                op_id: 5,
                pkg_name: "e".into(),
                hook_name: "pre_remove".into(),
                stage: HookStage::Starting,
            },
            SoarEvent::Hook {
                op_id: 5,
                pkg_name: "e".into(),
                hook_name: "pre_remove".into(),
                stage: HookStage::Complete,
            },
            SoarEvent::Hook {
                op_id: 6,
                pkg_name: "i".into(),
                hook_name: "post_install".into(),
                stage: HookStage::Failed {
                    exit_code: Some(1),
                },
            },
            // Run command
            SoarEvent::Running {
                op_id: 7,
                pkg_name: "j".into(),
                stage: RunStage::CacheHit,
            },
            SoarEvent::Running {
                op_id: 8,
                pkg_name: "k".into(),
                stage: RunStage::Downloading,
            },
            SoarEvent::Running {
                op_id: 7,
                pkg_name: "j".into(),
                stage: RunStage::Executing,
            },
            SoarEvent::Running {
                op_id: 7,
                pkg_name: "j".into(),
                stage: RunStage::Complete {
                    exit_code: 0,
                },
            },
            // Build
            SoarEvent::Building {
                op_id: 4,
                pkg_name: "d".into(),
                stage: BuildStage::Sandboxing,
            },
            SoarEvent::Building {
                op_id: 4,
                pkg_name: "d".into(),
                stage: BuildStage::Running {
                    command_index: 0,
                    total_commands: 3,
                },
            },
            SoarEvent::Building {
                op_id: 4,
                pkg_name: "d".into(),
                stage: BuildStage::CommandComplete {
                    command_index: 0,
                },
            },
            // Operation completion
            SoarEvent::OperationComplete {
                op_id: 1,
                pkg_name: "a".into(),
            },
            SoarEvent::OperationFailed {
                op_id: 2,
                pkg_name: "b".into(),
                error: "not found".into(),
            },
            // Process output and cancellation
            SoarEvent::ProcessOutput {
                op_id: 1,
                pkg_name: "a".into(),
                line: "hello from hook".into(),
            },
            SoarEvent::OperationCancelled {
                op_id: 2,
                pkg_name: "b".into(),
            },
            // Sync stages
            SoarEvent::SyncProgress {
                repo_name: "repo".into(),
                stage: SyncStage::Fetching,
            },
            SoarEvent::SyncProgress {
                repo_name: "repo".into(),
                stage: SyncStage::UpToDate,
            },
            SoarEvent::SyncProgress {
                repo_name: "repo".into(),
                stage: SyncStage::Decompressing,
            },
            SoarEvent::SyncProgress {
                repo_name: "repo".into(),
                stage: SyncStage::WritingDatabase,
            },
            SoarEvent::SyncProgress {
                repo_name: "repo".into(),
                stage: SyncStage::Validating,
            },
            SoarEvent::SyncProgress {
                repo_name: "repo".into(),
                stage: SyncStage::Complete {
                    package_count: Some(500),
                },
            },
            // Batch, apply, log
            SoarEvent::BatchProgress {
                completed: 5,
                total: 10,
                failed: 1,
            },
            SoarEvent::ApplyComplete {
                installed: 3,
                updated: 1,
                removed: 2,
                failed: 0,
            },
            SoarEvent::Log {
                level: LogLevel::Debug,
                message: "debug".into(),
            },
            SoarEvent::Log {
                level: LogLevel::Info,
                message: "info".into(),
            },
            SoarEvent::Log {
                level: LogLevel::Warning,
                message: "warning".into(),
            },
            SoarEvent::Log {
                level: LogLevel::Error,
                message: "error".into(),
            },
        ]
    }

    #[test]
    fn test_all_event_variants() {
        let collector = CollectorSink::default();
        for event in every_variant() {
            collector.emit(event);
        }
        assert_eq!(collector.len(), 60);
    }

    #[test]
    fn every_variant_round_trips_through_json() {
        for event in every_variant() {
            let json = serde_json::to_string(&event).unwrap();
            let parsed: SoarEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, event);
        }
    }
}
