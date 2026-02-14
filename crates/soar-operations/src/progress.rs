use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use soar_dl::types::Progress;
use soar_events::{EventSinkHandle, OperationId, SoarEvent};

/// Creates a soar-dl progress callback that bridges to SoarEvent emissions.
///
/// The returned closure can be passed to `PackageInstaller::new()` as the
/// `progress_callback` parameter.
pub fn create_progress_bridge(
    events: EventSinkHandle,
    op_id: OperationId,
    pkg_name: String,
    pkg_id: String,
) -> Arc<dyn Fn(Progress) + Send + Sync> {
    Arc::new(move |progress| {
        let event = match progress {
            Progress::Starting {
                total,
            } => {
                SoarEvent::DownloadStarting {
                    op_id,
                    pkg_name: pkg_name.clone(),
                    pkg_id: pkg_id.clone(),
                    total,
                }
            }
            Progress::Resuming {
                current,
                total,
            } => {
                SoarEvent::DownloadResuming {
                    op_id,
                    pkg_name: pkg_name.clone(),
                    pkg_id: pkg_id.clone(),
                    current,
                    total,
                }
            }
            Progress::Chunk {
                current,
                total,
            } => {
                SoarEvent::DownloadProgress {
                    op_id,
                    pkg_name: pkg_name.clone(),
                    pkg_id: pkg_id.clone(),
                    current,
                    total,
                }
            }
            Progress::Complete {
                total,
            } => {
                SoarEvent::DownloadComplete {
                    op_id,
                    pkg_name: pkg_name.clone(),
                    pkg_id: pkg_id.clone(),
                    total,
                }
            }
            Progress::Error => {
                SoarEvent::DownloadRetry {
                    op_id,
                    pkg_name: pkg_name.clone(),
                    pkg_id: pkg_id.clone(),
                }
            }
            Progress::Aborted => {
                SoarEvent::DownloadAborted {
                    op_id,
                    pkg_name: pkg_name.clone(),
                    pkg_id: pkg_id.clone(),
                }
            }
            Progress::Recovered => {
                SoarEvent::DownloadRecovered {
                    op_id,
                    pkg_name: pkg_name.clone(),
                    pkg_id: pkg_id.clone(),
                }
            }
        };
        events.emit(event);
    })
}

/// Generates a unique operation ID.
pub fn next_op_id() -> OperationId {
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use soar_events::{CollectorSink, SoarEvent};

    use super::*;

    #[test]
    fn test_next_op_id_is_unique() {
        let id1 = next_op_id();
        let id2 = next_op_id();
        let id3 = next_op_id();
        assert_ne!(id1, id2);
        assert_ne!(id2, id3);
    }

    #[test]
    fn test_progress_bridge_maps_all_variants() {
        let collector = Arc::new(CollectorSink::default());
        let events: EventSinkHandle = collector.clone();

        let bridge = create_progress_bridge(events, 1, "pkg".into(), "pkg-id".into());

        bridge(Progress::Starting {
            total: 1000,
        });
        bridge(Progress::Resuming {
            current: 500,
            total: 1000,
        });
        bridge(Progress::Chunk {
            current: 750,
            total: 1000,
        });
        bridge(Progress::Complete {
            total: 1000,
        });
        bridge(Progress::Error);
        bridge(Progress::Aborted);
        bridge(Progress::Recovered);

        let events = collector.events();
        assert_eq!(events.len(), 7);

        assert!(matches!(
            &events[0],
            SoarEvent::DownloadStarting {
                total: 1000,
                ..
            }
        ));
        assert!(matches!(
            &events[1],
            SoarEvent::DownloadResuming {
                current: 500,
                total: 1000,
                ..
            }
        ));
        assert!(matches!(
            &events[2],
            SoarEvent::DownloadProgress {
                current: 750,
                total: 1000,
                ..
            }
        ));
        assert!(matches!(
            &events[3],
            SoarEvent::DownloadComplete {
                total: 1000,
                ..
            }
        ));
        assert!(matches!(&events[4], SoarEvent::DownloadRetry { .. }));
        assert!(matches!(&events[5], SoarEvent::DownloadAborted { .. }));
        assert!(matches!(&events[6], SoarEvent::DownloadRecovered { .. }));
    }
}
