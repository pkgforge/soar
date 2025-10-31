use serde::{Deserialize, Serialize};

/// Download progress events
#[derive(Debug, Clone, Copy)]
pub enum Progress {
    Starting { total: u64 },
    Chunk { current: u64, total: u64 },
    Complete { total: u64 },
}

/// How to handle existing files
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverwriteMode {
    Skip,
    Force,
    Prompt,
}

/// Resume information stored in xattrs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumeInfo {
    pub downloaded: u64,
    pub total: u64,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}
