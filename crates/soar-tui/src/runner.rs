//! Carries out the work the interface's state asks for.

use std::{
    collections::VecDeque,
    ffi::OsString,
    fs,
    future::Future,
    io::{BufRead as _, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        mpsc::{Receiver, RecvTimeoutError, Sender},
    },
    time::{Duration, Instant},
};

use soar_config::{
    config::{CONFIG_PATH, Config, get_config, init, is_system_mode},
    error::ConfigError,
    packages::{PACKAGES_CONFIG_PATH, PackagesConfig},
    repository::Repository,
};
use soar_core::{SoarResult, error::SoarError};
use soar_events::{EventSinkHandle, SoarEvent};
use soar_operations::{
    AmbiguousPackage, ApplyReport, InstallOptions, InstallReport, PackageListEntry, RemoveReport,
    ResolveResult, SoarContext, UpdateReport, apply, health, install, list,
    privilege::{ESCALATION_DIRS, EscalationTool, escalated_command, find_escalation_tool},
    remove,
    repo::RepoUpdate,
    run, search, switch, update,
};
use soar_utils::cancel::CancelToken;
use tokio::runtime::Handle;

use crate::{
    effect::{Effect, Suspended},
    event_loop::InputThread,
    jobs::{JobId, JobOutcome, JobSpec, OutcomeLine, RepoChange, SystemMode},
    msg::{Loaded, Msg, Resolution},
    privileged::{self, EscalationProbe},
    sink::TuiSink,
    terminal::{self, Session},
    views::{
        apply::ApplyPlan,
        browse::{CatalogRow, PackageKey},
        repos::RepoRow,
    },
};

/// How many lines of the system pass's stderr the report keeps.
const KEPT_STDERR_LINES: usize = 20;

/// How often the system pass's output loop checks whether the child
/// exited.
const PIPE_POLL: Duration = Duration::from_millis(50);

/// How long the output loop keeps reading after the child exited, so a
/// grandchild holding the pipes cannot hold the job hostage.
const PIPE_GRACE: Duration = Duration::from_millis(300);

/// One line the system pass wrote, with the pipe it came from.
enum PipeLine {
    /// A line of stdout, which holds the JSON event stream.
    Out(String),
    /// A line of stderr, which holds the child's own logging.
    Err(String),
}

/// Executes effects. It is the only part of the interface that calls into
/// soar's operations.
pub(crate) struct Runner {
    /// The context operations run through.
    ctx: SoarContext,
    /// Where operations report their progress.
    sink: EventSinkHandle,
    /// The runtime asynchronous work is spawned on.
    handle: Handle,
    /// Channel the results of spawned asynchronous work arrive through.
    tx: Sender<Msg>,
    /// Global options a privileged soar child runs with, so it reads the
    /// same configuration and connections this session was given.
    forward_args: Vec<OsString>,
    /// The token that asks the job running now to stop.
    current_cancel: Option<CancelToken>,
}

impl Runner {
    /// Creates the runner whose operations report into the interface's
    /// message channel.
    pub(crate) fn new(handle: Handle, tx: Sender<Msg>, forward_args: Vec<OsString>) -> Runner {
        let sink: EventSinkHandle = Arc::new(TuiSink::new(tx.clone()));
        let ctx = SoarContext::new(get_config(), Arc::clone(&sink));
        Runner {
            ctx,
            sink,
            handle,
            tx,
            forward_args,
            current_cancel: None,
        }
    }

