use serde::{Deserialize, Serialize};

/// Download progress events
///
/// `Preparing` is the wait on the remote: the request is out, but no byte has
/// arrived and the size is still unknown.
#[derive(Debug, Clone, Copy)]
pub enum Progress {
    Preparing,
    Starting { total: u64 },
    Resuming { current: u64, total: u64 },
    Chunk { current: u64, total: u64 },
    Complete { total: u64 },
    Error,
    Aborted,
    Recovered,
}

/// How to handle existing files
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverwriteMode {
    Skip,
    Force,
    Prompt,
}

/// Where one segment of a segmented download stands.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentProgress {
    /// First byte of the segment, absolute in the file.
    pub start: u64,
    /// Last byte of the segment (inclusive), absolute in the file.
    pub end: u64,
    /// Bytes of this segment already on disk, counted from `start`.
    pub written: u64,
}

/// Resume information stored in xattrs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumeInfo {
    pub downloaded: u64,
    pub total: u64,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    /// Per-segment state of a segmented download. `None` for the
    /// single-stream layout, where `downloaded` is a prefix length.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub segments: Option<Vec<SegmentProgress>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_starting() {
        let progress = Progress::Starting {
            total: 1024,
        };
        match progress {
            Progress::Starting {
                total,
            } => assert_eq!(total, 1024),
            _ => panic!("Expected Progress::Starting"),
        }
    }

    #[test]
    fn test_progress_chunk() {
        let progress = Progress::Chunk {
            current: 512,
            total: 1024,
        };
        match progress {
            Progress::Chunk {
                current,
                total,
            } => {
                assert_eq!(current, 512);
                assert_eq!(total, 1024);
            }
            _ => panic!("Expected Progress::Chunk"),
        }
    }

    #[test]
    fn test_progress_complete() {
        let progress = Progress::Complete {
            total: 1024,
        };
        match progress {
            Progress::Complete {
                total,
            } => assert_eq!(total, 1024),
            _ => panic!("Expected Progress::Complete"),
        }
    }

    #[test]
    fn test_progress_clone() {
        let p1 = Progress::Starting {
            total: 100,
        };
        let p2 = p1;
        match (p1, p2) {
            (
                Progress::Starting {
                    total: t1,
                },
                Progress::Starting {
                    total: t2,
                },
            ) => {
                assert_eq!(t1, t2);
            }
            _ => panic!("Clone failed"),
        }
    }

    #[test]
    fn test_overwrite_mode_equality() {
        assert_eq!(OverwriteMode::Skip, OverwriteMode::Skip);
        assert_eq!(OverwriteMode::Force, OverwriteMode::Force);
        assert_eq!(OverwriteMode::Prompt, OverwriteMode::Prompt);

        assert_ne!(OverwriteMode::Skip, OverwriteMode::Force);
        assert_ne!(OverwriteMode::Force, OverwriteMode::Prompt);
        assert_ne!(OverwriteMode::Skip, OverwriteMode::Prompt);
    }

    #[test]
    fn test_overwrite_mode_clone() {
        let mode1 = OverwriteMode::Force;
        let mode2 = mode1;
        assert_eq!(mode1, mode2);
    }

    #[test]
    fn test_resume_info_with_etag() {
        let info = ResumeInfo {
            downloaded: 512,
            total: 1024,
            etag: Some("\"abc123\"".to_string()),
            last_modified: None,
            segments: None,
        };

        assert_eq!(info.downloaded, 512);
        assert_eq!(info.total, 1024);
        assert_eq!(info.etag, Some("\"abc123\"".to_string()));
        assert_eq!(info.last_modified, None);
    }

    #[test]
    fn test_resume_info_with_last_modified() {
        let info = ResumeInfo {
            downloaded: 256,
            total: 1024,
            etag: None,
            last_modified: Some("Wed, 21 Oct 2015 07:28:00 GMT".to_string()),
            segments: None,
        };

        assert_eq!(info.downloaded, 256);
        assert_eq!(
            info.last_modified,
            Some("Wed, 21 Oct 2015 07:28:00 GMT".to_string())
        );
    }

    #[test]
    fn test_resume_info_serialize_deserialize() {
        let info = ResumeInfo {
            downloaded: 1024,
            total: 2048,
            etag: Some("etag-value".to_string()),
            last_modified: Some("date".to_string()),
            segments: None,
        };

        let json = serde_json::to_string(&info).unwrap();
        let deserialized: ResumeInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.downloaded, info.downloaded);
        assert_eq!(deserialized.total, info.total);
        assert_eq!(deserialized.etag, info.etag);
        assert_eq!(deserialized.last_modified, info.last_modified);
        assert!(deserialized.segments.is_none());
    }

    #[test]
    fn resume_info_segments_round_trip() {
        let info = ResumeInfo {
            downloaded: 100,
            total: 200,
            etag: None,
            last_modified: None,
            segments: Some(vec![SegmentProgress {
                start: 0,
                end: 99,
                written: 100,
            }]),
        };

        let json = serde_json::to_string(&info).unwrap();
        let deserialized: ResumeInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.segments.as_ref().unwrap()[0].written, 100);
    }

    #[test]
    fn resume_info_without_segments_parses_from_old_json() {
        let old = r#"{"downloaded":10,"total":20,"etag":null,"last_modified":null}"#;
        let info: ResumeInfo = serde_json::from_str(old).unwrap();
        assert_eq!(info.downloaded, 10);
        assert!(info.segments.is_none());
    }

    #[test]
    fn test_resume_info_clone() {
        let info1 = ResumeInfo {
            downloaded: 100,
            total: 200,
            etag: Some("tag".to_string()),
            last_modified: None,
            segments: None,
        };

        let info2 = info1.clone();
        assert_eq!(info1.downloaded, info2.downloaded);
        assert_eq!(info1.total, info2.total);
        assert_eq!(info1.etag, info2.etag);
    }
}
