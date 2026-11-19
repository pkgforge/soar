//! What a pressed key means.

use crate::views::ViewId;

/// One thing a key can ask the interface to do.
///
/// Every variant corresponds to one row of the keymap tables in
/// [`crate::keymap`]. View-local actions reach the active view once the views
/// hold their own state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    /// Shows one of the seven views.
    View(ViewId),
    /// Moves to the next view, wrapping around.
    NextView,
    /// Moves to the previous view, wrapping around.
    PrevView,
    /// Opens the help overlay.
    Help,
    /// Quits the interface.
    Quit,
    /// Reloads the active view's data.
    Reload,
    /// Moves the cursor one row down.
    Down,
    /// Moves the cursor one row up.
    Up,
    /// Moves the cursor to the first row.
    Top,
    /// Moves the cursor to the last row.
    Bottom,
    /// Moves the cursor half a page down.
    PageDown,
    /// Moves the cursor half a page up.
    PageUp,
    /// Clears the filter, then clears the marks.
    Escape,
    /// Edits the view's filter.
    Filter,
    /// Marks or unmarks the current row.
    ToggleMark,
    /// Marks every row, or clears all marks.
    MarkAll,
    /// Installs the marked packages, or the current row.
    Install,
    /// Installs through the install options dialog.
    InstallWithOptions,
    /// Picks a version of the package, then installs it.
    PickVersion,
    /// Runs the package without installing it.
    RunPackage,
    /// Installs from a name, URL, or path.
    InstallByQuery,
    /// Cycles the repository filter.
    CycleRepo,
    /// Opens or closes the full-screen detail.
    ToggleDetail,
    /// Removes the marked packages, or the current row.
    Remove,
    /// Switches the current package to another variant.
    SwitchVariant,
    /// Checks updates for the marked packages, or the current row.
    UpdateSelected,
    /// Cycles the sort order.
    CycleSort,
    /// Updates the marked packages, or all when none are marked.
    Update,
    /// Toggles keeping old versions during an update.
    ToggleKeepOld,
    /// Shows or hides the skipped rows.
    ToggleSkipped,
    /// Applies the changes packages.toml describes.
    Apply,
    /// Toggles pruning packages missing from packages.toml.
    TogglePrune,
    /// Opens packages.toml in an editor.
    EditPackages,
    /// Creates the default packages.toml when it is missing.
    CreatePackages,
    /// Syncs every repository.
    Sync,
    /// Enables or disables the current repository.
    ToggleEnabled,
    /// Adds a repository.
    AddRepo,
    /// Removes the current repository.
    RemoveRepo,
    /// Opens config.toml in an editor.
    EditConfig,
    /// Cleans the package cache.
    CleanCache,
    /// Fixes broken symlinks.
    FixSymlinks,
    /// Removes broken packages.
    RemoveBroken,
    /// Focuses the detail pane of the current job.
    FocusDetail,
    /// Focuses the log pane.
    FocusLog,
    /// Removes finished jobs from the list.
    ClearFinished,
    /// Asks the running job to stop, with a confirm first.
    AbortJob,
    /// Aborts or drops the job the Jobs view points at.
    AbortSelected,
}