    /// Performs one piece of requested work.
    ///
    /// A suspended effect hands the terminal to another program: the
    /// input thread stops reading keystrokes and the session gives the
    /// screen back until the program exits.
    pub(crate) fn execute(
        &mut self,
        effect: Effect,
        session: &mut Session,
        input: &InputThread,
    ) -> std::io::Result<()> {
        match effect {
            Effect::RebuildContext => {
                if let Err(err) = init() {
                    tracing::error!("reloading config: {err}");
                } else {
                    self.ctx = SoarContext::new(get_config(), Arc::clone(&self.sink));
                }
            }
            Effect::StartJob {
                id,
                spec,
            } => {
                let cancel = CancelToken::new();
                self.current_cancel = Some(cancel.clone());
                let ctx = self.ctx.with_cancel(cancel);
                let forward_args = self.forward_args.clone();
                let tx = self.tx.clone();
                self.spawn(async move {
                    Msg::JobFinished {
                        id,
                        outcome: run_job(ctx, spec, &forward_args, tx).await,
                    }
                });
            }
            Effect::AbortJob => {
                if let Some(cancel) = &self.current_cancel {
                    cancel.cancel();
                }
            }
            Effect::LoadCatalog => {
                let ctx = self.ctx.clone();
                self.spawn(async move {
                    let result = match list::list_packages(&ctx, None).await {
                        Ok(listing) => Ok(listing.packages.into_iter().map(catalog_row).collect()),
                        Err(err) => Err(err.to_string()),
                    };
                    Msg::Loaded(Loaded::Catalog(result))
                });
            }
            Effect::LoadInstalled => {
                let ctx = self.ctx.clone();
                self.spawn(async move {
                    let result = tokio::task::spawn_blocking(move || {
                        list::list_installed(&ctx, None).map_err(|err| err.to_string())
                    })
                    .await
                    .unwrap_or_else(|err| Err(err.to_string()));
                    Msg::Loaded(Loaded::Installed(result))
                });
            }
            Effect::LoadRepos => {
                let config = self.ctx.config().clone();
                let repositories = config.repositories.clone();
                self.spawn(async move {
                    Msg::Loaded(Loaded::Repos(Ok(repo_rows(&config, &repositories))))
                });
            }
            Effect::LoadHealth => {
                let ctx = self.ctx.clone();
                let config = self.ctx.config().clone();
                self.spawn(async move {
                    let result = tokio::task::spawn_blocking(move || {
                        let paths = health_paths(&config).map_err(|err| err.to_string())?;
                        let report = health::check_health(&ctx).map_err(|err| err.to_string())?;
                        Ok((report, paths))
                    })
                    .await
                    .unwrap_or_else(|err| Err(err.to_string()));
                    Msg::Loaded(Loaded::Health(result))
                });
            }
            Effect::ComputeApplyDiff {
                prune,
            } => {
                let ctx = self.ctx.clone();
                self.spawn(async move {
                    let result = compute_apply_plan(&ctx, prune).await;
                    Msg::Loaded(Loaded::ApplyDiff(result))
                });
            }
            Effect::LoadVariants {
                name,
            } => {
                let ctx = self.ctx.clone();
                let query = name.clone();
                self.spawn(async move {
                    let result = tokio::task::spawn_blocking(move || {
                        switch::list_variants(&ctx, &query).map_err(|err| err.to_string())
                    })
                    .await
                    .unwrap_or_else(|err| Err(err.to_string()));
                    Msg::Loaded(Loaded::Variants {
                        name,
                        result: Box::new(result),
                    })
                });
            }
            Effect::LoadDetail {
                key,
                version,
            } => {
                let ctx = self.ctx.clone();
                self.spawn(async move {
                    let query = match key.pkg_family.as_deref() {
                        Some(family) => format!("{family}/{}:{}", key.pkg_name, key.repo_name),
                        None => format!("{}:{}", key.pkg_name, key.repo_name),
                    };
                    let result = match search::query_package(&ctx, &query).await {
                        Ok(packages) => {
                            Ok(packages
                                .iter()
                                .find(|package| package.version == version)
                                .cloned()
                                .or_else(|| packages.first().cloned()))
                        }
                        Err(err) => Err(err.to_string()),
                    };
                    Msg::Loaded(Loaded::Detail {
                        key,
                        result: Box::new(result),
                    })
                });
            }
            Effect::Resolve {
                queries,
                choices,
            } => {
                let ctx = self.ctx.clone();
                let options = choices.options();
                self.spawn(async move {
                    let result = resolve_queries(&ctx, &queries, &options).await;
                    Msg::Loaded(Loaded::Resolved {
                        choices,
                        result,
                    })
                });
            }
            Effect::PrepareRun {
                query,
                repo,
                pkg_id,
                args,
            } => {
                let ctx = self.ctx.clone();
                self.spawn(async move {
                    let result =
                        run::prepare_run(&ctx, &query, repo.as_deref(), pkg_id.as_deref(), false)
                            .await
                            .map_err(|err| err.to_string());
                    Msg::Loaded(Loaded::RunReady {
                        args,
                        result: Box::new(result),
                    })
                });
            }
            Effect::CheckUpdates {
                names,
                check,
            } => {
                let ctx = self.ctx.clone();
                self.spawn(async move {
                    let result = update::check_updates(&ctx, names.as_deref())
                        .await
                        .map_err(|err| err.to_string());
                    Msg::Loaded(Loaded::Updates {
                        check,
                        result,
                    })
                });
            }
            Effect::Suspend(suspended) => {
                input.pause();
                if let Err(err) = session.suspend() {
                    input.resume();
                    return Err(err);
                }
                let mut edited: Option<PathBuf> = None;
                let mut finished: Option<(JobId, JobOutcome)> = None;
                match suspended {
                    Suspended::Exec {
                        path,
                        args,
                    } => {
                        match run::execute_binary(&path, &args) {
                            Ok(result) => {
                                println!(
                                    "\n{} exited with code {}",
                                    path.display(),
                                    result.exit_code
                                )
                            }
                            Err(err) => eprintln!("\n{err}"),
                        }
                        terminal::wait_for_enter();
                    }
                    Suspended::Edit(path) => {
                        // The editor already held the screen while it ran,
                        // so only a failed one pauses for Enter on the way
                        // back into the interface.
                        if let Err(err) = open_in_editor(&path) {
                            eprintln!("\n{err}");
                            terminal::wait_for_enter();
                        }
                        edited = Some(path);
                    }
                    Suspended::SystemPass {
                        id,
                        tool,
                        packages_path,
                    } => {
                        println!(
                            "\nApplying system packages. {} may ask for your password.",
                            tool_name(&tool)
                        );
                        let outcome =
                            run_system_terminal(&self.forward_args, &tool, &packages_path);
                        terminal::wait_for_enter();
                        finished = Some((id, outcome));
                    }
                }
                // The keystrokes come back whatever restoring does, so a
                // failed restore cannot leave the interface deaf.
                let resumed = session.resume();
                input.resume();
                resumed?;
                if let Some((id, outcome)) = finished {
                    let _ = self.tx.send(Msg::JobFinished {
                        id,
                        outcome,
                    });
                }
                if let Some(path) = edited {
                    let _ = self.tx.send(Msg::Edited(path));
                }
                let _ = self.tx.send(Msg::Redraw);
            }
            Effect::ProbeEscalation => {
                self.spawn(async move {
                    let probe = tokio::task::spawn_blocking(privileged::probe_escalation)
                        .await
                        .unwrap_or_else(|err| EscalationProbe::Unavailable(err.to_string()));
                    Msg::Loaded(Loaded::Escalation(probe))
                });
            }
            Effect::Authenticate {
                secret,
                check,
            } => {
                self.spawn(async move {
                    let result =
                        tokio::task::spawn_blocking(move || privileged::authenticate(secret))
                            .await
                            .unwrap_or_else(|err| Err(err.to_string()));
                    Msg::Loaded(Loaded::Authenticated {
                        check,
                        result,
                    })
                });
            }
        }
        Ok(())
    }

