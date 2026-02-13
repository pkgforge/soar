use crate::OperationId;

/// All event types emitted by soar operations.
#[derive(Debug, Clone)]
pub enum SoarEvent {
    /// Download is starting.
    DownloadStarting {
        op_id: OperationId,
        pkg_name: String,
        pkg_id: String,
        total: u64,
    },
    /// Download is resuming from a previous checkpoint.
    DownloadResuming {
        op_id: OperationId,
        pkg_name: String,
        pkg_id: String,
        current: u64,
        total: u64,
    },
    /// Download progress update.
    DownloadProgress {
        op_id: OperationId,
        pkg_name: String,
        pkg_id: String,
        current: u64,
        total: u64,
    },
    /// Download completed successfully.
    DownloadComplete {
        op_id: OperationId,
        pkg_name: String,
        pkg_id: String,
        total: u64,
    },
    /// Download error, retrying.
    DownloadRetry {
        op_id: OperationId,
        pkg_name: String,
        pkg_id: String,
    },
    /// Download permanently failed after retries.
    DownloadAborted {
        op_id: OperationId,
        pkg_name: String,
        pkg_id: String,
    },
    /// Download recovered from an error.
    DownloadRecovered {
        op_id: OperationId,
        pkg_name: String,
        pkg_id: String,
    },
    /// Verification stage.
    Verifying {
        op_id: OperationId,
        pkg_name: String,
        pkg_id: String,
        stage: VerifyStage,
    },
    /// Install/extraction stage.
    Installing {
        op_id: OperationId,
        pkg_name: String,
        pkg_id: String,
        stage: InstallStage,
    },
    /// Package removal stage.
    Removing {
        op_id: OperationId,
        pkg_name: String,
        pkg_id: String,
        stage: RemoveStage,
    },
    /// Update check for a package.
    UpdateCheck {
        pkg_name: String,
        pkg_id: String,
        status: UpdateCheckStatus,
    },
    /// Old version cleanup after update.
    UpdateCleanup {
        op_id: OperationId,
        pkg_name: String,
        pkg_id: String,
        old_version: String,
        stage: UpdateCleanupStage,
    },
    /// Hook execution event.
    Hook {
        op_id: OperationId,
        pkg_name: String,
        pkg_id: String,
        hook_name: String,
        stage: HookStage,
    },
    /// Package execution (run command).
    Running {
        op_id: OperationId,
        pkg_name: String,
        pkg_id: String,
        stage: RunStage,
    },
    /// Build stage (for source packages).
    Building {
        op_id: OperationId,
        pkg_name: String,
        pkg_id: String,
        stage: BuildStage,
    },
    /// Operation completed successfully.
    OperationComplete {
        op_id: OperationId,
        pkg_name: String,
        pkg_id: String,
    },
    /// Operation failed.
    OperationFailed {
        op_id: OperationId,
        pkg_name: String,
        pkg_id: String,
        error: String,
    },
    /// Repository sync progress.
    SyncProgress { repo_name: String, stage: SyncStage },
    /// Batch operation overall progress.
    BatchProgress {
        completed: u32,
        total: u32,
        failed: u32,
    },
    /// Log message.
    Log { level: LogLevel, message: String },
}

/// Verification stages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyStage {
    /// Calculating and verifying checksum (blake3).
    Checksum,
    /// Verifying signature with repository public key.
    Signature,
    /// All verification passed.
    Passed,
    /// Verification failed.
    Failed(String),
}

/// Installation stages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallStage {
    /// Extracting package archive.
    Extracting,
    /// Extracting a nested archive within the package.
    ExtractingNested,
    /// Creating binary symlinks in bin directory.
    LinkingBinaries,
    /// Integrating desktop files, icons, and appstream metadata.
    DesktopIntegration,
    /// Setting up portable directories.
    SetupPortable,
    /// Recording installation metadata to database.
    RecordingDatabase,
    /// Running a hook (post_download, post_extract, post_install).
    RunningHook(String),
    /// Installation complete.
    Complete,
}

/// Package removal stages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoveStage {
    /// Running pre-remove hook.
    RunningHook(String),
    /// Removing binary symlinks from bin directory.
    UnlinkingBinaries,
    /// Removing desktop file symlinks.
    UnlinkingDesktop,
    /// Removing icon symlinks.
    UnlinkingIcons,
    /// Deleting the package directory.
    RemovingDirectory,
    /// Cleaning up database records.
    CleaningDatabase,
    /// Removal complete.
    Complete { size_freed: Option<u64> },
}

/// Repository sync stages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncStage {
    /// Fetching metadata from remote.
    Fetching,
    /// Repository metadata is already up to date (304 Not Modified).
    UpToDate,
    /// Decompressing metadata (zstd).
    Decompressing,
    /// Writing metadata to local database.
    WritingDatabase,
    /// Validating metadata signature.
    Validating,
    /// Sync complete.
    Complete { package_count: Option<u64> },
}

/// Update check result for a single package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateCheckStatus {
    /// A newer version is available.
    Available {
        current_version: String,
        new_version: String,
    },
    /// Already up to date.
    UpToDate { version: String },
    /// Skipped (pinned, no update source, etc.).
    Skipped { reason: String },
}

/// Old version cleanup stages after update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateCleanupStage {
    /// Removing the old version.
    Removing,
    /// Old version cleanup complete.
    Complete { size_freed: Option<u64> },
    /// Old version kept.
    Kept,
}

/// Hook execution stages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookStage {
    /// Hook is starting.
    Starting,
    /// Hook completed successfully.
    Complete,
    /// Hook failed.
    Failed { exit_code: Option<i32> },
}

/// Package execution stages (run command).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunStage {
    /// Using a cached binary (already downloaded).
    CacheHit,
    /// Binary not cached, downloading.
    Downloading,
    /// Running the binary.
    Executing,
    /// Execution finished.
    Complete { exit_code: i32 },
}

/// Build stages (for source packages).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildStage {
    /// Running build command N of M.
    Running {
        command_index: usize,
        total_commands: usize,
    },
    /// Build command completed.
    CommandComplete { command_index: usize },
    /// Activating sandbox for build.
    Sandboxing,
}

/// Log levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warning,
    Error,
}
