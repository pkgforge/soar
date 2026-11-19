//! Work the interface's state asks the runner to carry out.

use std::path::PathBuf;

use soar_operations::privilege::EscalationTool;

use crate::{
    jobs::{InstallChoices, JobId, JobSpec},
    privileged::Secret,
    views::browse::PackageKey,
};

/// A request for work outside the state, such as talking to soar.
pub(crate) enum Effect {
    /// Rebuilds the soar context from the current configuration.
    ///
    /// Produced once configuration changes through the interface, such as
    /// after a repository sync replaces the metadata databases.
    RebuildContext,
    /// Runs the queued job through the runner.
    StartJob {
        /// The job to mark started.
        id: JobId,
        /// What the job runs.
        spec: JobSpec,
    },
    /// Stops the job running now at its next safe point.
    AbortJob,
    /// Loads the package catalog the Browse view shows.
    LoadCatalog {
        /// Which load this is: every start numbers itself, and a reply
        /// from a load a newer one replaced is dropped, so only the
        /// newest load fills the catalog.
        check: u32,
    },
    /// Loads the installed packages the Installed view shows. The listing
    /// walks the disk for sizes, so the runner runs it off the async
    /// workers.
    LoadInstalled {
        /// Which load this is: every start numbers itself, and a reply
        /// from a load a newer one replaced is dropped, so only the
        /// newest load fills the list.
        check: u32,
    },
    /// Loads the configured repositories the Repos view shows.
    LoadRepos,
    /// Loads the installation health report and the paths it names, for
    /// the Health view. The check walks the filesystem, so the runner runs
    /// it off the async workers.
    LoadHealth,
    /// Computes what applying the packages file would change, for the
    /// Apply view. Remote declarations are resolved over the network, so
    /// the runner runs it on the async workers.
    ComputeApplyDiff {
        /// Whether applying also removes packages the file does not
        /// declare.
        prune: bool,
        /// Which computation this is: every start numbers itself, and a
        /// result from an older one is dropped, so only the newest
        /// computation fills the view.
        check: u32,
    },
    /// Loads the installed variants of one package name, for the flow
    /// that chooses which one is linked. The listing queries the
    /// metadata database, so the runner runs it off the async workers.
    LoadVariants {
        /// The package name whose variants load.
        name: String,
    },
    /// Loads one package's full metadata for the detail pane.
    ///
    /// The version rides along because the key alone cannot pick it: the
    /// catalog row knows which variant the pane should show, and the
    /// query needs that version to fetch exactly it.
    LoadDetail {
        /// The package to load.
        key: PackageKey,
        /// The version the catalog row shows.
        version: String,
    },
    /// Resolves install queries into targets, variants to pick, and
    /// not-found reports, without installing anything yet.
    Resolve {
        /// The queries to resolve, each a name, an exact
        /// `family/name@version:repo`, a URL, or a local path.
        queries: Vec<String>,
        /// The options the install runs with once it is confirmed.
        choices: InstallChoices,
    },
    /// Prepares a package run without installing it: resolves the query,
    /// downloads the binary when the cache lacks it, and reports the path
    /// to execute or the variants to pick between.
    PrepareRun {
        /// The query naming the package, either `family/name:repo` for
        /// its newest variant or an exact `family/name@version:repo`.
        query: String,
        /// The repository to search, when the query does not name one.
        repo: Option<String>,
        /// The package id pinning the variant, so a query picked out of
        /// an ambiguity cannot turn ambiguous again.
        pkg_id: Option<String>,
        /// The arguments the binary runs with.
        args: Vec<String>,
    },
    /// Checks the installed packages for newer versions, limited to the
    /// named packages when some are given. The check reports its progress
    /// through `UpdateCheck` events and its result through
    /// `Loaded::Updates`, tagged with the check it answers.
    CheckUpdates {
        /// The packages the check is limited to, or every installed one.
        names: Option<Vec<String>>,
        /// Which check this is: every start numbers itself, and a result
        /// from an older number is dropped, so only the newest check
        /// fills the list.
        check: u32,
    },
    /// Hands the terminal to a program and takes it back after it exits.
    Suspend(Suspended),
    /// Checks how the administrator pass could run here: already root, a
    /// password question, a terminal question, or no way to get root. The
    /// answer arrives as `Loaded::Escalation`.
    ProbeEscalation,
    /// Sends the password the dialog collected to sudo, which caches the
    /// credentials for the system pass that follows. The answer arrives
    /// as `Loaded::Authenticated`. The `check` number says which
    /// password check is asking; an answer with a number the app no
    /// longer expects is stale.
    Authenticate {
        /// The password the dialog collected, moved out of the input so
        /// it never lingers in the interface's state.
        secret: Secret,
        /// Which password check this is: every submission numbers
        /// itself, and an answer from an older one is dropped.
        check: u32,
    },
}

/// Something that runs with the terminal handed back to the user, while
/// the interface's input is paused.
pub(crate) enum Suspended {
    /// Runs a package's cached binary with the arguments the run prompt
    /// collected.
    Exec {
        /// The binary to execute.
        path: PathBuf,
        /// The arguments handed to the binary.
        args: Vec<String>,
    },
    /// Opens the configuration file in the user's editor.
    Edit(PathBuf),
    /// Runs the system pass with the terminal handed over, so the
    /// escalation tool can ask for the password itself. The job named by
    /// `id` is already marked running; the outcome comes back through the
    /// queue when the pass is over.
    SystemPass {
        /// The job whose outcome reports the pass.
        id: JobId,
        /// The escalation tool that asks for the password.
        tool: EscalationTool,
        /// Where the declared packages are written.
        packages_path: String,
        /// The system names the confirm named. The runner reads the file
        /// again before spawning the child, so an edit after the
        /// confirmation cannot apply unapproved packages.
        names: Vec<String>,
    },
}