    /// Spawns asynchronous work on the runtime and sends its message back
    /// through the channel when it is done.
    fn spawn(&self, fut: impl Future<Output = Msg> + Send + 'static) {
        let tx = self.tx.clone();
        self.handle.spawn(async move {
            let msg = fut.await;
            let _ = tx.send(msg);
        });
    }
}

/// Runs the user's editor on `path` through the shell, so editors whose
/// command carries arguments of its own, such as `code --wait`, work. The
/// path is passed as an argument rather than spliced into the command line.
fn open_in_editor(path: &Path) -> Result<(), String> {
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
    let status = Command::new("sh")
        .arg("-c")
        .arg(format!("{editor} \"$1\""))
        .arg("sh")
        .arg(path)
        .status()
        .map_err(|err| format!("executing command {editor} {}: {err}", path.display()))?;
    if !status.success() {
        return Err(format!(
            "{editor} exited with {status} while editing {}",
            path.display()
        ));
    }
    Ok(())
}

/// Turns the configured repositories into the rows the repositories view
/// keeps. Each row reads the repository's data directory once, so a load
/// is a handful of filesystem stats.
fn repo_rows(config: &Config, repositories: &[Repository]) -> Vec<RepoRow> {
    repositories
        .iter()
        .map(|repo| {
            RepoRow {
                name: repo.name.clone(),
                url: repo.url.clone(),
                enabled: repo.is_enabled(),
                signed: repo.signature_verification(),
                desktop: config.has_desktop_integration(&repo.name),
                sync_interval: repo
                    .sync_interval
                    .clone()
                    .unwrap_or_else(|| "3h".to_string()),
                synced: repo
                    .get_path()
                    .ok()
                    .and_then(|path| fs::metadata(path.join("metadata.db")).ok())
                    .and_then(|metadata| metadata.modified().ok()),
            }
        })
        .collect()
}

/// Builds the plan the apply view shows: the diff of the user's declared
/// packages against what is installed, plus the names left for the system
/// pass. A missing packages file is not an error; it plans an empty
/// diff so the view can offer to create the file.
async fn compute_apply_plan(ctx: &SoarContext, prune: bool) -> Result<ApplyPlan, String> {
    let path = PackagesConfig::resolve_path(None);
    let config = match PackagesConfig::load(None) {
        Ok(config) => config,
        Err(ConfigError::PackagesConfigNotFound(_)) => {
            return Ok(ApplyPlan {
                path,
                diff: None,
                system: Vec::new(),
            });
        }
        Err(err) => return Err(err.to_string()),
    };
    let declared = config.resolved_packages();
    let (resolved, system): (Vec<_>, Vec<_>) = if is_system_mode() {
        (declared, Vec::new())
    } else {
        declared.into_iter().partition(|pkg| !pkg.system)
    };
    let diff = apply::compute_diff(ctx, &resolved, prune)
        .await
        .map_err(|err| err.to_string())?;
    Ok(ApplyPlan {
        path,
        diff: Some(diff),
        system: system.into_iter().map(|pkg| pkg.name).collect(),
    })
}

/// The paths soar uses, under the labels the health view shows them
/// with. The two configuration file paths come from their shared slots;
/// the rest come from the session's configuration, so one of them failing
/// fails the whole load.
fn health_paths(config: &Config) -> Result<Vec<(String, PathBuf)>, String> {
    let bin = config.get_bin_path().map_err(|err| err.to_string())?;
    let db = config.get_db_path().map_err(|err| err.to_string())?;
    let cache = config.get_cache_path().map_err(|err| err.to_string())?;
    let packages = config
        .get_packages_path(None)
        .map_err(|err| err.to_string())?;
    let repositories = config
        .get_repositories_path()
        .map_err(|err| err.to_string())?;
    Ok(vec![
        (
            "Config".to_string(),
            CONFIG_PATH
                .read()
                .map(|path| path.to_path_buf())
                .unwrap_or_default(),
        ),
        (
            "Packages config".to_string(),
            PACKAGES_CONFIG_PATH
                .read()
                .map(|path| path.to_path_buf())
                .unwrap_or_default(),
        ),
        ("Bin".to_string(), bin),
        ("Database".to_string(), db),
        ("Cache".to_string(), cache),
        ("Packages".to_string(), packages),
        ("Repositories".to_string(), repositories),
    ])
}

/// Turns one listing entry into the row the catalog view keeps.
fn catalog_row(entry: PackageListEntry) -> CatalogRow {
    let package = entry.package;
    let haystack = match package.pkg_family.as_deref() {
        Some(family) => format!("{family}/{}", package.pkg_name),
        None => package.pkg_name.clone(),
    }
    .to_lowercase();
    CatalogRow {
        key: PackageKey {
            repo_name: package.repo_name,
            pkg_name: package.pkg_name,
            pkg_family: package.pkg_family,
            pkg_id: package.pkg_id,
        },
        version: package.version,
        pkg_type: package.pkg_type,
        description: package.description,
        installed: entry.installed,
        other_versions: entry.other_versions,
        haystack,
    }
}

