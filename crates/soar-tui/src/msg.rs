//! Everything that can arrive while the interface runs.

use std::path::PathBuf;

use ratatui::crossterm::event::Event;
use soar_core::{database::models::Package, package::install::InstallTarget};
use soar_events::SoarEvent;
use soar_operations::{
    HealthReport, InstalledListResult, PrepareRunResult, UpdateInfo, VariantInfo,
};

use crate::{
    jobs::{InstallChoices, JobId, JobOutcome},
    privileged::EscalationProbe,
    views::{
        apply::ApplyPlan,
        browse::{CatalogRow, PackageKey},
        repos::RepoRow,
    },
};

/// One thing the interface's state can react to.
pub(crate) enum Msg {
    /// A terminal event, such as a key press or a resize.
    Input(Event),
    /// The periodic tick that drives animation.
    Tick,
    /// Asks for one repaint after the interface has taken the terminal
    /// back from a suspended program.
    Redraw,
    /// Reports the file the user finished editing while the interface was
    /// suspended, so the views that depend on it can reload.
    Edited(PathBuf),
    /// Reports that reading the terminal failed, with why, so the
    /// interface can quit instead of running deaf.
    InputFailed(String),
    /// An event from a running soar operation, feeding the progress of the
    /// running job or of background work.
    Soar(SoarEvent),
    /// One formatted log line from the interface's subscriber.
    Output(String),
    /// Read-only background work finished, with its result.
    Loaded(Loaded),
    /// A job finished, with what it reports.
    JobFinished {
        /// The job that finished.
        id: JobId,
        /// What the job reports.
        outcome: JobOutcome,
    },
}

/// Results of read-only background work the runner performed.
pub(crate) enum Loaded {
    /// The package catalog, one row per package, or why it could not
    /// load. The `check` number says which load is answering; a number
    /// the app no longer expects is stale, because a newer load replaced
    /// it while both ran.
    Catalog {
        /// The number the load was started with.
        check: u32,
        /// The catalog rows, or why the load failed.
        result: Result<Vec<CatalogRow>, String>,
    },
    /// The installed packages, with their sizes and health, or why the
    /// listing could not load. The `check` number says which load is
    /// answering; a number the app no longer expects is stale.
    Installed {
        /// The number the load was started with.
        check: u32,
        /// The installed listing, or why the load failed.
        result: Result<InstalledListResult, String>,
    },
    /// The installed packages with a newer version available, or why the
    /// check could not run. The `check` number says which check is
    /// answering; a number the view no longer expects is stale.
    Updates {
        /// The number the check was started with.
        check: u32,
        /// What the check found.
        result: Result<Vec<UpdateInfo>, String>,
    },
    /// What applying the packages file would change, or why the plan
    /// could not be computed. The `check` number says which computation
    /// is answering; a number the view no longer expects is stale.
    ApplyDiff {
        /// The number the computation was started with.
        check: u32,
        /// The computed plan, or why it failed.
        result: Result<ApplyPlan, String>,
    },
    /// The configured repositories, one row each, or why the load could
    /// not run.
    Repos(Result<Vec<RepoRow>, String>),
    /// The installation health report with the paths soar uses, or why
    /// the check could not run.
    Health(Result<(HealthReport, Vec<(String, PathBuf)>), String>),
    /// The installed variants of one package name, or why the listing
    /// could not load.
    Variants {
        /// The package name the variants belong to.
        name: String,
        /// The variants, or why they could not load.
        result: Box<Result<Vec<VariantInfo>, String>>,
    },
    /// One package's full metadata for the detail pane. The result stays
    /// `None` when the repositories hold no package with that query. It
    /// is boxed because the metadata alone outweighs every other message.
    Detail {
        /// The package the details are for.
        key: PackageKey,
        /// The metadata, or why it could not load.
        result: Box<Result<Option<Package>, String>>,
    },
    /// The outcome of resolving install queries, with the options the
    /// install runs with once it is confirmed.
    Resolved {
        /// The options gathered before the queries were sent.
        choices: InstallChoices,
        /// One outcome per query, in query order, or why the resolve
        /// failed.
        result: Result<Vec<Resolution>, String>,
    },
    /// The outcome of preparing a package run, with the arguments the
    /// run prompt collected. The result is boxed because an ambiguous
    /// query can carry a large list of packages.
    RunReady {
        /// The arguments the binary runs with.
        args: Vec<String>,
        /// The path to execute, the variants to pick between, or why the
        /// preparation failed.
        result: Box<Result<PrepareRunResult, String>>,
    },
    /// What the escalation probe learned about running the system pass.
    Escalation(EscalationProbe),
    /// The answer of the password check `check` started by
    /// `Effect::Authenticate`, or why it could not run.
    Authenticated {
        /// Which password check this answers: every submission numbers
        /// itself, and an answer from an older one is dropped.
        check: u32,
        /// Whether sudo took the password, or why it refused it.
        result: Result<(), String>,
    },
}

/// The outcome of resolving one install query.
pub(crate) enum Resolution {
    /// The query resolved into install plans, ready to run.
    Ready(Vec<InstallTarget>),
    /// Several packages match the query; one has to be picked.
    Ambiguous {
        /// The query that matched more than one package.
        query: String,
        /// The packages the query matched.
        candidates: Vec<Package>,
    },
    /// Nothing matches the query, with close names to suggest.
    NotFound {
        /// The query nothing matched.
        query: String,
        /// Similar names worth trying instead.
        suggestions: Vec<String>,
    },
    /// A version of the package is installed already.
    AlreadyInstalled {
        /// The installed package's name.
        name: String,
        /// The repository the installed copy came from.
        #[allow(dead_code)]
        repo: String,
        /// The version of the installed copy.
        #[allow(dead_code)]
        version: String,
    },
}
