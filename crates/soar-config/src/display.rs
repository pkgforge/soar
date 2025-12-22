use documented::{Documented, DocumentedFields};
use serde::{Deserialize, Serialize};

/// Display settings for CLI output formatting
#[derive(Clone, Debug, Default, Deserialize, Serialize, Documented, DocumentedFields)]
pub struct DisplaySettings {
    /// Progress bar style: "classic", "modern", or "minimal"
    /// Default: "modern"
    pub progress_style: Option<ProgressStyle>,

    /// Show unicode icons/symbols in output
    /// Default: true
    pub icons: Option<bool>,

    /// Show spinners for async operations
    /// Default: true
    pub spinners: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProgressStyle {
    /// Classic ASCII progress bar (=>)
    Classic,
    /// Modern unicode progress bar with spinner and ETA
    #[default]
    Modern,
    /// Minimal percentage-only display
    Minimal,
}

impl DisplaySettings {
    pub fn progress_style(&self) -> ProgressStyle {
        self.progress_style.clone().unwrap_or_default()
    }

    pub fn icons(&self) -> bool {
        self.icons.unwrap_or(true)
    }

    pub fn spinners(&self) -> bool {
        self.spinners.unwrap_or(true)
    }
}