/// Runs one job and reports what it did. Each arm hands the spec to the
/// operation it names and turns the operation's report into an outcome.
/// The system pass in direct mode is the exception: it runs as another
/// process through the escalation tool, and its progress reaches the
/// interface through the child's own event stream.
async fn run_job(
    ctx: SoarContext,
    spec: JobSpec,
    forward_args: &[OsString],
    tx: Sender<Msg>,
) -> JobOutcome {
    let title = spec.title();
    match spec {
        JobSpec::Install {
            mut targets,
            picked,
            options,
        } => {
            let options = options.options();
            for package in picked {
                match install::target_for(&ctx, package, &options) {
                    Ok(target) => targets.push(target),
                    Err(err) => return failed_outcome(&title, err),
                }
            }
            match install::perform_installation(&ctx, targets, &options).await {
                Ok(report) => {
                    let installed = report.installed.len();
                    let skipped = report.cancelled.len();
                    let mut outcome = install_outcome(report);
                    if ctx.cancel_token().is_cancelled() {
                        outcome.aborted = true;
                        outcome.headline =
                            format!("Aborted: {installed} installed, {skipped} skipped");
                    }
                    outcome
                }
                Err(err) => failed_outcome(&title, err),
            }
        }
        JobSpec::Remove {
            packages,
        } => {
            match remove::perform_removal(&ctx, packages).await {
                Ok(report) => {
                    let removed = report.removed.len();
                    let skipped = report.cancelled.len();
                    let mut outcome = remove_outcome(report);
                    if ctx.cancel_token().is_cancelled() {
                        outcome.aborted = true;
                        outcome.headline = format!("Aborted: {removed} removed, {skipped} skipped");
                    }
                    outcome
                }
                Err(err) => failed_outcome(&title, err),
            }
        }
        JobSpec::Update {
            updates,
            keep_old,
        } => {
            match update::perform_update(&ctx, updates, keep_old, false).await {
                Ok(report) => {
                    let updated = report.updated.len();
                    let skipped = report.cancelled.len();
                    let mut outcome = update_outcome(report);
                    if ctx.cancel_token().is_cancelled() {
                        outcome.aborted = true;
                        outcome.headline = format!("Aborted: {updated} updated, {skipped} skipped");
                    }
                    outcome
                }
                Err(err) => failed_outcome(&title, err),
            }
        }
        JobSpec::Switch {
            id,
            name,
        } => {
            match switch::switch_variant(&ctx, id).await {
                Ok(()) => ok_outcome(format!("Switched to {name}")),
                Err(err) => failed_outcome(&title, err),
            }
        }
        JobSpec::Sync => {
            match ctx.sync().await {
                Ok(()) => ok_outcome("Repositories synced".to_string()),
                Err(err) => failed_outcome(&title, err),
            }
        }
        JobSpec::Apply {
            diff,
            packages_path,
        } => {
            match apply::execute_apply(&ctx, diff, false, &packages_path).await {
                Ok(report) => {
                    let mut outcome = apply_outcome(report);
                    if ctx.cancel_token().is_cancelled() {
                        outcome.aborted = true;
                    }
                    outcome
                }
                Err(err) => failed_outcome(&title, err),
            }
        }
        // The direct mode reaches this arm only through the authorization
        // flow; the other modes never run here, since the interface asks
        // first and a terminal pass suspends the interface itself.
        JobSpec::ApplySystem {
            names: _,
            packages_path,
            mode: SystemMode::Direct,
        } => {
            let forward = forward_args.to_vec();
            let pass_tx = tx.clone();
            match tokio::task::spawn_blocking(move || {
                run_system_direct(packages_path, forward, pass_tx)
            })
            .await
            {
                Ok(outcome) => outcome,
                Err(err) => system_failed(err.to_string()),
            }
        }
        JobSpec::ApplySystem {
            ..
        } => {
            failed_outcome(
                &title,
                SoarError::Custom("System apply is not ready".into()),
            )
        }
        JobSpec::CleanCache => {
            match blocking(soar_core::utils::cleanup_cache).await {
                Ok(()) => ok_outcome("Cache cleaned".to_string()),
                Err(err) => failed_outcome(&title, err),
            }
        }
        JobSpec::RemoveBrokenSymlinks => {
            match blocking(move || health::remove_broken_symlinks(&ctx)).await {
                Ok(paths) => {
                    let count = paths.len();
                    JobOutcome {
                        ok: true,
                        aborted: false,
                        headline: format!("Removed {count} broken symlinks"),
                        lines: paths
                            .into_iter()
                            .map(|path| OutcomeLine::Done(path.display().to_string()))
                            .collect(),
                    }
                }
                Err(err) => failed_outcome(&title, err),
            }
        }
        JobSpec::RemoveBrokenPackages => {
            match health::remove_broken_packages(&ctx).await {
                Ok(report) => {
                    let removed = report.removed.len();
                    let skipped = report.cancelled.len();
                    let mut outcome = remove_outcome(report);
                    if ctx.cancel_token().is_cancelled() {
                        outcome.aborted = true;
                        outcome.headline = format!("Aborted: {removed} removed, {skipped} skipped");
                    }
                    outcome
                }
                Err(err) => failed_outcome(&title, err),
            }
        }
        JobSpec::Repo(RepoChange::Add(repo)) => {
            let name = repo.name.clone();
            match blocking(move || ctx.add_repository(repo)).await {
                Ok(()) => ok_outcome(format!("Added repository {name}")),
                Err(err) => failed_outcome(&title, err),
            }
        }
        JobSpec::Repo(RepoChange::SetEnabled {
            name,
            enabled,
        }) => {
            let change = RepoUpdate {
                url: None,
                enabled: Some(enabled),
                pubkey: None,
                desktop_integration: None,
                signature_verification: None,
                sync_interval: None,
            };
            let headline = if enabled {
                format!("Enabled {name}")
            } else {
                format!("Disabled {name}")
            };
            match blocking(move || ctx.update_repository(&name, change)).await {
                Ok(()) => ok_outcome(headline),
                Err(err) => failed_outcome(&title, err),
            }
        }
        JobSpec::Repo(RepoChange::Remove(name)) => {
            let headline = format!("Removed repository {name}");
            match blocking(move || ctx.remove_repository(&name)).await {
                Ok(()) => ok_outcome(headline),
                Err(err) => failed_outcome(&title, err),
            }
        }
        JobSpec::CreatePackagesConfig => {
            match soar_config::packages::generate_default_packages_config() {
                Ok(()) => ok_outcome("Created packages.toml".to_string()),
                Err(err) => failed_outcome(&title, SoarError::Config(err)),
            }
        }
    }
}

