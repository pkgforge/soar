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
    use super::*;

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
            pkg_id: "test-pkg-id".to_string(),
            total: 1024,
        });
        sink.emit(SoarEvent::DownloadProgress {
            op_id: 1,
            pkg_name: "test-pkg".to_string(),
            pkg_id: "test-pkg-id".to_string(),
            current: 512,
            total: 1024,
        });
        sink.emit(SoarEvent::DownloadComplete {
            op_id: 1,
            pkg_name: "test-pkg".to_string(),
            pkg_id: "test-pkg-id".to_string(),
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
            pkg_id: "pkg-id".to_string(),
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

    #[test]
    fn test_all_event_variants() {
        let collector = CollectorSink::default();

        // Download lifecycle
        collector.emit(SoarEvent::DownloadStarting {
            op_id: 1,
            pkg_name: "a".into(),
            pkg_id: "a-id".into(),
            total: 100,
        });
        collector.emit(SoarEvent::DownloadResuming {
            op_id: 1,
            pkg_name: "a".into(),
            pkg_id: "a-id".into(),
            current: 50,
            total: 100,
        });
        collector.emit(SoarEvent::DownloadProgress {
            op_id: 1,
            pkg_name: "a".into(),
            pkg_id: "a-id".into(),
            current: 75,
            total: 100,
        });
        collector.emit(SoarEvent::DownloadComplete {
            op_id: 1,
            pkg_name: "a".into(),
            pkg_id: "a-id".into(),
            total: 100,
        });
        collector.emit(SoarEvent::DownloadRetry {
            op_id: 2,
            pkg_name: "b".into(),
            pkg_id: "b-id".into(),
        });
        collector.emit(SoarEvent::DownloadAborted {
            op_id: 2,
            pkg_name: "b".into(),
            pkg_id: "b-id".into(),
        });
        collector.emit(SoarEvent::DownloadRecovered {
            op_id: 3,
            pkg_name: "c".into(),
            pkg_id: "c-id".into(),
        });

        // Verification
        collector.emit(SoarEvent::Verifying {
            op_id: 1,
            pkg_name: "a".into(),
            pkg_id: "a-id".into(),
            stage: VerifyStage::Checksum,
        });
        collector.emit(SoarEvent::Verifying {
            op_id: 1,
            pkg_name: "a".into(),
            pkg_id: "a-id".into(),
            stage: VerifyStage::Signature,
        });
        collector.emit(SoarEvent::Verifying {
            op_id: 1,
            pkg_name: "a".into(),
            pkg_id: "a-id".into(),
            stage: VerifyStage::Passed,
        });
        collector.emit(SoarEvent::Verifying {
            op_id: 1,
            pkg_name: "a".into(),
            pkg_id: "a-id".into(),
            stage: VerifyStage::Failed("bad checksum".into()),
        });

        // Installation stages
        collector.emit(SoarEvent::Installing {
            op_id: 1,
            pkg_name: "a".into(),
            pkg_id: "a-id".into(),
            stage: InstallStage::Extracting,
        });
        collector.emit(SoarEvent::Installing {
            op_id: 1,
            pkg_name: "a".into(),
            pkg_id: "a-id".into(),
            stage: InstallStage::ExtractingNested,
        });
        collector.emit(SoarEvent::Installing {
            op_id: 1,
            pkg_name: "a".into(),
            pkg_id: "a-id".into(),
            stage: InstallStage::LinkingBinaries,
        });
        collector.emit(SoarEvent::Installing {
            op_id: 1,
            pkg_name: "a".into(),
            pkg_id: "a-id".into(),
            stage: InstallStage::DesktopIntegration,
        });
        collector.emit(SoarEvent::Installing {
            op_id: 1,
            pkg_name: "a".into(),
            pkg_id: "a-id".into(),
            stage: InstallStage::SetupPortable,
        });
        collector.emit(SoarEvent::Installing {
            op_id: 1,
            pkg_name: "a".into(),
            pkg_id: "a-id".into(),
            stage: InstallStage::RecordingDatabase,
        });
        collector.emit(SoarEvent::Installing {
            op_id: 1,
            pkg_name: "a".into(),
            pkg_id: "a-id".into(),
            stage: InstallStage::RunningHook("post_install".into()),
        });
        collector.emit(SoarEvent::Installing {
            op_id: 1,
            pkg_name: "a".into(),
            pkg_id: "a-id".into(),
            stage: InstallStage::Complete,
        });

        // Removal stages
        collector.emit(SoarEvent::Removing {
            op_id: 5,
            pkg_name: "e".into(),
            pkg_id: "e-id".into(),
            stage: RemoveStage::RunningHook("pre_remove".into()),
        });
        collector.emit(SoarEvent::Removing {
            op_id: 5,
            pkg_name: "e".into(),
            pkg_id: "e-id".into(),
            stage: RemoveStage::UnlinkingBinaries,
        });
        collector.emit(SoarEvent::Removing {
            op_id: 5,
            pkg_name: "e".into(),
            pkg_id: "e-id".into(),
            stage: RemoveStage::UnlinkingDesktop,
        });
        collector.emit(SoarEvent::Removing {
            op_id: 5,
            pkg_name: "e".into(),
            pkg_id: "e-id".into(),
            stage: RemoveStage::UnlinkingIcons,
        });
        collector.emit(SoarEvent::Removing {
            op_id: 5,
            pkg_name: "e".into(),
            pkg_id: "e-id".into(),
            stage: RemoveStage::RemovingDirectory,
        });
        collector.emit(SoarEvent::Removing {
            op_id: 5,
            pkg_name: "e".into(),
            pkg_id: "e-id".into(),
            stage: RemoveStage::CleaningDatabase,
        });
        collector.emit(SoarEvent::Removing {
            op_id: 5,
            pkg_name: "e".into(),
            pkg_id: "e-id".into(),
            stage: RemoveStage::Complete {
                size_freed: Some(1024 * 1024),
            },
        });

        // Update checks
        collector.emit(SoarEvent::UpdateCheck {
            pkg_name: "f".into(),
            pkg_id: "f-id".into(),
            status: UpdateCheckStatus::Available {
                current_version: "1.0.0".into(),
                new_version: "2.0.0".into(),
            },
        });
        collector.emit(SoarEvent::UpdateCheck {
            pkg_name: "g".into(),
            pkg_id: "g-id".into(),
            status: UpdateCheckStatus::UpToDate {
                version: "1.0.0".into(),
            },
        });
        collector.emit(SoarEvent::UpdateCheck {
            pkg_name: "h".into(),
            pkg_id: "h-id".into(),
            status: UpdateCheckStatus::Skipped {
                reason: "pinned".into(),
            },
        });

        // Update cleanup
        collector.emit(SoarEvent::UpdateCleanup {
            op_id: 1,
            pkg_name: "a".into(),
            pkg_id: "a-id".into(),
            old_version: "1.0.0".into(),
            stage: UpdateCleanupStage::Removing,
        });
        collector.emit(SoarEvent::UpdateCleanup {
            op_id: 1,
            pkg_name: "a".into(),
            pkg_id: "a-id".into(),
            old_version: "1.0.0".into(),
            stage: UpdateCleanupStage::Complete {
                size_freed: Some(512),
            },
        });
        collector.emit(SoarEvent::UpdateCleanup {
            op_id: 1,
            pkg_name: "a".into(),
            pkg_id: "a-id".into(),
            old_version: "1.0.0".into(),
            stage: UpdateCleanupStage::Kept,
        });

        // Hook execution
        collector.emit(SoarEvent::Hook {
            op_id: 5,
            pkg_name: "e".into(),
            pkg_id: "e-id".into(),
            hook_name: "pre_remove".into(),
            stage: HookStage::Starting,
        });
        collector.emit(SoarEvent::Hook {
            op_id: 5,
            pkg_name: "e".into(),
            pkg_id: "e-id".into(),
            hook_name: "pre_remove".into(),
            stage: HookStage::Complete,
        });
        collector.emit(SoarEvent::Hook {
            op_id: 6,
            pkg_name: "i".into(),
            pkg_id: "i-id".into(),
            hook_name: "post_install".into(),
            stage: HookStage::Failed {
                exit_code: Some(1),
            },
        });

        // Run command
        collector.emit(SoarEvent::Running {
            op_id: 7,
            pkg_name: "j".into(),
            pkg_id: "j-id".into(),
            stage: RunStage::CacheHit,
        });
        collector.emit(SoarEvent::Running {
            op_id: 8,
            pkg_name: "k".into(),
            pkg_id: "k-id".into(),
            stage: RunStage::Downloading,
        });
        collector.emit(SoarEvent::Running {
            op_id: 7,
            pkg_name: "j".into(),
            pkg_id: "j-id".into(),
            stage: RunStage::Executing,
        });
        collector.emit(SoarEvent::Running {
            op_id: 7,
            pkg_name: "j".into(),
            pkg_id: "j-id".into(),
            stage: RunStage::Complete {
                exit_code: 0,
            },
        });

        // Build
        collector.emit(SoarEvent::Building {
            op_id: 4,
            pkg_name: "d".into(),
            pkg_id: "d-id".into(),
            stage: BuildStage::Sandboxing,
        });
        collector.emit(SoarEvent::Building {
            op_id: 4,
            pkg_name: "d".into(),
            pkg_id: "d-id".into(),
            stage: BuildStage::Running {
                command_index: 0,
                total_commands: 3,
            },
        });
        collector.emit(SoarEvent::Building {
            op_id: 4,
            pkg_name: "d".into(),
            pkg_id: "d-id".into(),
            stage: BuildStage::CommandComplete {
                command_index: 0,
            },
        });

        // Operation completion
        collector.emit(SoarEvent::OperationComplete {
            op_id: 1,
            pkg_name: "a".into(),
            pkg_id: "a-id".into(),
        });
        collector.emit(SoarEvent::OperationFailed {
            op_id: 2,
            pkg_name: "b".into(),
            error: "not found".into(),
        });

        // Sync stages
        collector.emit(SoarEvent::SyncProgress {
            repo_name: "repo".into(),
            stage: SyncStage::Fetching,
        });
        collector.emit(SoarEvent::SyncProgress {
            repo_name: "repo".into(),
            stage: SyncStage::UpToDate,
        });
        collector.emit(SoarEvent::SyncProgress {
            repo_name: "repo".into(),
            stage: SyncStage::Decompressing,
        });
        collector.emit(SoarEvent::SyncProgress {
            repo_name: "repo".into(),
            stage: SyncStage::WritingDatabase,
        });
        collector.emit(SoarEvent::SyncProgress {
            repo_name: "repo".into(),
            stage: SyncStage::Validating,
        });
        collector.emit(SoarEvent::SyncProgress {
            repo_name: "repo".into(),
            stage: SyncStage::Complete {
                package_count: Some(500),
            },
        });

        // Batch + Log
        collector.emit(SoarEvent::BatchProgress {
            completed: 5,
            total: 10,
            failed: 1,
        });
        collector.emit(SoarEvent::Log {
            level: LogLevel::Debug,
            message: "debug".into(),
        });
        collector.emit(SoarEvent::Log {
            level: LogLevel::Info,
            message: "info".into(),
        });
        collector.emit(SoarEvent::Log {
            level: LogLevel::Warning,
            message: "warning".into(),
        });
        collector.emit(SoarEvent::Log {
            level: LogLevel::Error,
            message: "error".into(),
        });

        assert_eq!(collector.len(), 55);
    }
}