/// Resolves install queries, one outcome per query. A query nothing
/// matches also gets the similar names worth trying; a failed lookup
/// leaves that list empty rather than failing the whole resolve.
async fn resolve_queries(
    ctx: &SoarContext,
    queries: &[String],
    options: &InstallOptions,
) -> Result<Vec<Resolution>, String> {
    let results = match install::resolve_packages(ctx, queries, options).await {
        Ok(results) => results,
        Err(err) => return Err(err.to_string()),
    };
    let mut resolutions = Vec::with_capacity(results.len());
    for result in results {
        resolutions.push(match result {
            ResolveResult::Resolved(targets) => Resolution::Ready(targets),
            ResolveResult::Ambiguous(AmbiguousPackage {
                query,
                candidates,
            }) => {
                Resolution::Ambiguous {
                    query,
                    candidates,
                }
            }
            ResolveResult::NotFound(query) => {
                let suggestions = search::suggest_similar(ctx, &query, 3)
                    .await
                    .unwrap_or_default();
                Resolution::NotFound {
                    query,
                    suggestions,
                }
            }
            ResolveResult::AlreadyInstalled {
                pkg_name,
                repo_name,
                version,
            } => {
                Resolution::AlreadyInstalled {
                    name: pkg_name,
                    repo: repo_name,
                    version,
                }
            }
        });
    }
    Ok(resolutions)
}

/// Runs a filesystem-bound call on the blocking pool, so it does not hold up
/// the async workers. A panic in the call reports as a job failure.
async fn blocking<T: Send + 'static>(
    call: impl FnOnce() -> SoarResult<T> + Send + 'static,
) -> SoarResult<T> {
    tokio::task::spawn_blocking(call)
        .await
        .unwrap_or_else(|err| Err(SoarError::Custom(err.to_string())))
}

/// The escalation tool's own name, for the note the interface prints
/// before the terminal pass takes the screen.
fn tool_name(tool: &EscalationTool) -> String {
    tool.path.file_name().map_or_else(
        || tool.path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// The arguments the privileged soar child runs with. `--json` keeps the
/// progress flowing over the captured output, `--system` selects the
/// system tree, the forwarded options carry the configuration and network
/// settings, and `apply --only-system --yes` applies just the packages
/// marked `system = true` without asking again.
fn system_pass_args(packages_path: &str, forward_args: &[OsString], json: bool) -> Vec<OsString> {
    let mut args: Vec<OsString> = Vec::new();
    if json {
        args.push("--json".into());
    }
    args.push("--system".into());
    args.extend(forward_args.iter().cloned());
    args.push("apply".into());
    args.push("--only-system".into());
    args.push("--yes".into());
    args.push("--packages".into());
    args.push(packages_path.into());
    args
}

/// Runs the system pass with the terminal handed over, so the escalation
/// tool can ask for the password itself. The output lands in the terminal
/// the user is looking at, and only the exit status reports back.
fn run_system_terminal(
    forward_args: &[OsString],
    tool: &EscalationTool,
    packages_path: &str,
) -> JobOutcome {
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(err) => return system_failed(format!("finding the soar executable: {err}")),
    };
    let args = system_pass_args(packages_path, forward_args, false);
    match escalated_command(tool, &[], &exe, &args).status() {
        Ok(status) if status.success() => ok_outcome("System packages applied".to_string()),
        Ok(status) => system_failed(format!("the system pass exited with {status}")),
        Err(err) => system_failed(format!("running the system pass: {err}")),
    }
}

/// Runs the system pass through the escalation tool with its output
/// captured, so the child's JSON event stream feeds the job's progress.
/// Root runs the executable on its own; otherwise the tool runs it with
/// the credentials the password check cached.
fn run_system_direct(
    packages_path: String,
    forward_args: Vec<OsString>,
    tx: Sender<Msg>,
) -> JobOutcome {
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(err) => return system_failed(format!("finding the soar executable: {err}")),
    };
    let args = system_pass_args(&packages_path, &forward_args, true);
    let mut command = if soar_utils::system::is_root() {
        let mut command = Command::new(&exe);
        command.args(&args);
        command
    } else {
        match find_escalation_tool() {
            Some(tool) => escalated_command(&tool, &["-n"], &exe, &args),
            None => {
                return system_failed(format!(
                    "System mode requires root privileges. Neither 'doas' nor 'sudo' found in {}.",
                    ESCALATION_DIRS.join(", ")
                ));
            }
        }
    };
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => return system_failed(format!("running the system pass: {err}")),
    };

    // Both pipes feed one channel. The exit check runs on every pass
    // before the wait, so output arriving faster than the poll cannot
    // hide an exit: once the child is gone, a grandchild it left behind
    // may still hold the pipes open, and the loop stops a short grace
    // after the exit rather than waiting for an end of output that may
    // never come. A reader thread blocked in read() notices neither the
    // receiver dropping nor that grace, so after a grace break it stays
    // leaked until the pipes close; two leaked threads per grace-broken
    // pass is the accepted cost.
    let (line_tx, line_rx) = std::sync::mpsc::channel::<PipeLine>();
    let out_tx = line_tx.clone();
    let stdout = child.stdout.take();
    std::thread::spawn(move || {
        if let Some(stdout) = stdout {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if out_tx.send(PipeLine::Out(line)).is_err() {
                    return;
                }
            }
        }
    });
    let stderr = child.stderr.take();
    std::thread::spawn(move || {
        if let Some(stderr) = stderr {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if line_tx.send(PipeLine::Err(line)).is_err() {
                    return;
                }
            }
        }
    });

    let (complete, kept_errors) = collect_output(
        &line_rx,
        &mut || child.try_wait().ok().flatten().is_some(),
        &tx,
    );
    let status = child.wait();
    match status {
        Ok(status) if status.success() => {
            match complete {
                Some(counts) => {
                    JobOutcome {
                        ok: counts.failed == 0,
                        aborted: false,
                        headline: format!(
                            "System: {} installed, {} updated, {} removed",
                            counts.installed, counts.updated, counts.removed
                        ),
                        lines: Vec::new(),
                    }
                }
                None => ok_outcome("System packages applied".to_string()),
            }
        }
        Ok(status) => {
            let mut error = if kept_errors.is_empty() {
                format!("the system pass exited with {status}")
            } else {
                kept_errors.into_iter().collect::<Vec<_>>().join("\n")
            };
            if password_demanded(&error) {
                error.push('\n');
                error.push_str(
                    "sudo did not keep the authorization. Run soar apply in a terminal instead.",
                );
            }
            system_failed(error)
        }
        Err(err) => system_failed(format!("waiting for the system pass: {err}")),
    }
}

/// Reads the system pass's pipe lines until the child has exited and the
/// grace passed, or the readers hung up. The exit check runs before every
/// wait, so a grandchild emitting a line more often than the poll cannot
/// keep the loop running past the child's own exit. Stdout lines are
/// forwarded as events or plain output, stderr lines as warnings, with
/// the last few kept back for a failure report. Returns the completion
/// counts and the kept error lines.
fn collect_output(
    line_rx: &Receiver<PipeLine>,
    is_exited: &mut dyn FnMut() -> bool,
    tx: &Sender<Msg>,
) -> (Option<ApplyCounts>, VecDeque<String>) {
    let mut complete = None;
    let mut kept_errors: VecDeque<String> = VecDeque::new();
    let mut exited_at: Option<Instant> = None;
    loop {
        if is_exited() {
            let since = exited_at.get_or_insert_with(Instant::now);
            if Instant::now() - *since > PIPE_GRACE {
                break;
            }
        }
        match line_rx.recv_timeout(PIPE_POLL) {
            Ok(PipeLine::Out(line)) => {
                match serde_json::from_str::<SoarEvent>(&line) {
                    Ok(event) => {
                        if let SoarEvent::ApplyComplete {
                            installed,
                            updated,
                            removed,
                            failed,
                        } = &event
                        {
                            complete = Some(ApplyCounts {
                                installed: *installed,
                                updated: *updated,
                                removed: *removed,
                                failed: *failed,
                            });
                        }
                        let _ = tx.send(Msg::Soar(event));
                    }
                    Err(_) => {
                        let _ = tx.send(Msg::Output(line));
                    }
                }
            }
            Ok(PipeLine::Err(line)) => {
                let _ = tx.send(Msg::Output(format!("WARN {line}")));
                if kept_errors.len() == KEPT_STDERR_LINES {
                    kept_errors.pop_front();
                }
                kept_errors.push_back(line);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    (complete, kept_errors)
}

/// The installed, updated, removed, and failed counts the system pass
/// reported when it was over.
struct ApplyCounts {
    installed: usize,
    updated: usize,
    removed: usize,
    failed: usize,
}

/// Whether stderr says the tool wanted a password after all, meaning the
/// credentials cached by the password check ran out before the pass.
fn password_demanded(stderr: &str) -> bool {
    stderr.to_lowercase().contains("password is required")
}

/// The outcome of a system pass that did not run through.
fn system_failed(error: String) -> JobOutcome {
    JobOutcome {
        ok: false,
        aborted: false,
        headline: "System apply failed".to_string(),
        lines: vec![OutcomeLine::Failed {
            name: "System apply".to_string(),
            error,
        }],
    }
}

/// The outcome of a job that ran through without a failure.
fn ok_outcome(headline: String) -> JobOutcome {
    JobOutcome {
        ok: true,
        aborted: false,
        headline,
        lines: Vec::new(),
    }
}

/// The outcome of a call that failed outright. A call stopped by an abort
/// reports as aborted rather than failed.
fn failed_outcome(title: &str, err: SoarError) -> JobOutcome {
    if matches!(err, SoarError::Cancelled) {
        JobOutcome {
            ok: true,
            aborted: true,
            headline: "Aborted".into(),
            lines: Vec::new(),
        }
    } else {
        JobOutcome {
            ok: false,
            aborted: false,
            headline: format!("{title} failed"),
            lines: vec![OutcomeLine::Failed {
                name: title.to_string(),
                error: err.to_string(),
            }],
        }
    }
}

/// Turns an install report into the job's outcome.
fn install_outcome(report: InstallReport) -> JobOutcome {
    let mut lines = Vec::new();
    for info in &report.installed {
        lines.push(OutcomeLine::Done(format!(
            "{} {}",
            info.pkg_name, info.version
        )));
    }
    for failed in &report.failed {
        lines.push(OutcomeLine::Failed {
            name: failed.pkg_name.clone(),
            error: failed.error.clone(),
        });
    }
    for name in &report.cancelled {
        lines.push(OutcomeLine::Skipped(name.clone()));
    }
    for info in &report.installed {
        for text in info.notes.iter().flatten() {
            lines.push(OutcomeLine::Note {
                name: info.pkg_name.clone(),
                text: text.clone(),
            });
        }
    }
    for warning in &report.warnings {
        lines.push(OutcomeLine::Warning(warning.clone()));
    }
    let headline = if report.failed.is_empty() {
        match report.installed.as_slice() {
            [] => "Installed 0 packages".to_string(),
            [one] => format!("Installed {} {}", one.pkg_name, one.version),
            many => format!("Installed {} packages", many.len()),
        }
    } else {
        format!("Install finished with {} failures", report.failed.len())
    };
    JobOutcome {
        ok: report.failed.is_empty(),
        aborted: false,
        headline,
        lines,
    }
}

/// Turns a removal report into the job's outcome.
fn remove_outcome(report: RemoveReport) -> JobOutcome {
    let mut lines = Vec::new();
    for info in &report.removed {
        lines.push(OutcomeLine::Done(format!(
            "{} {}",
            info.pkg_name, info.version
        )));
    }
    for failed in &report.failed {
        lines.push(OutcomeLine::Failed {
            name: failed.pkg_name.clone(),
            error: failed.error.clone(),
        });
    }
    for name in &report.cancelled {
        lines.push(OutcomeLine::Skipped(name.clone()));
    }
    JobOutcome {
        ok: report.failed.is_empty(),
        aborted: false,
        headline: format!("Removed {} packages", report.removed.len()),
        lines,
    }
}

/// Turns an update report into the job's outcome.
fn update_outcome(report: UpdateReport) -> JobOutcome {
    let mut lines = Vec::new();
    for info in &report.updated {
        lines.push(OutcomeLine::Done(format!(
            "{} {}",
            info.pkg_name, info.version
        )));
    }
    for failed in &report.failed {
        lines.push(OutcomeLine::Failed {
            name: failed.pkg_name.clone(),
            error: failed.error.clone(),
        });
    }
    for name in &report.cancelled {
        lines.push(OutcomeLine::Skipped(name.clone()));
    }
    JobOutcome {
        ok: report.failed.is_empty(),
        aborted: false,
        headline: format!("Updated {} packages", report.updated.len()),
        lines,
    }
}

/// Turns an apply report into the job's outcome. The report carries counts
/// only, so there are no per-package lines.
fn apply_outcome(report: ApplyReport) -> JobOutcome {
    JobOutcome {
        ok: report.failed_count == 0,
        aborted: false,
        headline: format!(
            "Applied: {} installed, {} updated, {} removed",
            report.installed_count, report.updated_count, report.removed_count
        ),
        lines: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use soar_operations::{
        ApplyReport, FailedInfo, InstallReport, InstalledInfo, RemoveReport, RemovedInfo,
        UpdateReport,
    };

    use super::*;

    #[test]
    fn the_system_pass_args_carry_the_config_and_the_json_stream() {
        let forward: Vec<std::ffi::OsString> = ["--user".into(), "--net-timeout".into()].to_vec();
        let args = system_pass_args("/tmp/packages.toml", &forward, true);
        let shown: Vec<String> = args
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            shown,
            vec![
                "--json",
                "--system",
                "--user",
                "--net-timeout",
                "apply",
                "--only-system",
                "--yes",
                "--packages",
                "/tmp/packages.toml",
            ]
        );
        let quiet = system_pass_args("/tmp/packages.toml", &[], false);
        assert!(
            !quiet.iter().any(|arg| arg.to_string_lossy() == "--json"),
            "the terminal pass prints for a person"
        );
    }

    #[test]
    fn password_demanded_spots_sudos_own_words() {
        assert!(password_demanded("sudo: a password is required"));
        assert!(!password_demanded("dpkg gave up halfway"));
    }

    #[test]
    fn the_output_wait_honors_the_grace_while_lines_keep_arriving() {
        let (line_tx, line_rx) = std::sync::mpsc::channel::<PipeLine>();
        let (msg_tx, _msg_rx) = std::sync::mpsc::channel::<Msg>();
        // A grandchild that never stops emitting: the sends only fail
        // once the receiver is gone.
        let writer = std::thread::spawn(move || {
            while line_tx.send(PipeLine::Out("heartbeat".to_string())).is_ok() {
                std::thread::sleep(Duration::from_millis(10));
            }
        });
        let mut exited = || true;
        let started = Instant::now();
        let (complete, kept_errors) = collect_output(&line_rx, &mut exited, &msg_tx);
        let elapsed = started.elapsed();
        drop(line_rx);
        let _ = writer.join();
        assert!(complete.is_none());
        assert!(kept_errors.is_empty());
        assert!(elapsed >= PIPE_GRACE, "the wait gave up after {elapsed:?}");
        assert!(
            elapsed < Duration::from_secs(3),
            "the wait ran {elapsed:?}, far past the {PIPE_GRACE:?} grace"
        );
    }

    fn installed_info(name: &str, version: &str) -> InstalledInfo {
        InstalledInfo {
            pkg_name: name.to_string(),
            pkg_family: None,
            repo_name: "bincache".to_string(),
            version: version.to_string(),
            install_dir: PathBuf::new(),
            symlinks: Vec::new(),
            shared: Vec::new(),
            notes: None,
        }
    }

    fn removed_info(name: &str, version: &str) -> RemovedInfo {
        RemovedInfo {
            pkg_name: name.to_string(),
            repo_name: "bincache".to_string(),
            version: version.to_string(),
        }
    }

    fn failed_info(name: &str, error: &str) -> FailedInfo {
        FailedInfo {
            pkg_name: name.to_string(),
            error: error.to_string(),
        }
    }

    #[test]
    fn an_install_report_becomes_per_package_lines_and_a_counted_headline() {
        let mut bat = installed_info("bat", "0.24.0");
        bat.notes = Some(vec!["Config lives in ~/.config/bat".to_string()]);
        let report = InstallReport {
            installed: vec![bat, installed_info("fd", "10.2.0")],
            failed: vec![failed_info("jq", "no checksum")],
            warnings: vec!["Signature verification skipped".to_string()],
            cancelled: vec!["yq".to_string()],
        };
        let outcome = install_outcome(report);
        assert!(!outcome.ok);
        assert!(!outcome.aborted);
        assert_eq!(outcome.headline, "Install finished with 1 failures");
        assert_eq!(outcome.lines.len(), 6);
        assert!(matches!(&outcome.lines[0], OutcomeLine::Done(text) if text == "bat 0.24.0"));
        assert!(matches!(&outcome.lines[1], OutcomeLine::Done(text) if text == "fd 10.2.0"));
        assert!(
            matches!(&outcome.lines[2], OutcomeLine::Failed { name, error } if name == "jq" && error == "no checksum")
        );
        assert!(matches!(&outcome.lines[3], OutcomeLine::Skipped(name) if name == "yq"));
        assert!(
            matches!(&outcome.lines[4], OutcomeLine::Note { name, text } if name == "bat" && text == "Config lives in ~/.config/bat")
        );
        assert!(
            matches!(&outcome.lines[5], OutcomeLine::Warning(text) if text == "Signature verification skipped")
        );
    }

    #[test]
    fn a_single_install_is_named_in_the_headline() {
        let report = InstallReport {
            installed: vec![installed_info("bat", "0.24.0")],
            failed: Vec::new(),
            warnings: Vec::new(),
            cancelled: Vec::new(),
        };
        let outcome = install_outcome(report);
        assert!(outcome.ok);
        assert_eq!(outcome.headline, "Installed bat 0.24.0");
        assert_eq!(outcome.lines.len(), 1);
    }

    #[test]
    fn several_installs_are_counted_in_the_headline() {
        let report = InstallReport {
            installed: vec![
                installed_info("bat", "0.24.0"),
                installed_info("fd", "10.2.0"),
            ],
            failed: Vec::new(),
            warnings: Vec::new(),
            cancelled: Vec::new(),
        };
        let outcome = install_outcome(report);
        assert!(outcome.ok);
        assert_eq!(outcome.headline, "Installed 2 packages");
    }

    #[test]
    fn a_remove_report_becomes_per_package_lines_and_a_counted_headline() {
        let report = RemoveReport {
            removed: vec![
                removed_info("obsidian", "1.7.7"),
                removed_info("zellij", "0.41.2"),
            ],
            failed: vec![failed_info("fd", "files still open")],
            cancelled: vec!["jq".to_string()],
        };
        let outcome = remove_outcome(report);
        assert!(!outcome.ok);
        assert_eq!(outcome.headline, "Removed 2 packages");
        assert!(matches!(&outcome.lines[0], OutcomeLine::Done(text) if text == "obsidian 1.7.7"));
        assert!(matches!(&outcome.lines[1], OutcomeLine::Done(text) if text == "zellij 0.41.2"));
        assert!(
            matches!(&outcome.lines[2], OutcomeLine::Failed { name, error } if name == "fd" && error == "files still open")
        );
        assert!(matches!(&outcome.lines[3], OutcomeLine::Skipped(name) if name == "jq"));
    }

    #[test]
    fn an_update_report_becomes_per_package_lines_and_a_counted_headline() {
        let report = UpdateReport {
            updated: vec![installed_info("yq", "4.44.3")],
            failed: Vec::new(),
            url_updates: Vec::new(),
            cancelled: Vec::new(),
        };
        let outcome = update_outcome(report);
        assert!(outcome.ok);
        assert_eq!(outcome.headline, "Updated 1 packages");
        assert!(matches!(&outcome.lines[0], OutcomeLine::Done(text) if text == "yq 4.44.3"));
    }

    #[test]
    fn an_apply_report_becomes_a_counted_headline() {
        let outcome = apply_outcome(ApplyReport {
            installed_count: 2,
            updated_count: 1,
            removed_count: 0,
            failed_count: 0,
            cancelled_count: 0,
        });
        assert!(outcome.ok);
        assert_eq!(
            outcome.headline,
            "Applied: 2 installed, 1 updated, 0 removed"
        );

        let failed = apply_outcome(ApplyReport {
            installed_count: 2,
            updated_count: 1,
            removed_count: 0,
            failed_count: 1,
            cancelled_count: 0,
        });
        assert!(!failed.ok);
    }

    #[test]
    fn a_cancelled_call_reports_as_aborted() {
        let outcome = failed_outcome("Sync repositories", SoarError::Cancelled);
        assert!(outcome.ok);
        assert!(outcome.aborted);
        assert_eq!(outcome.headline, "Aborted");
        assert!(outcome.lines.is_empty());
    }

    #[test]
    fn a_failed_call_names_the_job_and_carries_the_error() {
        let outcome = failed_outcome("Sync repositories", SoarError::Custom("offline".into()));
        assert!(!outcome.ok);
        assert!(!outcome.aborted);
        assert_eq!(outcome.headline, "Sync repositories failed");
        assert_eq!(outcome.lines.len(), 1);
        assert!(
            matches!(&outcome.lines[0], OutcomeLine::Failed { name, error } if name == "Sync repositories" && error == "offline")
        );
    }
}
