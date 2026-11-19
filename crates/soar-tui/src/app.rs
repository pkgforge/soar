//! The interface's state and how it reacts to messages.

use std::{
    collections::{HashSet, VecDeque},
    path::PathBuf,
    time::{Duration, Instant},
};

use ratatui::{
    crossterm::event::{
        Event, KeyCode, KeyEvent, KeyEventKind, MouseButton, MouseEvent, MouseEventKind,
    },
    layout::{Position, Rect},
    text::{Line, Span},
};
use soar_config::repository::Repository;
use soar_core::{
    database::models::{InstalledPackage, Package},
    package::install::InstallTarget,
};
use soar_events::{LogLevel as EventLevel, SoarEvent, UpdateCheckStatus};
use soar_operations::{InstalledEntry, PrepareRunResult, VariantInfo, privilege::EscalationTool};

use crate::{
    action::Action,
    effect::{Effect, Suspended},
    format,
    jobs::{
        InstallChoices, Job, JobId, JobKind, JobOutcome, JobSpec, JobState, Jobs, OutcomeLine,
        RepoChange, SystemMode,
    },
    keymap,
    loadable::Loadable,
    log::{Log, LogLevel},
    msg::{Loaded, Msg, Resolution},
    privileged::EscalationProbe,
    progress::Tracker,
    theme::Theme,
    views::{
        ViewId,
        apply::ApplyView,
        browse::{BrowseView, PackageKey},
        health::HealthView,
        installed::InstalledView,
        jobs::{JobsFocus, JobsView},
        repos::ReposView,
        updates::UpdatesView,
    },
    widgets::{
        authorize::Authorize,
        confirm::Confirm,
        form::{Field, FieldValue, Form, FormValues},
        help::Help,
        overlay::{OnAccept, Overlay, OverlayOutcome, QuitGuard, Value},
        picker::Picker,
        text_input::{Prompt, TextInput},
        toast::{ToastKind, Toasts},
    },
};

/// Facts about this session, shown in the header.
pub(crate) struct AppEnv {
    /// The version string shown in the header.
    pub(crate) version: String,
    /// The profile the interface runs against.
    pub(crate) profile: String,
    /// Whether the session manages the system tree.
    pub(crate) system: bool,
    /// The configuration file the repositories view opens in an editor.
    pub(crate) config_path: PathBuf,
    /// The declared packages file the apply view edits and creates.
    pub(crate) packages_path: PathBuf,
}

/// An install being arranged, from the resolution to the accepted confirm.
/// It holds what the queries resolved to and what the user picked until
/// the confirm queues the job or the flow is cancelled.
pub(crate) struct PendingInstall {
    /// The options the install runs with.
    choices: InstallChoices,
    /// The resolved install plans, ready to run.
    targets: Vec<InstallTarget>,
    /// The variants the user chose for ambiguous queries.
    picked: Vec<Package>,
    /// The ambiguous queries still to ask about, each with the packages
    /// it matched.
    ambiguous: VecDeque<(String, Vec<Package>)>,
}

/// How long the finished-job summary stays in the activity strip after the
/// last job ends.
const STRIP_LINGER: Duration = Duration::from_secs(3);

/// How many rows a health fix confirm lists before the rest fold into a
/// count.
const CONFIRM_ROWS: usize = 10;

/// How many rows the wheel moves the list under the pointer by.
const WHEEL_ROWS: usize = 3;

/// Where chrome elements landed in the last frame, so pointer clicks can
/// find what they hit.
#[derive(Debug, Default)]
pub(crate) struct LayoutCache {
    /// The rect of each header tab, in screen coordinates.
    pub(crate) tabs: Vec<(Rect, ViewId)>,
    /// The rows area of the focused view's main list, with the index of
    /// its first visible row.
    pub(crate) list: Option<(Rect, usize)>,
    /// The rows area of the detail pane on screen, when one fills the
    /// screen or sits beside the jobs list.
    pub(crate) detail: Option<Rect>,
    /// The rows area of the jobs view's log pane.
    pub(crate) log: Option<Rect>,
    /// The rect an open overlay covers, when one is open.
    pub(crate) overlay: Option<Rect>,
}

/// The interface's whole state. Every change goes through [`App::update`].
pub(crate) struct App {
    /// The view on screen.
    pub(crate) view: ViewId,
    /// The session facts shown in the header.
    pub(crate) env: AppEnv,
    /// Whether the user asked to quit.
    pub(crate) quit: bool,
    /// Whether the screen needs a redraw.
    pub(crate) dirty: bool,
    /// The colors and glyphs this session draws with.
    pub(crate) theme: Theme,
    /// Spinner frame counter, advanced on every tick.
    pub(crate) frame: u64,
    /// Where chrome elements landed in the last frame.
    pub(crate) layout: LayoutCache,
    /// The overlay capturing input, if one is open.
    pub(crate) overlay: Option<Overlay>,
    /// The brief notifications stacked over the body.
    pub(crate) toasts: Toasts,
    /// The mutating operations, run one at a time from a queue.
    pub(crate) jobs: Jobs,
    /// Progress of soar work outside the queue, such as the metadata sync on
    /// first load or a download started by running a package.
    pub(crate) background: Tracker,
    /// When the finished-job summary leaves the activity strip.
    pub(crate) activity_until: Option<Instant>,
    /// The last time a message arrived, for elapsed times on screen.
    pub(crate) now: Instant,
    /// The rolling log of everything the interface and soar printed.
    pub(crate) log: Log,
    /// The cursor state of the jobs view.
    pub(crate) jobs_view: JobsView,
    /// The package catalog view.
    pub(crate) browse: BrowseView,
    /// The installed packages view.
    pub(crate) installed: InstalledView,
    /// The available updates view.
    pub(crate) updates: UpdatesView,
    /// The declarative packages view.
    pub(crate) apply: ApplyView,
    /// The repositories view.
    pub(crate) repos: ReposView,
    /// The health checks and the paths they ran against.
    pub(crate) health: HealthView,
    /// The install being arranged, from the resolution to the confirm.
    pub(crate) pending: Option<PendingInstall>,
    /// The run waiting for its variant to be picked: the arguments the
    /// run prompt collected with the packages the ambiguous query
    /// matched. The picker's accept turns the chosen package into an
    /// exact query and prepares the run again.
    pub(crate) run_pick: Option<(Vec<String>, Vec<Package>)>,
    /// The variants loaded for a switch: the package name they belong to
    /// with the variants themselves. The picker's accept links the
    /// chosen one, and closing the picker drops them.
    pub(crate) variant_pick: Option<(String, Vec<VariantInfo>)>,
    /// The rows the update confirm was opened for, by index into the
    /// ready updates list, sorted ascending. The confirm's accept moves
    /// those entries out of the list, and closing the confirm drops the
    /// indexes unused.
    pub(crate) update_pick: Option<Vec<usize>>,
    /// The package the version picker was opened for, with its newest
    /// version and the older ones. The picker's accept reads the picked
    /// index against this package, so a catalog reload while the picker
    /// is open cannot aim the pick at another row.
    pub(crate) version_pick: Option<(PackageKey, String, Vec<String>)>,
    /// The package the run prompt was opened for, as the query, the
    /// repository, and the package id a run needs. Accepting the prompt
    /// runs this package with the typed arguments, whatever row the
    /// cursor sits on by then.
    pub(crate) run_args: Option<(String, Option<String>, Option<String>)>,
    /// Whether the interface quits once the running job finishes.
    pub(crate) quit_when_idle: bool,
    /// The number of the password check in flight. Every submission
    /// numbers itself so an answer from a check a newer one replaced can
    /// be dropped.
    pub(crate) pass_check: u32,
    /// The number of the catalog load in flight. Every load numbers
    /// itself so a reply from a load a newer one replaced, success or
    /// error, can be dropped instead of overwriting newer state.
    pub(crate) catalog_load: u32,
    /// The number of the installed list load in flight, numbered like
    /// the catalog's so a replaced load's reply is dropped.
    pub(crate) installed_load: u32,
    /// The job the abort confirm was opened for. The confirm names that
    /// job, so accepting it stops that one and not whatever runs by the
    /// time the answer comes.
    pub(crate) abort_pick: Option<JobId>,
    /// The plan number the apply confirm was opened against. A fresh
    /// plan arriving under the confirm retires it, so accepting can
    /// never queue removals the confirm never named.
    pub(crate) apply_pick: Option<u32>,
    /// The error that ended the terminal input, if any. The interface
    /// cannot show it once its input is gone, so the entry point prints
    /// it after the terminal is restored.
    pub(crate) fatal: Option<String>,
}

impl App {
    /// Creates the state the interface starts in: on the first view, dirty so
    /// the first frame gets drawn.
    pub(crate) fn new(env: AppEnv, theme: Theme) -> App {
        App {
            view: ViewId::Browse,
            env,
            quit: false,
            dirty: true,
            theme,
            frame: 0,
            layout: LayoutCache::default(),
            overlay: None,
            toasts: Toasts::default(),
            jobs: Jobs::new(),
            background: Tracker::default(),
            activity_until: None,
            now: Instant::now(),
            log: Log::new(),
            jobs_view: JobsView::default(),
            browse: BrowseView::default(),
            installed: InstalledView::default(),
            updates: UpdatesView::default(),
            apply: ApplyView::default(),
            repos: ReposView::default(),
            health: HealthView::default(),
            pending: None,
            run_pick: None,
            variant_pick: None,
            update_pick: None,
            version_pick: None,
            run_args: None,
            quit_when_idle: false,
            pass_check: 0,
            catalog_load: 0,
            installed_load: 0,
            abort_pick: None,
            apply_pick: None,
            fatal: None,
        }
    }

    /// The number of available updates, shown beside the Updates tab.
    pub(crate) fn updates_badge(&self) -> Option<usize> {
        self.updates.list.ready().map(Vec::len)
    }

    /// Whether the last job failed without being seen, shown beside the Jobs
    /// tab. Opening the Jobs view clears it.
    pub(crate) fn jobs_attention(&self) -> bool {
        self.jobs.attention && self.view != ViewId::Jobs
    }

    /// The message at the right end of the status bar: how many rows are
    /// marked for the next bulk action.
    pub(crate) fn status_right(&self) -> String {
        let marked = match self.view {
            ViewId::Browse if !self.browse.marks.is_empty() => Some(self.browse.marks.len()),
            ViewId::Installed if !self.installed.marks.is_empty() => {
                Some(self.installed.marks.len())
            }
            _ => None,
        };
        marked
            .map(|count| format!("{count} marked"))
            .unwrap_or_default()
    }

    /// The live value shown after a status bar hint, such as the repository
    /// the catalog list is narrowed to or the order the installed list
    /// sorts in.
    pub(crate) fn hint_suffix(&self, action: &Action) -> Option<String> {
        if *action == Action::CycleRepo && self.view == ViewId::Browse {
            let name = self.browse.repo_filter.as_deref().unwrap_or("all");
            return Some(format!(": {name}"));
        }
        if *action == Action::CycleSort && self.view == ViewId::Installed {
            return Some(format!(": {}", self.installed.sort.label()));
        }
        None
    }

    /// Whether a bound key's hint applies right now. Hints that name an
    /// action with nothing to do stay off the status bar.
    pub(crate) fn hint_visible(&self, action: &Action) -> bool {
        if *action == Action::CreatePackages {
            return self
                .apply
                .plan
                .ready()
                .is_some_and(|plan| plan.diff.is_none());
        }
        true
    }

    /// The height of the activity strip above the status bar: a rule, a
    /// summary line, and up to four operation lines while a job runs, then
    /// a rule and a summary line for three seconds after the last one
    /// finishes, then a rule plus one line per repository still syncing
    /// and a summary for other background work, then nothing.
    pub(crate) fn activity_height(&self) -> u16 {
        if let Some(job) = self.running_job() {
            let lines = if job.kind == JobKind::Sync {
                job.tracker.sync.len()
            } else {
                job.tracker.active().count()
            };
            2 + lines.min(4) as u16 + u16::from(lines > 4)
        } else if self.activity_until.is_some() {
            2
        } else {
            let syncing = self.background.active_sync().count();
            let preparing = self.background.active().next().is_some();
            if syncing == 0 && !preparing {
                return 0;
            }
            1 + syncing.min(4) as u16 + u16::from(syncing > 4) + u16::from(preparing)
        }
    }

    /// Queues a brief notification and marks the screen for a redraw.
    pub(crate) fn toast(&mut self, kind: ToastKind, text: impl Into<String>, now: Instant) {
        self.toasts.push(kind, text, now);
        self.dirty = true;
    }

    /// Adds a job to the queue and starts it right away when nothing runs
    /// and nothing waits for authorization. The views call this as their
    /// actions are accepted; until then only tests do.
    #[allow(dead_code)]
    pub(crate) fn enqueue(&mut self, spec: JobSpec, now: Instant) -> Vec<Effect> {
        // Wait-and-quit drops the queue and leaves at the next idle
        // moment, so a confirmation accepted after it must not sneak an
        // unapproved job past the guard, and must not hold the quit open
        // either.
        if self.quit_when_idle {
            self.toast(
                ToastKind::Info,
                "Quitting soon, so no new work is accepted",
                now,
            );
            return Vec::new();
        }
        let id = self.jobs.next_id;
        self.jobs.next_id += 1;
        self.jobs.list.push(Job {
            id,
            kind: spec.kind(),
            title: spec.title(),
            state: JobState::Queued,
            spec: Some(spec),
            tracker: Tracker::default(),
        });
        self.dirty = true;
        if self.running_job().is_none() && !self.authorizing() {
            self.start_next(now)
        } else {
            Vec::new()
        }
    }

    /// Whether any job waits for the user to authorize the administrator
    /// pass. While one does, the queue holds still, since the pass that
    /// follows it must run before anything else.
    fn authorizing(&self) -> bool {
        self.jobs
            .list
            .iter()
            .any(|job| matches!(job.state, JobState::Authorizing))
    }

    /// The job waiting for the administrator authorization, if any.
    fn authorizing_job(&self) -> Option<&Job> {
        self.jobs
            .list
            .iter()
            .find(|job| matches!(job.state, JobState::Authorizing))
    }

    /// The job waiting for the administrator authorization, for finishing
    /// or starting it.
    fn authorizing_job_mut(&mut self) -> Option<&mut Job> {
        self.jobs
            .list
            .iter_mut()
            .find(|job| matches!(job.state, JobState::Authorizing))
    }

    /// The job currently running, if any. The quit guard and the activity
    /// strip read it.
    pub(crate) fn running_job(&self) -> Option<&Job> {
        self.jobs
            .list
            .iter()
            .find(|job| matches!(job.state, JobState::Running { .. }))
    }

    /// The job currently running, for feeding it progress events.
    pub(crate) fn running_job_mut(&mut self) -> Option<&mut Job> {
        self.jobs
            .list
            .iter_mut()
            .find(|job| matches!(job.state, JobState::Running { .. }))
    }

    /// How many jobs are waiting for their turn, shown by the activity
    /// strip's summary line.
    pub(crate) fn queued_count(&self) -> usize {
        self.jobs
            .list
            .iter()
            .filter(|job| matches!(job.state, JobState::Queued))
            .count()
    }

    /// Starts the first queued job, taking its spec, and returns the effect
    /// that runs it. A queued system pass first waits for the user to
    /// authorize it, so its turn returns the probe instead. Nothing starts
    /// while a job waits for authorization.
    fn start_next(&mut self, now: Instant) -> Vec<Effect> {
        if self.authorizing() {
            return Vec::new();
        }
        let Some(job) = self
            .jobs
            .list
            .iter_mut()
            .find(|job| matches!(job.state, JobState::Queued))
        else {
            return Vec::new();
        };
        if matches!(
            job.spec,
            Some(JobSpec::ApplySystem {
                mode: SystemMode::Pending,
                ..
            })
        ) {
            job.state = JobState::Authorizing;
            self.dirty = true;
            return vec![Effect::ProbeEscalation];
        }
        let Some(spec) = job.spec.take() else {
            return Vec::new();
        };
        job.state = JobState::Running {
            started: now,
            aborting: false,
        };
        self.dirty = true;
        vec![Effect::StartJob {
            id: job.id,
            spec,
        }]
    }

    /// Starts the job whose authorization succeeded, taking its spec.
    fn start_authorizing(&mut self, now: Instant) -> Vec<Effect> {
        let Some(job) = self.authorizing_job_mut() else {
            return Vec::new();
        };
        let JobState::Authorizing = job.state else {
            return Vec::new();
        };
        let Some(spec) = job.spec.take() else {
            return Vec::new();
        };
        job.state = JobState::Running {
            started: now,
            aborting: false,
        };
        let id = job.id;
        self.dirty = true;
        vec![Effect::StartJob {
            id,
            spec,
        }]
    }

    /// Finishes the job waiting for authorization as withdrawn by the
    /// user, then starts the next one.
    fn skip_authorizing(&mut self, now: Instant) -> Vec<Effect> {
        let Some(job) = self.authorizing_job() else {
            return Vec::new();
        };
        let id = job.id;
        let outcome = JobOutcome {
            ok: true,
            aborted: true,
            headline: "System packages skipped".to_string(),
            lines: Vec::new(),
        };
        self.on_job_finished(id, outcome, now)
    }

    /// Drops the pick and prompt state an open overlay kept, so nothing
    /// outlives the overlay it belonged to.
    fn drop_overlay_state(&mut self) {
        self.run_pick = None;
        self.variant_pick = None;
        self.update_pick = None;
        self.version_pick = None;
        self.run_args = None;
        self.abort_pick = None;
        self.apply_pick = None;
    }

    /// Opens the administrator password dialog for the authorizing job.
    /// While `busy`, it shows the password being checked.
    fn open_authorize(&mut self, busy: bool) {
        let names = match self.authorizing_job() {
            Some(Job {
                spec: Some(JobSpec::ApplySystem {
                    names, ..
                }),
                ..
            }) => names.clone(),
            _ => return,
        };
        // The dialog takes the overlay slot from whatever else is open,
        // so the state that overlay kept goes the way its own close path
        // would drop it.
        if self.overlay.is_some() && !matches!(self.overlay, Some(Overlay::Authorize(_))) {
            self.drop_overlay_state();
        }
        self.overlay = Some(Overlay::Authorize(if busy {
            Authorize::waiting(names)
        } else {
            Authorize::new(names)
        }));
        self.dirty = true;
    }

    /// The open password dialog, for showing why a password was refused.
    fn authorize_mut(&mut self) -> Option<&mut Authorize> {
        match self.overlay.as_mut() {
            Some(Overlay::Authorize(dialog)) => Some(dialog),
            _ => None,
        }
    }

    /// Marks a finished job, tells the user how it went, and starts the
    /// next one. A job that never ran, because its authorization failed
    /// or was withdrawn, finishes with the moment it was given up as its
    /// start.
    fn on_job_finished(&mut self, id: JobId, outcome: JobOutcome, now: Instant) -> Vec<Effect> {
        let Some(job) = self.jobs.list.iter_mut().find(|job| job.id == id) else {
            return Vec::new();
        };
        let kind = job.kind;
        let started = match job.state {
            JobState::Running {
                started, ..
            } => started,
            JobState::Authorizing => now,
            _ => return Vec::new(),
        };
        let ok = outcome.ok;
        let aborted = outcome.aborted;
        let headline = outcome.headline.clone();
        job.state = JobState::Finished {
            started,
            ended: now,
            outcome,
        };
        self.dirty = true;
        self.toast(
            if aborted {
                ToastKind::Warning
            } else if ok {
                ToastKind::Success
            } else {
                ToastKind::Error
            },
            headline,
            now,
        );
        if !ok && !aborted {
            self.jobs.attention = true;
        }
        if matches!(
            kind,
            JobKind::Install | JobKind::Remove | JobKind::Update | JobKind::Switch
        ) {
            self.browse.details.clear();
            self.dirty = true;
        }
        // Whatever starts next was never named by an open abort confirm,
        // so the confirm goes with the job it was opened for.
        self.retire_abort_confirm(id);
        let mut effects = self.after_job(kind, now);
        if self.quit_when_idle
            && self.running_job().is_none()
            && !self.authorizing()
            && self.queued_count() == 0
        {
            self.quit = true;
            self.dirty = true;
            return effects;
        }
        effects.extend(self.start_next(now));
        if self.running_job().is_none() {
            self.activity_until = Some(now + STRIP_LINGER);
        }
        effects
    }

    /// The effects that reload the views the finished job changed. An
    /// install changes which packages are marked installed, so the
    /// catalog reloads and its installed markers move without a manual
    /// reload. Every job that installs, removes, or replaces packages also
    /// changes the installed list, so it reloads with the catalog. A sync
    /// replaces the metadata databases on disk and a repository change
    /// replaces the configuration, both of which the runner's context
    /// caches, so the context is rebuilt before anything reads either and
    /// the repositories reload from the fresh configuration. Jobs that
    /// change what is installed also start a fresh update check
    /// so the list and its count catch up, but only when the updates view
    /// has loaded its list before, and an update job resets the list to
    /// loading first because it took its entries out of it. An apply moves
    /// the installation to what the packages file declares, so its plan
    /// recomputes with the prune flag in force while the catalog and the
    /// installed list reload. Jobs that fix
    /// the installation reload the health view unconditionally, and the
    /// install and removal jobs reload it once it has loaded its report.
    fn after_job(&mut self, kind: JobKind, now: Instant) -> Vec<Effect> {
        let mut effects = match kind {
            JobKind::Sync => {
                self.repos.start();
                let mut effects = vec![
                    Effect::RebuildContext,
                    self.catalog_effect(),
                    self.installed_effect(),
                    Effect::LoadRepos,
                ];
                if !self.updates.list.is_idle() {
                    let check = self.check_effect(self.updates.scope.clone(), now);
                    effects.push(check);
                }
                effects
            }
            JobKind::Repo => {
                self.repos.start();
                // A disabled or removed repository must leave the catalog
                // with it, the same as an edited configuration.
                vec![
                    Effect::RebuildContext,
                    Effect::LoadRepos,
                    self.catalog_effect(),
                    self.installed_effect(),
                ]
            }
            JobKind::Install => {
                let mut effects = vec![self.catalog_effect(), self.installed_effect()];
                if !self.health.report.is_idle() {
                    self.health.start();
                    effects.push(Effect::LoadHealth);
                }
                effects
            }
            JobKind::Remove | JobKind::Switch => {
                let mut effects = vec![self.installed_effect()];
                if kind == JobKind::Remove && !self.health.report.is_idle() {
                    self.health.start();
                    effects.push(Effect::LoadHealth);
                }
                effects
            }
            JobKind::Apply => {
                let mut effects = vec![
                    self.apply_effect(),
                    self.catalog_effect(),
                    self.installed_effect(),
                ];
                if !self.health.report.is_idle() {
                    self.health.start();
                    effects.push(Effect::LoadHealth);
                }
                effects
            }
            JobKind::ApplySystem => {
                vec![self.apply_effect()]
            }
            JobKind::RemoveBrokenPackages => {
                self.health.start();
                vec![self.installed_effect(), Effect::LoadHealth]
            }
            JobKind::CleanCache | JobKind::RemoveBrokenSymlinks => {
                self.health.start();
                vec![Effect::LoadHealth]
            }
            JobKind::CreatePackagesConfig => {
                vec![self.apply_effect()]
            }
            JobKind::Update => {
                let check = self.check_effect(self.updates.scope.clone(), now);
                vec![check, self.installed_effect(), self.catalog_effect()]
            }
        };
        if matches!(
            kind,
            JobKind::Install | JobKind::Remove | JobKind::Switch | JobKind::Apply
        ) && !self.updates.list.is_idle()
        {
            let check = self.check_effect(self.updates.scope.clone(), now);
            effects.push(check);
        }
        effects
    }

    /// The effects to run when the interface starts: loading the package
    /// catalog the first view shows, and the installed list the second
    /// one shows.
    pub(crate) fn startup(&mut self) -> Vec<Effect> {
        self.browse.catalog = Loadable::Loading;
        self.installed.list = Loadable::Loading;
        vec![self.catalog_effect(), self.installed_effect()]
    }

    /// Applies one message and returns the work it asks for.
    pub(crate) fn update(&mut self, msg: Msg, now: Instant) -> Vec<Effect> {
        self.now = now;
        match msg {
            Msg::Input(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                if self.overlay.is_some() {
                    self.on_overlay_key(key, now)
                } else {
                    self.on_key(key, now)
                }
            }
            Msg::Input(Event::Resize(..)) => {
                self.dirty = true;
                Vec::new()
            }
            Msg::Input(Event::Mouse(mouse)) => self.on_mouse(mouse, now),
            Msg::Input(_) => Vec::new(),
            Msg::Output(line) => {
                let (level, text) = Log::parse_output(&line);
                self.log.push(level, text.clone());
                if level == LogLevel::Error && self.running_job().is_none() {
                    self.toast(ToastKind::Error, text, now);
                }
                Vec::new()
            }
            Msg::Soar(event) => self.on_soar(event, now),
            Msg::Loaded(Loaded::Catalog {
                check,
                result,
            }) => {
                // Two loads can overlap, as when a manual reload races
                // the reload a repository change started, so a reply
                // from a load a newer one replaced must not overwrite
                // what the newer state already holds, whatever it found.
                if check != self.catalog_load {
                    return Vec::new();
                }
                let catalog_ok = result.is_ok();
                self.browse.catalog = match result {
                    Ok(rows) => Loadable::Ready(rows),
                    Err(error) => Loadable::Failed(error),
                };
                self.browse.refilter(now);
                self.dirty = true;
                // The first catalog load quietly starts an update check so
                // the header badge fills without a visit to the view.
                if catalog_ok && self.updates.list.is_idle() {
                    return vec![self.check_effect(None, now)];
                }
                Vec::new()
            }
            Msg::Loaded(Loaded::Installed {
                check,
                result,
            }) => {
                if check != self.installed_load {
                    return Vec::new();
                }
                self.installed.list = match result {
                    Ok(listing) => Loadable::Ready(listing),
                    Err(error) => Loadable::Failed(error),
                };
                self.installed.refilter();
                self.dirty = true;
                Vec::new()
            }
            Msg::Loaded(Loaded::Updates {
                check,
                result,
            }) => {
                if check != self.updates.check {
                    return Vec::new();
                }
                self.updates.loaded(result);
                // A fresh list moves every row, so a confirm holding row
                // indexes into the old one can no longer be trusted.
                if self.update_pick.take().is_some() {
                    self.close_update_confirm();
                }
                self.dirty = true;
                Vec::new()
            }
            Msg::Loaded(Loaded::ApplyDiff {
                check,
                result,
            }) => {
                if check != self.apply.check {
                    return Vec::new();
                }
                self.apply.loaded(result);
                // The plan the confirm named is gone, so accepting it
                // could queue removals the user never saw named.
                if self.apply_pick.take().is_some() {
                    self.close_apply_confirm();
                    self.toast(
                        ToastKind::Info,
                        "The plan changed, so the confirm closed",
                        now,
                    );
                }
                self.dirty = true;
                Vec::new()
            }
            Msg::Loaded(Loaded::Repos(result)) => {
                self.repos.loaded(result);
                self.dirty = true;
                Vec::new()
            }
            Msg::Loaded(Loaded::Health(result)) => {
                self.health.loaded(result);
                self.dirty = true;
                Vec::new()
            }
            Msg::Loaded(Loaded::Detail {
                key,
                result,
            }) => {
                let state = match *result {
                    Ok(package) => Loadable::Ready(package),
                    Err(error) => Loadable::Failed(error),
                };
                self.browse.cache_detail(key, state);
                self.dirty = true;
                Vec::new()
            }
            Msg::Loaded(Loaded::Resolved {
                choices,
                result,
            }) => {
                match result {
                    Ok(resolutions) => self.on_resolved(choices, resolutions, now),
                    Err(error) => {
                        self.toast(ToastKind::Error, error, now);
                        Vec::new()
                    }
                }
            }
            Msg::Loaded(Loaded::RunReady {
                args,
                result,
            }) => {
                match *result {
                    Ok(PrepareRunResult::Ready {
                        path, ..
                    }) => {
                        self.dirty = true;
                        vec![Effect::Suspend(Suspended::Exec {
                            path,
                            args,
                        })]
                    }
                    Ok(PrepareRunResult::Ambiguous(ambiguous)) => {
                        self.open_run_picker(ambiguous.query, ambiguous.candidates, args);
                        Vec::new()
                    }
                    Err(error) => {
                        self.toast(ToastKind::Error, error, now);
                        Vec::new()
                    }
                }
            }
            Msg::Loaded(Loaded::Variants {
                name,
                result,
            }) => self.on_variants_loaded(name, *result, now),
            Msg::Loaded(Loaded::Escalation(probe)) => {
                if self.authorizing_job().is_none() {
                    return Vec::new();
                }
                match probe {
                    EscalationProbe::Ready => {
                        if let Some(job) = self.authorizing_job_mut()
                            && let Some(JobSpec::ApplySystem {
                                mode, ..
                            }) = job.spec.as_mut()
                        {
                            *mode = SystemMode::Direct;
                        }
                        self.start_authorizing(now)
                    }
                    EscalationProbe::Password => {
                        self.open_authorize(false);
                        Vec::new()
                    }
                    EscalationProbe::Terminal(tool) => {
                        self.open_system_terminal(tool);
                        Vec::new()
                    }
                    EscalationProbe::Unavailable(message) => {
                        let Some(job) = self.authorizing_job() else {
                            return Vec::new();
                        };
                        let id = job.id;
                        let title = job.title.clone();
                        let outcome = JobOutcome {
                            ok: false,
                            aborted: false,
                            headline: format!("{title} failed"),
                            lines: vec![OutcomeLine::Failed {
                                name: title,
                                error: message,
                            }],
                        };
                        self.on_job_finished(id, outcome, now)
                    }
                }
            }
            Msg::Loaded(Loaded::Authenticated {
                check,
                result,
            }) => {
                let waiting =
                    matches!(&self.overlay, Some(Overlay::Authorize(dialog)) if dialog.is_busy());
                if !waiting || check != self.pass_check || self.authorizing_job().is_none() {
                    return Vec::new();
                }
                match result {
                    Ok(()) => {
                        self.overlay = None;
                        if let Some(job) = self.authorizing_job_mut()
                            && let Some(JobSpec::ApplySystem {
                                mode, ..
                            }) = job.spec.as_mut()
                        {
                            *mode = SystemMode::Direct;
                        }
                        self.start_authorizing(now)
                    }
                    Err(error) => {
                        if let Some(dialog) = self.authorize_mut() {
                            dialog.fail(error);
                        }
                        Vec::new()
                    }
                }
            }
            Msg::JobFinished {
                id,
                outcome,
            } => self.on_job_finished(id, outcome, now),
            Msg::Edited(path) => {
                // Each edited file drives the views that read it; anything
                // else just needs a repaint.
                self.dirty = true;
                if path == self.env.config_path {
                    vec![
                        Effect::RebuildContext,
                        Effect::LoadRepos,
                        self.catalog_effect(),
                        self.installed_effect(),
                    ]
                } else if path == self.env.packages_path {
                    vec![self.apply_effect()]
                } else {
                    Vec::new()
                }
            }
            Msg::InputFailed(error) => {
                // The reader is gone and no key will ever arrive, so the
                // interface quits and the entry point reports why.
                self.fatal = Some(error);
                self.quit = true;
                self.dirty = true;
                Vec::new()
            }
            Msg::Redraw => {
                self.dirty = true;
                Vec::new()
            }
            Msg::Tick => {
                self.frame += 1;
                if self.toasts.expire(now) {
                    self.dirty = true;
                }
                if self.running_job().is_some() || self.authorizing() {
                    self.dirty = true;
                } else if self.activity_until.is_some_and(|until| until <= now) {
                    self.activity_until = None;
                    self.dirty = true;
                }
                let mut effects = Vec::new();
                if let Some(key) = self.browse.due_detail(now)
                    && let Some(version) = self.browse.row_version(&key)
                {
                    effects.push(Effect::LoadDetail {
                        key,
                        version,
                    });
                }
                effects
            }
        }
    }

    /// Feeds one operation event into the running job's progress, or into
    /// the background tracker when no job runs, so a download or a sync
    /// started outside the queue still lands somewhere. Update checks are
    /// the Updates view's data, and log lines belong in the log buffer,
    /// which collects them later.
    fn on_soar(&mut self, event: SoarEvent, now: Instant) -> Vec<Effect> {
        match event {
            SoarEvent::UpdateCheck {
                pkg_name,
                status,
            } => {
                // The events carry no check number, so while a check and
                // the one that replaced it run together the counters
                // take strays from both. The load cannot tell them apart
                // and drops the lot, but only when a replacement
                // actually happened; otherwise the progress is the
                // check's own and stays for the skipped view.
                if self.updates.list.is_loading() {
                    self.updates.checked += 1;
                    if let UpdateCheckStatus::Skipped {
                        reason,
                    } = status
                    {
                        self.updates.skipped.push((pkg_name, reason));
                    }
                    self.dirty = true;
                }
            }
            SoarEvent::Log {
                level,
                message,
            } => {
                let level = match level {
                    EventLevel::Debug => LogLevel::Debug,
                    EventLevel::Info => LogLevel::Info,
                    EventLevel::Warning => LogLevel::Warn,
                    EventLevel::Error => LogLevel::Error,
                };
                self.log.push(level, message.clone());
                if level == LogLevel::Error && self.running_job().is_none() {
                    self.toast(ToastKind::Error, message, now);
                }
            }
            SoarEvent::ProcessOutput {
                line,
                pkg_name,
                op_id,
            } => {
                let clean = Log::strip_ansi(&line);
                self.log
                    .push(LogLevel::Info, format!("{pkg_name}: {clean}"));
                self.feed_tracker(
                    &SoarEvent::ProcessOutput {
                        op_id,
                        pkg_name,
                        line,
                    },
                    now,
                );
            }
            _ => self.feed_tracker(&event, now),
        }
        Vec::new()
    }

    /// Hands one operation event to the running job's progress, or to the
    /// background tracker when no job runs.
    fn feed_tracker(&mut self, event: &SoarEvent, now: Instant) {
        if let Some(job) = self.running_job_mut() {
            job.tracker.apply(event, now);
        } else {
            self.background.apply(event, now);
        }
        self.dirty = true;
    }

    /// Hands a pressed key to the open overlay. While one is open nothing
    /// else in the interface reacts to keys.
    fn on_overlay_key(&mut self, key: KeyEvent, now: Instant) -> Vec<Effect> {
        let Some(overlay) = self.overlay.take() else {
            return Vec::new();
        };
        self.dirty = true;
        match overlay.handle(key) {
            OverlayOutcome::Stay(overlay) => {
                self.overlay = Some(overlay);
                Vec::new()
            }
            OverlayOutcome::Close => self.on_overlay_close(now),
            OverlayOutcome::Accept(accept, value) => self.on_accept(accept, value, now),
            OverlayOutcome::Authenticate(secret) => {
                // The dialog goes back up in its waiting state, and the
                // secret travels to the runner alone.
                self.pass_check = self.pass_check.wrapping_add(1);
                let check = self.pass_check;
                self.open_authorize(true);
                vec![Effect::Authenticate {
                    secret,
                    check,
                }]
            }
        }
    }

    /// Responds to an overlay closing without acting. While an install is
    /// being arranged only the flow's own overlays can be open, so the
    /// close means one step further: the variant picker drops the query it
    /// was asking about, and the confirm cancels the whole install. The
    /// same close drops what a run variant picker kept, ending that flow,
    /// and what a switch variants picker kept. While a job waits for the
    /// administrator authorization, a closed dialog is a declined one, so
    /// the pass is skipped.
    fn on_overlay_close(&mut self, now: Instant) -> Vec<Effect> {
        if self.authorizing() {
            return self.skip_authorizing(now);
        }
        self.drop_overlay_state();
        let Some(mut pending) = self.pending.take() else {
            return Vec::new();
        };
        if pending.ambiguous.pop_front().is_some() {
            self.pending = Some(pending);
            self.advance_pending();
        }
        Vec::new()
    }

    /// Carries out what an accepted overlay asked for.
    fn on_accept(&mut self, accept: OnAccept, value: Option<Value>, now: Instant) -> Vec<Effect> {
        match accept {
            OnAccept::QuitWait => {
                self.wait_and_quit(now, false);
                Vec::new()
            }
            OnAccept::QuitAbort => self.wait_and_quit(now, true),
            OnAccept::QuitForce => {
                self.quit = true;
                self.dirty = true;
                Vec::new()
            }
            OnAccept::Abort => self.abort_confirmed(now),
            OnAccept::Remove(packages) => {
                self.installed.marks.clear();
                self.dirty = true;
                self.enqueue(
                    JobSpec::Remove {
                        packages,
                    },
                    now,
                )
            }
            OnAccept::UpdateNow => self.update_now(now),
            OnAccept::PickVariant => {
                match value {
                    Some(Value::Index(index)) => {
                        self.pick_variant(index);
                        Vec::new()
                    }
                    _ => Vec::new(),
                }
            }
            OnAccept::InstallNow => self.install_now(now),
            OnAccept::InstallWithOptions => {
                match value {
                    Some(Value::Form(FormValues(values))) => self.install_with_options(values, now),
                    _ => Vec::new(),
                }
            }
            OnAccept::PickVersion => {
                match value {
                    Some(Value::Index(index)) => self.install_picked_version(index, now),
                    _ => Vec::new(),
                }
            }
            OnAccept::InstallByQuery => {
                match value {
                    Some(Value::Text(text)) => self.install_by_query(text),
                    _ => Vec::new(),
                }
            }
            OnAccept::RunArguments => {
                match value {
                    Some(Value::Text(text)) => self.run_with_arguments(text),
                    _ => Vec::new(),
                }
            }
            OnAccept::RunVariant => {
                match value {
                    Some(Value::Index(index)) => self.run_picked_variant(index),
                    _ => Vec::new(),
                }
            }
            OnAccept::SwitchVariant => {
                match value {
                    Some(Value::Index(index)) => self.switch_picked_variant(index, now),
                    _ => Vec::new(),
                }
            }
            OnAccept::SwitchConfirm {
                id,
                name,
            } => {
                self.enqueue(
                    JobSpec::Switch {
                        id,
                        name,
                    },
                    now,
                )
            }
            OnAccept::RepoEnabled {
                name,
                enabled,
            } => {
                let mut effects = self.enqueue(
                    JobSpec::Repo(RepoChange::SetEnabled {
                        name: name.clone(),
                        enabled,
                    }),
                    now,
                );
                if enabled {
                    effects.extend(self.enqueue(JobSpec::Sync, now));
                }
                effects
            }
            OnAccept::RepoRemoved {
                name,
            } => self.enqueue(JobSpec::Repo(RepoChange::Remove(name)), now),
            OnAccept::CleanCacheConfirm => self.enqueue(JobSpec::CleanCache, now),
            OnAccept::FixSymlinksConfirm => self.enqueue(JobSpec::RemoveBrokenSymlinks, now),
            OnAccept::RemoveBrokenConfirm => self.enqueue(JobSpec::RemoveBrokenPackages, now),
            OnAccept::AddRepo => {
                match value {
                    Some(Value::Form(FormValues(values))) => self.add_repo(values, now),
                    _ => Vec::new(),
                }
            }
            OnAccept::ApplyPlan => self.apply_plan(now),
            OnAccept::SystemTerminal {
                tool,
            } => self.run_system_terminal(tool, now),
            #[cfg(test)]
            OnAccept::Test => Vec::new(),
        }
    }

    /// Sets up the install the resolution described: resolved targets go
    /// straight in, ambiguous queries queue for a picker, and the
    /// not-found and already installed ones toast. Then the flow's first
    /// step opens.
    fn on_resolved(
        &mut self,
        choices: InstallChoices,
        resolutions: Vec<Resolution>,
        now: Instant,
    ) -> Vec<Effect> {
        let mut pending = PendingInstall {
            choices,
            targets: Vec::new(),
            picked: Vec::new(),
            ambiguous: VecDeque::new(),
        };
        for resolution in resolutions {
            match resolution {
                Resolution::Ready(targets) => pending.targets.extend(targets),
                Resolution::Ambiguous {
                    query,
                    candidates,
                } => pending.ambiguous.push_back((query, candidates)),
                Resolution::NotFound {
                    query,
                    suggestions,
                } => {
                    let text = if suggestions.is_empty() {
                        format!("Not found: {query}.")
                    } else {
                        let near = suggestions.join(", ");
                        format!("Not found: {query}. Did you mean {near}?")
                    };
                    self.toast(ToastKind::Warning, text, now);
                }
                Resolution::AlreadyInstalled {
                    name, ..
                } => {
                    self.toast(ToastKind::Info, format!("{name} is already installed"), now);
                }
            }
        }
        self.pending = Some(pending);
        self.advance_pending();
        Vec::new()
    }

    /// Opens the next step of the install being arranged: a picker for the
    /// head ambiguous query, or the confirm listing everything to install
    /// once every query is decided. The confirm clears the flow again
    /// when nothing remains to install. While a pass is being authorized
    /// the flow opens nowhere: the resolution may answer long after the
    /// password dialog took the overlay slot, and stealing it back would
    /// leave the pass with no dialog to serve, so the install waits for a
    /// retry and the flow goes with the dropped resolution.
    fn advance_pending(&mut self) {
        if self.authorizing() {
            self.pending = None;
            return;
        }
        // The flow's overlay takes the slot from whatever else is open,
        // so the state that overlay kept goes the way its own close
        // path would drop it.
        if self.overlay.is_some() && !matches!(self.overlay, Some(Overlay::Picker(_))) {
            self.drop_overlay_state();
        }
        let picker = self.pending.as_ref().and_then(|pending| {
            let (query, candidates) = pending.ambiguous.front()?;
            let rows = variant_rows(candidates);
            Some((query.clone(), rows))
        });
        let Some((query, rows)) = picker else {
            self.confirm_pending();
            return;
        };
        self.overlay = Some(Overlay::Picker(Picker::new(
            format!("Choose a variant of \"{query}\""),
            rows,
            OnAccept::PickVariant,
        )));
        self.dirty = true;
    }

    /// Opens the non-destructive confirm naming every package the pending
    /// install sets up, with each download size and the total, and a
    /// summary of the options that differ from a plain install. The flow
    /// ends when nothing remains to install.
    fn confirm_pending(&mut self) {
        let nothing = self
            .pending
            .as_ref()
            .is_none_or(|pending| pending.targets.is_empty() && pending.picked.is_empty());
        if nothing {
            self.pending = None;
            return;
        }
        let Some(pending) = self.pending.as_ref() else {
            return;
        };
        let mut lines: Vec<Line<'static>> = Vec::new();
        for target in &pending.targets {
            lines.push(package_line(&target.package));
        }
        for package in &pending.picked {
            lines.push(package_line(package));
        }
        if let Some(options) = options_line(&pending.choices) {
            lines.push(options);
        }
        let total: u64 = pending
            .targets
            .iter()
            .map(|target| package_size(&target.package))
            .chain(pending.picked.iter().map(package_size))
            .sum();
        lines.push(Line::raw(format!("Download size: {}", format::size(total))));
        let count = pending.targets.len() + pending.picked.len();
        let title = match count {
            1 => {
                let name = pending
                    .picked
                    .first()
                    .map(|p| p.pkg_name.as_str())
                    .or_else(|| {
                        pending
                            .targets
                            .first()
                            .map(|target| target.package.pkg_name.as_str())
                    })
                    .unwrap_or("package");
                format!("Install {name}")
            }
            _ => format!("Install {count} packages"),
        };
        self.overlay = Some(Overlay::Confirm(Confirm::new(
            title,
            lines,
            "Install",
            false,
            OnAccept::InstallNow,
        )));
        self.dirty = true;
    }

    /// Records the variant the user chose for the head ambiguous query,
    /// then moves on to the next step of the flow.
    fn pick_variant(&mut self, index: usize) {
        let Some(mut pending) = self.pending.take() else {
            return;
        };
        if let Some((_, candidates)) = pending.ambiguous.pop_front()
            && let Some(package) = candidates.into_iter().nth(index)
        {
            pending.picked.push(package);
        }
        self.pending = Some(pending);
        self.advance_pending();
    }

    /// Queues the install the confirm accepted, with the marks that
    /// started it cleared.
    fn install_now(&mut self, now: Instant) -> Vec<Effect> {
        let Some(pending) = self.pending.take() else {
            return Vec::new();
        };
        self.browse.marks.clear();
        self.dirty = true;
        self.enqueue(
            JobSpec::Install {
                targets: pending.targets,
                picked: pending.picked,
                options: pending.choices,
            },
            now,
        )
    }

    /// Starts the install flow for `(key, version)` pairs: rows already
    /// installed are reported and skipped unless a reinstall was asked
    /// for, and the rest resolves into install targets. Each query pins
    /// the exact variant its catalog row shows.
    fn install_versions(
        &mut self,
        versions: Vec<(PackageKey, String)>,
        choices: InstallChoices,
        now: Instant,
    ) -> Vec<Effect> {
        let mut queries = Vec::new();
        let mut installed = Vec::new();
        for (key, version) in versions {
            let Some(row) = self.browse.row(&key) else {
                // The catalog reloaded while an overlay was open and the
                // row is gone: the query still resolves, and the
                // resolution itself reports an installed package.
                queries.push(key.query(&version));
                continue;
            };
            if row.installed && !choices.force {
                installed.push(row.key.pkg_name.clone());
            } else {
                queries.push(key.query(&version));
            }
        }
        let message = match installed.as_slice() {
            [] => None,
            [one] => {
                Some(format!(
                    "{one} is already installed. Use I and Force reinstall to reinstall."
                ))
            }
            many => {
                let count = many.len();
                Some(format!(
                    "{count} packages are already installed. Use I and Force reinstall to reinstall."
                ))
            }
        };
        if let Some(message) = message {
            self.toast(ToastKind::Info, message, now);
        }
        if queries.is_empty() {
            return Vec::new();
        }
        vec![Effect::Resolve {
            queries,
            choices,
        }]
    }

    /// Opens the install options dialog over the selected rows, marks or
    /// the cursor row. The rows are read again when the form is accepted.
    fn open_install_options(&mut self) {
        let keys = self.browse.selected_keys();
        let Some(first) = keys.first() else {
            return;
        };
        let title = if keys.len() == 1 {
            let name = &first.pkg_name;
            format!("Install {name} with options")
        } else {
            let count = keys.len();
            format!("Install {count} packages with options")
        };
        let fields = vec![
            Field::Check {
                label: "Force reinstall",
                checked: false,
            },
            Field::Check {
                label: "Binary only",
                checked: false,
            },
            Field::Check {
                label: "Skip checksum verification",
                checked: false,
            },
            Field::Check {
                label: "Portable",
                checked: false,
            },
            Field::Text {
                label: "Portable path",
                input: TextInput::default(),
            },
        ];
        self.overlay = Some(Overlay::Form(Form::new(
            title,
            fields,
            "Install",
            OnAccept::InstallWithOptions,
        )));
        self.dirty = true;
    }

    /// Builds the install choices out of the accepted options form and
    /// starts the flow for the selected rows. Force keeps already
    /// installed rows in, asking for a reinstall. An empty portable path
    /// means the default portable location.
    fn install_with_options(&mut self, values: Vec<FieldValue>, now: Instant) -> Vec<Effect> {
        let mut choices = InstallChoices::default();
        let mut portable = false;
        let mut portable_path = String::new();
        for (position, value) in values.into_iter().enumerate() {
            match (position, value) {
                (0, FieldValue::Check(force)) => choices.force = force,
                (1, FieldValue::Check(binary_only)) => choices.binary_only = binary_only,
                (2, FieldValue::Check(no_verify)) => choices.no_verify = no_verify,
                (3, FieldValue::Check(on)) => portable = on,
                (4, FieldValue::Text(path)) => portable_path = path,
                _ => {}
            }
        }
        if portable {
            choices.portable = Some(portable_path);
        }
        let versions = self.browse.selected_versions();
        self.install_versions(versions, choices, now)
    }

    /// Opens the version picker for the row under the cursor, the
    /// catalog's newest version first and the repository's older ones
    /// after it.
    fn open_version_picker(&mut self, now: Instant) {
        let Some(row) = self.browse.cursor_row() else {
            return;
        };
        if row.other_versions.is_empty() {
            let name = row.key.pkg_name.clone();
            self.toast(ToastKind::Info, format!("{name} has only one version"), now);
            return;
        }
        self.version_pick = Some((
            row.key.clone(),
            row.version.clone(),
            row.other_versions.clone(),
        ));
        let muted = self.theme.muted();
        let mut rows = vec![Line::from(vec![
            Span::raw(format!("{}  ", row.version)),
            Span::styled("latest", muted),
        ])];
        rows.extend(
            row.other_versions
                .iter()
                .map(|version| Line::raw(version.clone())),
        );
        let name = &row.key.pkg_name;
        self.overlay = Some(Overlay::Picker(Picker::new(
            format!("Versions of {name}"),
            rows,
            OnAccept::PickVersion,
        )));
        self.dirty = true;
    }

    /// Starts the install flow for the row under the cursor at the version
    /// the user picked: the first entry is the catalog's newest, the rest
    /// follow the row's other versions.
    fn install_picked_version(&mut self, index: usize, now: Instant) -> Vec<Effect> {
        let Some((key, latest, others)) = self.version_pick.take() else {
            return Vec::new();
        };
        let version = if index == 0 {
            latest
        } else {
            let Some(version) = others.get(index - 1) else {
                return Vec::new();
            };
            version.clone()
        };
        self.install_versions(vec![(key, version)], InstallChoices::default(), now)
    }

    /// Starts the install flow for the queries the prompt collected, split
    /// on whitespace. Nothing is checked against the catalog here: the
    /// resolution reports already installed packages itself.
    fn install_by_query(&mut self, text: String) -> Vec<Effect> {
        let queries: Vec<String> = text.split_whitespace().map(String::from).collect();
        if queries.is_empty() {
            return Vec::new();
        }
        vec![Effect::Resolve {
            queries,
            choices: InstallChoices::default(),
        }]
    }

    /// Opens the run prompt for the row under the cursor. Its text splits
    /// on whitespace with no shell parsing, and an empty text runs the
    /// package without arguments.
    fn open_run_prompt(&mut self) {
        let Some(row) = self.browse.cursor_row() else {
            return;
        };
        let query = match row.key.pkg_family.as_deref() {
            Some(family) => format!("{family}/{}:{}", row.key.pkg_name, row.key.repo_name),
            None => format!("{}:{}", row.key.pkg_name, row.key.repo_name),
        };
        self.run_args = Some((
            query,
            Some(row.key.repo_name.clone()),
            row.key.pkg_id.clone(),
        ));
        let name = &row.key.pkg_name;
        self.overlay = Some(Overlay::Prompt(
            Prompt::new(format!("Run {name} with arguments"), OnAccept::RunArguments).allow_empty(),
        ));
        self.dirty = true;
    }

    /// Opens the danger confirm naming the rows the removal takes: the
    /// marked rows, or the row under the cursor. Each line shows the size
    /// on disk the removal frees, and the records travel inside the
    /// accept action until the confirm is accepted. An empty selection
    /// opens nothing.
    fn open_remove_confirm(&mut self) {
        let entries = self.installed.selected_entries();
        if entries.is_empty() {
            return;
        }
        let mut lines: Vec<Line<'static>> = entries.iter().map(installed_package_line).collect();
        let total: u64 = entries.iter().map(|entry| entry.disk_size).sum();
        lines.push(Line::raw(format!("{} will be freed.", format::size(total))));
        let packages: Vec<InstalledPackage> =
            entries.into_iter().map(|entry| entry.package).collect();
        let title = match packages.as_slice() {
            [one] => format!("Remove {}", one.pkg_name),
            many => format!("Remove {} packages", many.len()),
        };
        self.overlay = Some(Overlay::Confirm(Confirm::new(
            title,
            lines,
            "Remove",
            true,
            OnAccept::Remove(packages),
        )));
        self.dirty = true;
    }

    /// Starts the switch flow for the row under the cursor: the
    /// installed variants of the row's package name load in the
    /// background, and the flow's step opens once they arrive.
    fn open_switch_flow(&mut self) -> Vec<Effect> {
        let Some(entry) = self.installed.cursor_entry() else {
            return Vec::new();
        };
        let name = entry.package.pkg_name.clone();
        vec![Effect::LoadVariants {
            name,
        }]
    }

    /// Opens the step the loaded variants ask for: an info toast when
    /// the package is not installed or has no other variant, a confirm
    /// for a sole inactive one, and a picker when several are
    /// installed. A failed load toasts the error.
    fn on_variants_loaded(
        &mut self,
        name: String,
        result: Result<Vec<VariantInfo>, String>,
        now: Instant,
    ) -> Vec<Effect> {
        let variants = match result {
            Ok(variants) => variants,
            Err(error) => {
                self.toast(ToastKind::Error, error, now);
                return Vec::new();
            }
        };
        // The load may answer long after the password dialog took the
        // overlay slot, so no step of the switch may open while a pass
        // is being authorized, and the flow waits for a retry instead.
        if self.authorizing() {
            self.toast(
                ToastKind::Info,
                "A pass is being authorized, so the switch waits. Try again once it finishes.",
                now,
            );
            return Vec::new();
        }
        match variants.as_slice() {
            [] => {
                self.toast(ToastKind::Info, format!("{name} is not installed"), now);
                Vec::new()
            }
            [only] if only.is_active => {
                self.toast(
                    ToastKind::Info,
                    format!("{name} has no other variants"),
                    now,
                );
                Vec::new()
            }
            [only] => self.confirm_switch(name, only),
            _ => {
                self.open_variant_picker(name, variants);
                Vec::new()
            }
        }
    }

    /// Opens the non-destructive confirm naming the sole inactive
    /// variant the switch links, noting that the active variant of the
    /// package moves to it.
    fn confirm_switch(&mut self, name: String, variant: &VariantInfo) -> Vec<Effect> {
        // The confirm takes the overlay slot from whatever else is
        // open, so the state that overlay kept goes the way its own
        // close path would drop it.
        if self.overlay.is_some() && !matches!(self.overlay, Some(Overlay::Confirm(_))) {
            self.drop_overlay_state();
        }
        let package = &variant.package;
        let target = match package.pkg_family.as_deref() {
            Some(family) => format!("{family}/{}", package.pkg_name),
            None => package.pkg_name.clone(),
        };
        let body = vec![
            Line::raw(format!(
                "{target}  {}  {}",
                package.version, package.repo_name
            )),
            Line::raw(format!("The active variant of {name} changes to this one.")),
        ];
        self.overlay = Some(Overlay::Confirm(Confirm::new(
            format!("Switch {} to {}", name, package.version),
            body,
            "Switch",
            false,
            OnAccept::SwitchConfirm {
                id: package.id,
                name,
            },
        )));
        self.dirty = true;
        Vec::new()
    }

    /// Opens the confirm that turns the cursor repository on or off.
    /// Disabling explains what leaves Browse; enabling explains that a
    /// sync brings its packages in, and accepting one queues the sync
    /// right after the change.
    fn open_repo_enabled_confirm(&mut self) {
        let Some(repo) = self.repos.cursor_row() else {
            return;
        };
        let name = repo.name.clone();
        let (title, body, label, enabled) = if repo.enabled {
            (
                format!("Disable {name}"),
                "Its packages will no longer appear in Browse. Installed packages stay installed.",
                "Disable",
                false,
            )
        } else {
            (
                format!("Enable {name}"),
                "Its packages will appear in Browse after the next sync.",
                "Enable",
                true,
            )
        };
        self.overlay = Some(Overlay::Confirm(Confirm::new(
            title,
            vec![Line::raw(body)],
            label,
            false,
            OnAccept::RepoEnabled {
                name,
                enabled,
            },
        )));
        self.dirty = true;
    }

    /// Opens the danger confirm that removes the cursor repository from
    /// the config file, noting that installed packages stay installed.
    fn open_repo_removed_confirm(&mut self) {
        let Some(repo) = self.repos.cursor_row() else {
            return;
        };
        let name = repo.name.clone();
        self.overlay = Some(Overlay::Confirm(Confirm::new(
            format!("Remove {name}"),
            vec![Line::raw(
                "This edits config.toml. Installed packages stay installed.",
            )],
            "Remove",
            true,
            OnAccept::RepoRemoved {
                name,
            },
        )));
        self.dirty = true;
    }

    /// Opens the form that adds a repository: a name and a URL the form
    /// insists on, an optional public key and sync interval, and two
    /// toggles that start switched on.
    fn open_add_repo_form(&mut self) {
        let fields = vec![
            Field::RequiredText {
                label: "Name",
                input: TextInput::default(),
            },
            Field::RequiredText {
                label: "URL",
                input: TextInput::default(),
            },
            Field::Text {
                label: "Public key",
                input: TextInput::default(),
            },
            Field::Check {
                label: "Enabled",
                checked: true,
            },
            Field::Check {
                label: "Desktop integration",
                checked: true,
            },
            Field::Text {
                label: "Sync interval",
                input: TextInput::default(),
            },
        ];
        self.overlay = Some(Overlay::Form(Form::new(
            "Add repository",
            fields,
            "Add",
            OnAccept::AddRepo,
        )));
        self.dirty = true;
    }

    /// Queues the addition of the repository the form collected, followed
    /// by a sync that brings its packages into Browse. Every text value is
    /// trimmed, an empty public key leaves signature verification to
    /// soar's own rule, and an empty sync interval keeps the default.
    fn add_repo(&mut self, values: Vec<FieldValue>, now: Instant) -> Vec<Effect> {
        let mut name = String::new();
        let mut url = String::new();
        let mut pubkey = String::new();
        let mut interval = String::new();
        let mut enabled = true;
        let mut desktop = true;
        for (position, value) in values.into_iter().enumerate() {
            match (position, value) {
                (0, FieldValue::Text(value)) => name = value,
                (1, FieldValue::Text(value)) => url = value,
                (2, FieldValue::Text(value)) => pubkey = value,
                (3, FieldValue::Check(on)) => enabled = on,
                (4, FieldValue::Check(on)) => desktop = on,
                (5, FieldValue::Text(value)) => interval = value,
                _ => {}
            }
        }
        let key = pubkey.trim();
        let interval = interval.trim();
        let repository = Repository {
            name: name.trim().to_string(),
            url: url.trim().to_string(),
            pubkey: (!key.is_empty()).then(|| key.to_string()),
            enabled: Some(enabled),
            desktop_integration: Some(desktop),
            signature_verification: None,
            sync_interval: (!interval.is_empty()).then(|| interval.to_string()),
        };
        let mut effects = self.enqueue(JobSpec::Repo(RepoChange::Add(repository)), now);
        effects.extend(self.enqueue(JobSpec::Sync, now));
        effects
    }

    /// Opens the picker over the installed variants, the active one
    /// marked. The variants stay in the state until one is picked or
    /// the picker closes. While a pass is being authorized the picker
    /// opens nowhere: the load may answer long after the password dialog
    /// took the overlay slot, and stealing it back would leave the pass
    /// with no dialog to serve, so the switch waits for a retry instead.
    fn open_variant_picker(&mut self, name: String, variants: Vec<VariantInfo>) {
        if self.authorizing() {
            return;
        }
        // The picker takes the overlay slot from whatever else is open,
        // so the state that overlay kept goes the way its own close
        // path would drop it.
        if self.overlay.is_some() && !matches!(self.overlay, Some(Overlay::Picker(_))) {
            self.drop_overlay_state();
        }
        let rows = variant_pick_rows(&variants, &self.theme);
        self.variant_pick = Some((name.clone(), variants));
        self.overlay = Some(Overlay::Picker(Picker::new(
            format!("Variants of {name}"),
            rows,
            OnAccept::SwitchVariant,
        )));
        self.dirty = true;
    }

    /// Links the variant the variants picker chose, when it is not the
    /// active one already. Choosing the active one only closes the
    /// flow.
    fn switch_picked_variant(&mut self, index: usize, now: Instant) -> Vec<Effect> {
        let Some((name, variants)) = self.variant_pick.take() else {
            return Vec::new();
        };
        let Some(variant) = variants.into_iter().nth(index) else {
            return Vec::new();
        };
        if variant.is_active {
            return Vec::new();
        }
        let id = variant.package.id;
        self.enqueue(
            JobSpec::Switch {
                id,
                name,
            },
            now,
        )
    }

    /// Opens the non-destructive confirm naming the updates to apply: the
    /// marked rows, or every row when none are marked. Each line shows
    /// the package moving from its current to its new version, and the
    /// keep-old choice appears when it is on. An empty list only toasts.
    /// The selected indexes stay in the state until the confirm is
    /// accepted, so closing it leaves the list untouched.
    fn open_update_confirm(&mut self, now: Instant) {
        let Some(list) = self.updates.list.ready() else {
            self.toast(ToastKind::Info, "Nothing to update", now);
            return;
        };
        if list.is_empty() {
            self.toast(ToastKind::Info, "Nothing to update", now);
            return;
        }
        let indexes: Vec<usize> = if self.updates.marks.is_empty() {
            (0..list.len()).collect()
        } else {
            let mut marks: Vec<usize> = self.updates.marks.iter().copied().collect();
            marks.sort_unstable();
            marks
        };
        let arrow = self.theme.sym().arrow;
        let mut lines: Vec<Line<'static>> = indexes
            .iter()
            .map(|&index| {
                let info = &list[index];
                Line::raw(format!(
                    "{}  {} {arrow} {}",
                    info.pkg_name, info.current_version, info.new_version
                ))
            })
            .collect();
        if self.updates.keep_old {
            lines.push(Line::raw("Keep old versions: yes"));
        }
        let title = match indexes.as_slice() {
            [one] => format!("Update {}", list[*one].pkg_name),
            many => format!("Update {} packages", many.len()),
        };
        self.update_pick = Some(indexes);
        self.overlay = Some(Overlay::Confirm(Confirm::new(
            title,
            lines,
            "Update",
            false,
            OnAccept::UpdateNow,
        )));
        self.dirty = true;
    }

    /// Queues the updates the confirm accepted: the selected entries move
    /// out of the ready list in its order, the list drops to loading
    /// because it is now missing them, and the marks clear. The fresh
    /// check the job's end starts refills the list. While a recheck has
    /// the list loading, the indexes no longer point anywhere, so the
    /// acceptance says so instead of updating nothing.
    fn update_now(&mut self, now: Instant) -> Vec<Effect> {
        let Some(indexes) = self.update_pick.take() else {
            return Vec::new();
        };
        let marked: HashSet<usize> = indexes.into_iter().collect();
        let Loadable::Ready(list) = &mut self.updates.list else {
            self.toast(
                ToastKind::Info,
                "The update list is being rechecked; try again once it finishes",
                now,
            );
            return Vec::new();
        };
        let (chosen, _rest): (Vec<_>, Vec<_>) = std::mem::take(list)
            .into_iter()
            .enumerate()
            .partition(|(index, _)| marked.contains(index));
        let updates = chosen.into_iter().map(|(_, info)| info).collect();
        self.updates.list = Loadable::Loading;
        self.updates.marks.clear();
        self.dirty = true;
        self.enqueue(
            JobSpec::Update {
                updates,
                keep_old: self.updates.keep_old,
            },
            now,
        )
    }

    /// Opens the confirm that applies the loaded plan, naming how much of
    /// it installs, updates, and removes, every removal by name, and what
    /// waits for the system pass. Danger when anything would be removed.
    /// A plan with nothing to do only notes it, and one that has not
    /// loaded opens nothing.
    fn open_apply_confirm(&mut self, now: Instant) {
        let Some(plan) = self.apply.plan.ready() else {
            return;
        };
        let Some(diff) = plan.diff.as_ref() else {
            return;
        };
        if !diff.has_changes() && !diff.has_toml_updates() && plan.system.is_empty() {
            self.toast(ToastKind::Info, "Nothing to apply", now);
            return;
        }
        let mut lines: Vec<Line<'static>> = Vec::new();
        let install = diff.to_install.len();
        if install > 0 {
            lines.push(Line::raw(format!("Install {install}")));
        }
        let update = diff.to_update.len();
        if update > 0 {
            lines.push(Line::raw(format!("Update {update}")));
        }
        let remove = diff.to_remove.len();
        if remove > 0 {
            lines.push(Line::raw(format!("Remove {remove}")));
            for package in &diff.to_remove {
                lines.push(Line::raw(package.pkg_name.clone()));
            }
        }
        if !plan.system.is_empty() {
            lines.push(Line::raw(format!(
                "System {} (asks for administrator rights)",
                plan.system.len()
            )));
        }
        // The confirm names this plan, so a fresh computation landing
        // under it can be told apart from the one the user accepted.
        self.apply_pick = Some(self.apply.check);
        self.overlay = Some(Overlay::Confirm(Confirm::new(
            "Apply packages.toml",
            lines,
            "Apply",
            remove > 0,
            OnAccept::ApplyPlan,
        )));
        self.dirty = true;
    }

    /// Queues the apply the confirm accepted. The diff moves out of the
    /// plan into the job when it still changes something, and the plan
    /// drops to loading so it recomputes once the installation changed.
    /// The packages marked `system = true` follow in their own job,
    /// applied through the administrator pass.
    fn apply_plan(&mut self, now: Instant) -> Vec<Effect> {
        self.apply_pick = None;
        let Some(plan) = self.apply.plan.ready_mut() else {
            return Vec::new();
        };
        let system = std::mem::take(&mut plan.system);
        let packages_path = plan.path.to_string_lossy().into_owned();
        let diff = plan
            .diff
            .take()
            .filter(|diff| diff.has_changes() || diff.has_toml_updates());
        self.apply.start();
        self.dirty = true;
        let mut effects = match diff {
            Some(diff) => {
                self.enqueue(
                    JobSpec::Apply {
                        diff,
                        packages_path: packages_path.clone(),
                    },
                    now,
                )
            }
            None => Vec::new(),
        };
        if !system.is_empty() {
            effects.extend(self.enqueue(
                JobSpec::ApplySystem {
                    names: system,
                    packages_path,
                    mode: SystemMode::Pending,
                },
                now,
            ));
        }
        effects
    }

    /// Opens the confirm that hands the terminal to doas for the system
    /// pass, since only doas itself can ask for its password.
    fn open_system_terminal(&mut self, tool: EscalationTool) {
        // The confirm takes the overlay slot from whatever else is
        // open, so the state that overlay kept goes the way its own
        // close path would drop it.
        if self.overlay.is_some() && !matches!(self.overlay, Some(Overlay::Confirm(_))) {
            self.drop_overlay_state();
        }
        let mut body: Vec<Line<'static>> = Vec::new();
        for row in format::wrap(
            "doas will ask for your password in the terminal. soar pauses until it finishes.",
            56,
        ) {
            body.push(Line::raw(row));
        }
        self.overlay = Some(Overlay::Confirm(Confirm::new(
            "Administrator rights needed",
            body,
            "Continue",
            false,
            OnAccept::SystemTerminal {
                tool,
            },
        )));
        self.dirty = true;
    }

    /// Starts the system pass with the terminal handed over: the job is
    /// running while the interface is suspended, and the runner reports
    /// the outcome when the pass is over.
    fn run_system_terminal(&mut self, tool: EscalationTool, now: Instant) -> Vec<Effect> {
        let Some(job) = self.authorizing_job_mut() else {
            return Vec::new();
        };
        let JobState::Authorizing = job.state else {
            return Vec::new();
        };
        if let Some(JobSpec::ApplySystem {
            mode, ..
        }) = job.spec.as_mut()
        {
            *mode = SystemMode::Terminal;
        }
        let id = job.id;
        let spec = job.spec.take();
        job.state = JobState::Running {
            started: now,
            aborting: false,
        };
        self.dirty = true;
        match spec {
            Some(JobSpec::ApplySystem {
                packages_path,
                names,
                ..
            }) => {
                vec![Effect::Suspend(Suspended::SystemPass {
                    id,
                    tool,
                    packages_path,
                    names,
                })]
            }
            _ => Vec::new(),
        }
    }

    /// Opens the non-destructive confirm naming the cache directory the
    /// clean empties, with the path taken from the loaded paths list.
    /// While the report is not loaded nothing opens, since there would be
    /// no path to name.
    fn open_clean_cache_confirm(&mut self) {
        let Some(cache) = self.health.report.ready().and_then(|(_, paths)| {
            paths
                .iter()
                .find(|(label, _)| label == "Cache")
                .map(|(_, path)| path.clone())
        }) else {
            return;
        };
        self.overlay = Some(Overlay::Confirm(Confirm::new(
            "Clean cache",
            vec![Line::raw(format::home_relative(&cache))],
            "Clean",
            false,
            OnAccept::CleanCacheConfirm,
        )));
        self.dirty = true;
    }

    /// Opens the danger confirm listing the broken symlinks the fix
    /// removes, the first ten with the rest folded into a count. Nothing
    /// broken only leaves a note, and a report that has not loaded opens
    /// nothing.
    fn open_fix_symlinks_confirm(&mut self, now: Instant) {
        let Some(symlinks) = self
            .health
            .report
            .ready()
            .map(|(report, _)| &report.broken_symlinks)
        else {
            return;
        };
        if symlinks.is_empty() {
            self.toast(ToastKind::Info, "No broken symlinks", now);
            return;
        }
        let rows: Vec<String> = symlinks
            .iter()
            .take(CONFIRM_ROWS)
            .map(|path| path.display().to_string())
            .collect();
        let count = symlinks.len();
        let title = if count == 1 {
            "Remove 1 broken symlink".to_string()
        } else {
            format!("Remove {count} broken symlinks")
        };
        self.open_health_fix_confirm(title, rows, count, OnAccept::FixSymlinksConfirm);
    }

    /// Opens the danger confirm naming each broken package and its
    /// install path, the first ten with the rest folded into a count.
    /// Nothing broken only leaves a note, and a report that has not
    /// loaded opens nothing.
    fn open_remove_broken_confirm(&mut self, now: Instant) {
        let Some(broken) = self
            .health
            .report
            .ready()
            .map(|(report, _)| &report.broken_packages)
        else {
            return;
        };
        if broken.is_empty() {
            self.toast(ToastKind::Info, "No broken packages", now);
            return;
        }
        let rows: Vec<String> = broken
            .iter()
            .take(CONFIRM_ROWS)
            .map(|package| format!("{}  {}", package.pkg_name, package.installed_path))
            .collect();
        let count = broken.len();
        let title = if count == 1 {
            "Remove 1 broken package".to_string()
        } else {
            format!("Remove {count} broken packages")
        };
        self.open_health_fix_confirm(title, rows, count, OnAccept::RemoveBrokenConfirm);
    }

    /// Opens the danger confirm carrying the rows a health fix names,
    /// with the rows past the tenth folded into a muted count.
    fn open_health_fix_confirm(
        &mut self,
        title: String,
        rows: Vec<String>,
        total: usize,
        accept: OnAccept,
    ) {
        let mut lines: Vec<Line<'static>> = rows.into_iter().map(Line::raw).collect();
        let hidden = total.saturating_sub(CONFIRM_ROWS);
        if hidden > 0 {
            lines.push(Line::styled(
                format!("{} and {hidden} more", self.theme.sym().ellipsis),
                self.theme.muted(),
            ));
        }
        self.overlay = Some(Overlay::Confirm(Confirm::new(
            title, lines, "Remove", true, accept,
        )));
        self.dirty = true;
    }

    /// Prepares the run of the row under the cursor with the arguments
    /// the prompt collected. The query names the row's package without
    /// pinning its version, so the newest variant runs and an ambiguity
    /// still gets to ask.
    fn run_with_arguments(&mut self, text: String) -> Vec<Effect> {
        let Some((query, repo, pkg_id)) = self.run_args.take() else {
            return Vec::new();
        };
        let args: Vec<String> = text.split_whitespace().map(String::from).collect();
        vec![Effect::PrepareRun {
            query,
            repo,
            pkg_id,
            args,
        }]
    }

    /// Opens the picker asking which variant of an ambiguous query to
    /// run, keeping the arguments and the candidates until one is picked.
    /// While a pass is being authorized the picker opens nowhere: the
    /// preparation may answer long after the password dialog took the
    /// overlay slot, and stealing it back would leave the pass with no
    /// dialog to serve, so the run waits for a retry instead.
    fn open_run_picker(&mut self, query: String, candidates: Vec<Package>, args: Vec<String>) {
        if self.authorizing() {
            return;
        }
        // The picker takes the overlay slot from whatever else is open,
        // so the state that overlay kept goes the way its own close
        // path would drop it.
        if self.overlay.is_some() && !matches!(self.overlay, Some(Overlay::Picker(_))) {
            self.drop_overlay_state();
        }
        let rows = variant_rows(&candidates);
        self.run_pick = Some((args, candidates));
        self.overlay = Some(Overlay::Picker(Picker::new(
            format!("Choose a variant of \"{query}\""),
            rows,
            OnAccept::RunVariant,
        )));
        self.dirty = true;
    }

    /// Prepares the run again for the exact variant the picker chose,
    /// pinning its version and package id so it cannot turn ambiguous
    /// again.
    fn run_picked_variant(&mut self, index: usize) -> Vec<Effect> {
        let Some((args, candidates)) = self.run_pick.take() else {
            return Vec::new();
        };
        let Some(package) = candidates.into_iter().nth(index) else {
            return Vec::new();
        };
        vec![Effect::PrepareRun {
            query: variant_query(&package),
            repo: Some(package.repo_name.clone()),
            pkg_id: package.pkg_id.clone(),
            args,
        }]
    }

    /// Drops every queued job and quits once the running one finishes. With
    /// `stop_running`, the running job is asked to stop first, unless it
    /// has no safe stop point: a sync and the system pass, which runs as
    /// root in another process the interface cannot stop. When nothing
    /// runs or queues anymore, the only job was waiting for its pass, so
    /// withdrawing it is the quit: the dialog goes with the job and the
    /// interface leaves at once, since no finished job remains to consume
    /// the wait.
    fn wait_and_quit(&mut self, now: Instant, stop_running: bool) -> Vec<Effect> {
        self.jobs
            .list
            .retain(|job| !matches!(job.state, JobState::Queued | JobState::Authorizing));
        self.quit_when_idle = true;
        self.dirty = true;
        if self.running_job().is_none() && self.queued_count() == 0 {
            self.overlay = None;
            self.quit = true;
            return Vec::new();
        }
        self.toast(
            ToastKind::Info,
            "Quitting after the current job finishes",
            now,
        );
        if stop_running
            && self
                .running_job()
                .is_some_and(|job| !matches!(job.kind, JobKind::Sync | JobKind::ApplySystem))
        {
            return self.abort_running();
        }
        Vec::new()
    }

    /// Marks the running job as asked to stop and returns the effect that
    /// cancels its token. A job already stopping is left alone.
    fn abort_running(&mut self) -> Vec<Effect> {
        let Some(job) = self.running_job_mut() else {
            return Vec::new();
        };
        let JobState::Running {
            aborting, ..
        } = &mut job.state
        else {
            return Vec::new();
        };
        if *aborting {
            return Vec::new();
        }
        *aborting = true;
        self.dirty = true;
        vec![Effect::AbortJob]
    }

    /// Stops the job the abort confirm was opened for, if it still runs.
    /// The confirm names the job running when it opened, and another job
    /// may have taken its place by the time the answer comes, so only the
    /// named one may be stopped. A job that finished meanwhile only gets
    /// a note, and nothing is cancelled.
    fn abort_confirmed(&mut self, now: Instant) -> Vec<Effect> {
        let Some(id) = self.abort_pick.take() else {
            return Vec::new();
        };
        if self.running_job().is_some_and(|job| job.id == id) {
            return self.abort_running();
        }
        let title = self
            .jobs
            .list
            .iter()
            .find(|job| job.id == id)
            .map(|job| job.title.clone());
        let note = match title {
            Some(title) => format!("{title} already finished"),
            None => "That job already finished".to_string(),
        };
        self.toast(ToastKind::Info, note, now);
        Vec::new()
    }

    /// Retires an open abort confirm whose job just finished, mirroring
    /// how a fresh update check retires the update confirm: whatever
    /// runs next was never named by it, so it must not stay open.
    fn retire_abort_confirm(&mut self, id: JobId) {
        if self.abort_pick != Some(id) {
            return;
        }
        self.abort_pick = None;
        if let Some(Overlay::Confirm(confirm)) = &self.overlay
            && matches!(confirm.action(), OnAccept::Abort)
        {
            self.overlay = None;
        }
    }

    /// Asks to stop the running job. Nothing running says so, a sync has
    /// no safe stop point, the system pass runs as root in another
    /// process the interface cannot stop cleanly, and a job already
    /// stopping is left alone. Anything else opens the confirm naming
    /// what an abort keeps.
    fn request_abort(&mut self, now: Instant) {
        let Some(job) = self.running_job() else {
            self.toast(ToastKind::Info, "Nothing is running", now);
            return;
        };
        if job.kind == JobKind::Sync {
            self.toast(ToastKind::Warning, "Sync cannot be aborted", now);
            return;
        }
        if job.kind == JobKind::ApplySystem {
            self.toast(ToastKind::Info, "System apply cannot be aborted", now);
            return;
        }
        if matches!(
            job.state,
            JobState::Running {
                aborting: true,
                ..
            }
        ) {
            return;
        }
        let title = job.title.clone();
        self.abort_pick = Some(job.id);
        self.overlay = Some(Overlay::Confirm(Confirm::new(
            format!("Abort {title}?"),
            vec![
                Line::raw("Packages that already finished stay installed."),
                Line::raw("Downloads in progress are kept and resume next time."),
            ],
            "Abort",
            true,
            OnAccept::Abort,
        )));
        self.dirty = true;
    }

    /// Finds what a pressed key means and carries its action out. While a
    /// view's filter holds focus, the keys go to it first: Enter keeps the
    /// text and returns focus to the list, Esc clears the text and returns
    /// focus, and the keys it does not use, such as the arrows, reach the
    /// view.
    fn on_key(&mut self, key: KeyEvent, now: Instant) -> Vec<Effect> {
        if self.view == ViewId::Browse && self.browse.filter_focused {
            return self.on_filter_key(key, now);
        }
        if self.view == ViewId::Installed && self.installed.filter_focused {
            return self.on_installed_filter_key(key, now);
        }
        if let Some(action) = keymap::lookup(self.view, key) {
            return self.on_action(action, now);
        }
        Vec::new()
    }

    /// Feeds one pressed key to the catalog filter while it holds focus,
    /// rebuilding the visible rows on every change, then lets the keys the
    /// input does not use move the cursor or act on the view.
    fn on_filter_key(&mut self, key: KeyEvent, now: Instant) -> Vec<Effect> {
        match key.code {
            KeyCode::Enter => {
                self.browse.filter_focused = false;
                self.dirty = true;
                return Vec::new();
            }
            KeyCode::Esc => {
                self.browse.filter.clear();
                self.browse.filter_focused = false;
                self.browse.refilter(now);
                self.dirty = true;
                return Vec::new();
            }
            _ => {}
        }
        if self.browse.filter.handle(key) {
            self.browse.refilter(now);
            self.dirty = true;
            return Vec::new();
        }
        if let Some(action) = keymap::lookup(self.view, key) {
            return self.on_action(action, now);
        }
        Vec::new()
    }

    /// Feeds one pressed key to the installed list's filter while it holds
    /// focus, rebuilding the visible rows on every change, then lets the
    /// keys the input does not use move the cursor or act on the view.
    fn on_installed_filter_key(&mut self, key: KeyEvent, now: Instant) -> Vec<Effect> {
        match key.code {
            KeyCode::Enter => {
                self.installed.filter_focused = false;
                self.dirty = true;
                return Vec::new();
            }
            KeyCode::Esc => {
                self.installed.filter.clear();
                self.installed.filter_focused = false;
                self.installed.refilter();
                self.dirty = true;
                return Vec::new();
            }
            _ => {}
        }
        if self.installed.filter.handle(key) {
            self.installed.refilter();
            self.dirty = true;
            return Vec::new();
        }
        if let Some(action) = keymap::lookup(self.view, key) {
            return self.on_action(action, now);
        }
        Vec::new()
    }

    /// Handles one pointer event. The wheel scrolls the list under the
    /// pointer and a left click switches tabs or moves the list cursor;
    /// with an overlay open only a left click outside a confirm or picker
    /// does anything, closing it the way esc does.
    fn on_mouse(&mut self, mouse: MouseEvent, now: Instant) -> Vec<Effect> {
        if self.overlay.is_some() {
            return self.on_overlay_mouse(mouse, now);
        }
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => self.on_click(mouse, now),
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => self.on_wheel(mouse, now),
            _ => Vec::new(),
        }
    }

    /// The pointer events an open overlay reacts to: a left click outside
    /// a confirm or picker closes it through the same close path as the
    /// esc key, so everything the close arranges still happens. Every
    /// other pointer event is ignored.
    fn on_overlay_mouse(&mut self, mouse: MouseEvent, now: Instant) -> Vec<Effect> {
        let dismiss = matches!(
            &self.overlay,
            Some(Overlay::Confirm(_) | Overlay::Picker(_))
        ) && mouse.kind == MouseEventKind::Down(MouseButton::Left)
            && !self
                .layout
                .overlay
                .is_some_and(|rect| rect.contains(Position::new(mouse.column, mouse.row)));
        if !dismiss {
            return Vec::new();
        }
        self.overlay = None;
        self.dirty = true;
        self.on_overlay_close(now)
    }

    /// One left click's hit test: a header tab switches views through the
    /// same path the number keys take, and a row of the list under the
    /// pointer moves the cursor onto itself, or toggles the detail when
    /// the cursor already sat there.
    fn on_click(&mut self, mouse: MouseEvent, now: Instant) -> Vec<Effect> {
        let position = Position::new(mouse.column, mouse.row);
        if let Some((_, view)) = self
            .layout
            .tabs
            .iter()
            .find(|(rect, _)| rect.contains(position))
        {
            return self.on_action(Action::View(*view), now);
        }
        let Some((rect, offset)) = self.layout.list else {
            return Vec::new();
        };
        if !rect.contains(position) {
            return Vec::new();
        }
        let index = offset + usize::from(mouse.row - rect.y);
        if index >= self.list_len() {
            return Vec::new();
        }
        if self.cursor_selected() == index {
            return self.on_action(Action::ToggleDetail, now);
        }
        self.move_cursor_to(index, now);
        Vec::new()
    }

    /// The wheel: scrolls the pane under the pointer by the wheel's row
    /// count, three rows per notch, the way the keyboard movement keys
    /// move it. A detail or log pane under the pointer scrolls itself
    /// whatever pane the keyboard focus sits on.
    fn on_wheel(&mut self, mouse: MouseEvent, now: Instant) -> Vec<Effect> {
        let position = Position::new(mouse.column, mouse.row);
        let down = mouse.kind == MouseEventKind::ScrollDown;
        let action = if down { Action::Down } else { Action::Up };
        if self.view == ViewId::Jobs && self.layout.log.is_some_and(|rect| rect.contains(position))
        {
            let saved = self.jobs_view.focus;
            self.jobs_view.focus = JobsFocus::Log;
            for _ in 0..WHEEL_ROWS {
                self.scroll_jobs(action);
            }
            self.jobs_view.focus = saved;
            return Vec::new();
        }
        if self
            .layout
            .detail
            .is_some_and(|rect| rect.contains(position))
        {
            match self.view {
                ViewId::Browse if self.browse.detail_open => {
                    for _ in 0..WHEEL_ROWS {
                        self.scroll_browse(action, now);
                    }
                }
                ViewId::Installed if self.installed.detail_open => {
                    for _ in 0..WHEEL_ROWS {
                        self.scroll_installed(action);
                    }
                }
                ViewId::Jobs => {
                    let saved = self.jobs_view.focus;
                    self.jobs_view.focus = JobsFocus::Detail;
                    for _ in 0..WHEEL_ROWS {
                        self.scroll_jobs(action);
                    }
                    self.jobs_view.focus = saved;
                }
                _ => {}
            }
            return Vec::new();
        }
        let Some((rect, _)) = self.layout.list else {
            return Vec::new();
        };
        if !rect.contains(position) {
            return Vec::new();
        }
        for _ in 0..WHEEL_ROWS {
            match self.view {
                ViewId::Browse => self.scroll_browse(action, now),
                ViewId::Installed => self.scroll_installed(action),
                ViewId::Updates => self.scroll_updates(action),
                ViewId::Repos => self.scroll_repos(action),
                ViewId::Health => self.scroll_health(action),
                ViewId::Apply => self.scroll_apply(action),
                ViewId::Jobs => {
                    let len = self.jobs.list.len();
                    if down {
                        self.jobs_view.cursor.down(1, len);
                    } else {
                        self.jobs_view.cursor.up(1);
                    }
                }
            }
        }
        self.dirty = true;
        Vec::new()
    }

    /// The number of rows the focused view's list holds, zero before its
    /// data loads.
    fn list_len(&self) -> usize {
        match self.view {
            ViewId::Browse => self.browse.visible.len(),
            ViewId::Installed => self.installed.visible.len(),
            ViewId::Updates => self.updates.ready_len(),
            ViewId::Apply => self.apply.body_rows,
            ViewId::Repos => self.repos.ready_len(),
            ViewId::Health => self.health.paths_len(),
            ViewId::Jobs => self.jobs.list.len(),
        }
    }

    /// The row the focused view's list cursor sits on.
    fn cursor_selected(&self) -> usize {
        match self.view {
            ViewId::Browse => self.browse.cursor.selected,
            ViewId::Installed => self.installed.cursor.selected,
            ViewId::Updates => self.updates.cursor.selected,
            ViewId::Apply => self.apply.cursor.selected,
            ViewId::Repos => self.repos.cursor.selected,
            ViewId::Health => self.health.cursor.selected,
            ViewId::Jobs => self.jobs_view.cursor.selected,
        }
    }

    /// Moves the focused view's list cursor to `index`, leaving the view
    /// exactly as the keyboard movement keys would.
    fn move_cursor_to(&mut self, index: usize, now: Instant) {
        match self.view {
            ViewId::Browse => {
                self.browse.cursor.selected = index;
                self.browse.note_cursor(now);
            }
            ViewId::Installed => self.installed.cursor.selected = index,
            ViewId::Updates => self.updates.cursor.selected = index,
            ViewId::Apply => self.apply.cursor.selected = index,
            ViewId::Repos => self.repos.cursor.selected = index,
            ViewId::Health => self.health.cursor.selected = index,
            ViewId::Jobs => self.jobs_view.cursor.selected = index,
        }
        self.dirty = true;
    }

    /// Carries out one action. View-local actions act on the view's own
    /// state; the global ones act on the interface as a whole.
    fn on_action(&mut self, action: Action, now: Instant) -> Vec<Effect> {
        match action {
            Action::View(view) => return self.switch_to(view, now),
            Action::NextView => return self.switch_to(self.view.next(), now),
            Action::PrevView => return self.switch_to(self.view.prev(), now),
            Action::Quit => self.on_quit(),
            Action::Help => {
                self.overlay = Some(Overlay::Help(Help::new(self.view)));
                self.dirty = true;
            }
            Action::Escape => {
                if self.view == ViewId::Browse {
                    self.escape_browse(now);
                } else if self.view == ViewId::Installed {
                    self.escape_installed();
                } else if self.view == ViewId::Updates && self.updates.scope.is_some() {
                    return self.escape_updates_scope(now);
                } else if self.view == ViewId::Jobs && self.jobs_view.focus != JobsFocus::List {
                    self.jobs_view.focus = JobsFocus::List;
                    self.dirty = true;
                }
            }
            Action::FocusDetail => {
                if self.view == ViewId::Jobs && !self.jobs.list.is_empty() {
                    self.jobs_view.focus = JobsFocus::Detail;
                    self.dirty = true;
                }
            }
            Action::FocusLog => {
                if self.view == ViewId::Jobs {
                    self.jobs_view.focus = JobsFocus::Log;
                    self.dirty = true;
                }
            }
            Action::ClearFinished => {
                if self.view == ViewId::Jobs {
                    self.jobs
                        .list
                        .retain(|job| !matches!(job.state, JobState::Finished { .. }));
                    self.jobs_view.cursor.clamp(self.jobs.list.len());
                    self.dirty = true;
                }
            }
            Action::Sync if self.view == ViewId::Repos => {
                if self.jobs.list.iter().any(|job| {
                    job.kind == JobKind::Sync && !matches!(job.state, JobState::Finished { .. })
                }) {
                    self.toast(ToastKind::Info, "Sync already pending", now);
                } else {
                    return self.enqueue(JobSpec::Sync, now);
                }
            }
            Action::ToggleEnabled if self.view == ViewId::Repos => {
                self.open_repo_enabled_confirm();
            }
            Action::RemoveRepo if self.view == ViewId::Repos => {
                self.open_repo_removed_confirm();
            }
            Action::AddRepo if self.view == ViewId::Repos => {
                self.open_add_repo_form();
            }
            Action::EditConfig if self.view == ViewId::Repos => {
                if self.running_job().is_some() {
                    self.toast(
                        ToastKind::Error,
                        "Wait for the running job to finish before editing config",
                        now,
                    );
                } else {
                    self.dirty = true;
                    return vec![Effect::Suspend(Suspended::Edit(
                        self.env.config_path.clone(),
                    ))];
                }
            }
            Action::CleanCache if self.view == ViewId::Health => {
                self.open_clean_cache_confirm();
            }
            Action::FixSymlinks if self.view == ViewId::Health => {
                self.open_fix_symlinks_confirm(now);
            }
            Action::RemoveBroken if self.view == ViewId::Health => {
                self.open_remove_broken_confirm(now);
            }
            Action::Down
            | Action::Up
            | Action::Top
            | Action::Bottom
            | Action::PageDown
            | Action::PageUp => {
                if self.view == ViewId::Browse {
                    self.scroll_browse(action, now);
                } else if self.view == ViewId::Installed {
                    self.scroll_installed(action);
                } else if self.view == ViewId::Updates {
                    self.scroll_updates(action);
                } else if self.view == ViewId::Repos {
                    self.scroll_repos(action);
                } else if self.view == ViewId::Health {
                    self.scroll_health(action);
                } else if self.view == ViewId::Apply {
                    self.scroll_apply(action);
                } else {
                    self.scroll_jobs(action);
                }
            }
            Action::AbortJob => self.request_abort(now),
            Action::AbortSelected => self.abort_selected(now),
            Action::Filter if self.view == ViewId::Browse => {
                self.browse.filter_focused = true;
                self.dirty = true;
            }
            Action::Filter if self.view == ViewId::Installed => {
                self.installed.filter_focused = true;
                self.dirty = true;
            }
            Action::ToggleMark if self.view == ViewId::Browse => {
                self.browse.toggle_mark(now);
                self.dirty = true;
            }
            Action::ToggleMark if self.view == ViewId::Installed => {
                self.installed.toggle_mark();
                self.dirty = true;
            }
            Action::ToggleMark if self.view == ViewId::Updates => {
                if !self.updates_table_hidden() {
                    self.updates.toggle_mark();
                    self.dirty = true;
                }
            }
            Action::MarkAll if self.view == ViewId::Updates => {
                if !self.updates_table_hidden() {
                    self.updates.mark_all();
                    self.dirty = true;
                }
            }
            Action::ToggleKeepOld if self.view == ViewId::Updates => {
                self.updates.keep_old = !self.updates.keep_old;
                self.dirty = true;
            }
            Action::ToggleSkipped if self.view == ViewId::Updates => {
                self.updates.show_skipped = !self.updates.show_skipped;
                self.dirty = true;
            }
            Action::Update if self.view == ViewId::Updates => {
                if self.updates_table_hidden() {
                    self.toast(ToastKind::Info, "Press S to return to the list", now);
                } else {
                    self.open_update_confirm(now);
                }
            }
            Action::UpdateSelected if self.view == ViewId::Installed => {
                return self.check_selected_for_updates(now);
            }
            Action::TogglePrune if self.view == ViewId::Apply => {
                self.apply.prune = !self.apply.prune;
                self.dirty = true;
                return vec![self.apply_effect()];
            }
            Action::Apply if self.view == ViewId::Apply => {
                self.open_apply_confirm(now);
            }
            Action::EditPackages if self.view == ViewId::Apply => {
                if self.running_job().is_some() {
                    self.toast(
                        ToastKind::Error,
                        "Wait for the running job to finish before editing packages.toml",
                        now,
                    );
                } else {
                    let path = self
                        .apply
                        .plan
                        .ready()
                        .map(|plan| plan.path.clone())
                        .unwrap_or_else(|| self.env.packages_path.clone());
                    self.dirty = true;
                    return vec![Effect::Suspend(Suspended::Edit(path))];
                }
            }
            Action::CreatePackages if self.view == ViewId::Apply => {
                let missing = self
                    .apply
                    .plan
                    .ready()
                    .is_some_and(|plan| plan.diff.is_none());
                if missing {
                    return self.enqueue(JobSpec::CreatePackagesConfig, now);
                }
            }
            Action::CycleRepo if self.view == ViewId::Browse => {
                self.browse.cycle_repo(now);
                self.dirty = true;
            }
            Action::CycleSort if self.view == ViewId::Installed => {
                self.installed.cycle_sort();
                self.dirty = true;
            }
            Action::ToggleDetail if self.view == ViewId::Browse => self.toggle_detail(),
            Action::ToggleDetail if self.view == ViewId::Installed => {
                self.installed.toggle_detail();
                self.dirty = true;
            }
            Action::Remove if self.view == ViewId::Installed => {
                self.open_remove_confirm();
            }
            Action::SwitchVariant if self.view == ViewId::Installed => {
                return self.open_switch_flow();
            }
            Action::Install if self.view == ViewId::Browse => {
                let versions = self.browse.selected_versions();
                return self.install_versions(versions, InstallChoices::default(), now);
            }
            Action::InstallWithOptions if self.view == ViewId::Browse => {
                self.open_install_options();
            }
            Action::PickVersion if self.view == ViewId::Browse => {
                self.open_version_picker(now);
            }
            Action::InstallByQuery if self.view == ViewId::Browse => {
                self.overlay = Some(Overlay::Prompt(Prompt::new(
                    "Install by name, URL, or path",
                    OnAccept::InstallByQuery,
                )));
                self.dirty = true;
            }
            Action::RunPackage if self.view == ViewId::Browse => {
                self.open_run_prompt();
            }
            Action::Reload if self.view == ViewId::Browse => {
                self.browse.catalog = Loadable::Loading;
                self.dirty = true;
                return vec![self.catalog_effect()];
            }
            Action::Reload if self.view == ViewId::Installed => {
                self.installed.list = Loadable::Loading;
                self.dirty = true;
                return vec![self.installed_effect()];
            }
            Action::Reload if self.view == ViewId::Updates => {
                let check = self.check_effect(self.updates.scope.clone(), now);
                self.dirty = true;
                return vec![check];
            }
            Action::Reload if self.view == ViewId::Apply => {
                self.dirty = true;
                return vec![self.apply_effect()];
            }
            Action::Reload if self.view == ViewId::Repos => {
                self.repos.start();
                self.dirty = true;
                return vec![Effect::LoadRepos];
            }
            Action::Reload if self.view == ViewId::Health => {
                self.health.start();
                self.dirty = true;
                return vec![Effect::LoadHealth];
            }
            _ => {}
        }
        Vec::new()
    }

    /// The catalog's esc: closes the full-screen detail first, then clears
    /// the filter, then the repository filter, then clears the marks, one
    /// press each.
    fn escape_browse(&mut self, now: Instant) {
        if self.browse.detail_open {
            self.browse.detail_open = false;
            self.dirty = true;
        } else if !self.browse.filter.is_empty() {
            self.browse.filter.clear();
            self.browse.refilter(now);
            self.dirty = true;
        } else if self.browse.repo_filter.is_some() {
            self.browse.repo_filter = None;
            self.browse.refilter(now);
            self.dirty = true;
        } else if !self.browse.marks.is_empty() {
            self.browse.marks.clear();
            self.dirty = true;
        }
    }

    /// The installed list's esc: closes the full-screen detail first, then
    /// clears the filter, then clears the marks, one press each.
    fn escape_installed(&mut self) {
        if self.installed.detail_open {
            self.installed.detail_open = false;
            self.dirty = true;
        } else if !self.installed.filter.is_empty() {
            self.installed.filter.clear();
            self.installed.refilter();
            self.dirty = true;
        } else if !self.installed.marks.is_empty() {
            self.installed.marks.clear();
            self.dirty = true;
        }
    }

    /// Starts a check and builds the effect that runs it, numbered for
    /// this start, so a result from a check a newer start replaced is
    /// dropped when it arrives. A check that starts while the update
    /// confirm is open closes it first: its picks are indexes into a
    /// list the fresh check is about to replace.
    fn check_effect(&mut self, names: Option<Vec<String>>, now: Instant) -> Effect {
        if self.update_pick.take().is_some() {
            self.close_update_confirm();
            self.toast(
                ToastKind::Info,
                "The update list is being rechecked, so the confirm closed",
                now,
            );
        }
        self.updates.start_check();
        Effect::CheckUpdates {
            names,
            check: self.updates.check,
        }
    }

    /// Starts a catalog load and builds the effect that runs it, numbered
    /// for this start, so a reply from a load a newer start replaced is
    /// dropped when it arrives.
    fn catalog_effect(&mut self) -> Effect {
        self.catalog_load = self.catalog_load.wrapping_add(1);
        Effect::LoadCatalog {
            check: self.catalog_load,
        }
    }

    /// Starts an installed list load and builds the effect that runs it,
    /// numbered like the catalog's so a replaced load's reply is dropped.
    fn installed_effect(&mut self) -> Effect {
        self.installed_load = self.installed_load.wrapping_add(1);
        Effect::LoadInstalled {
            check: self.installed_load,
        }
    }

    /// Starts a plan computation and builds the effect that runs it,
    /// numbered for this start, so a result from a computation a newer
    /// start replaced is dropped when it arrives.
    fn apply_effect(&mut self) -> Effect {
        self.apply.start();
        Effect::ComputeApplyDiff {
            prune: self.apply.prune,
            check: self.apply.check,
        }
    }

    /// Drops an open update confirm whose list a fresh check replaced, so
    /// accepting it cannot update rows that moved under it.
    fn close_update_confirm(&mut self) {
        if let Some(Overlay::Confirm(confirm)) = &self.overlay
            && matches!(confirm.action(), OnAccept::UpdateNow)
        {
            self.overlay = None;
        }
    }

    /// Drops an open apply confirm whose plan a fresh computation
    /// replaced, so accepting it cannot queue removals the confirm never
    /// named.
    fn close_apply_confirm(&mut self) {
        if let Some(Overlay::Confirm(confirm)) = &self.overlay
            && matches!(confirm.action(), OnAccept::ApplyPlan)
        {
            self.overlay = None;
        }
    }

    /// Drops the updates scope and checks everything again, so esc backs
    /// the view out to the full list.
    fn escape_updates_scope(&mut self, now: Instant) -> Vec<Effect> {
        self.updates.scope = None;
        let check = self.check_effect(None, now);
        self.dirty = true;
        vec![check]
    }

    /// Moves to the updates view and checks only the packages selected in
    /// the installed list: marked rows, or the row under the cursor.
    /// Duplicate names collapse to one check, keeping first order.
    fn check_selected_for_updates(&mut self, now: Instant) -> Vec<Effect> {
        let mut names: Vec<String> = Vec::new();
        for entry in self.installed.selected_entries() {
            let name = entry.package.pkg_name;
            if !names.contains(&name) {
                names.push(name);
            }
        }
        if names.is_empty() {
            return Vec::new();
        }
        self.updates.scope = Some(names.clone());
        let check = self.check_effect(Some(names), now);
        let mut effects = self.switch_to(ViewId::Updates, now);
        effects.push(check);
        effects
    }

    /// Opens the full-screen detail on the row under the cursor, or closes
    /// it back to the list.
    fn toggle_detail(&mut self) {
        if self.browse.detail_open {
            self.browse.detail_open = false;
        } else if self.browse.cursor_row().is_some() {
            self.browse.detail_open = true;
            self.browse.detail.top();
        }
        self.dirty = true;
    }

    /// How many rows a page key moves: half the current view's main list
    /// height, or ten rows when no list is on screen. The main list is
    /// the view's own, even where a page key scrolls another pane, such
    /// as the jobs view's detail or log.
    fn page_rows(&self) -> usize {
        self.layout
            .list
            .map_or(10, |(rect, _)| usize::from(rect.height / 2).max(1))
    }

    /// Moves the catalog cursor, or scrolls the detail pane while it fills
    /// the screen. The drawn window follows at render time.
    fn scroll_browse(&mut self, action: Action, now: Instant) {
        let page = self.page_rows();
        if self.browse.detail_open {
            let len = self.browse.detail_rows;
            match action {
                Action::Down => self.browse.detail.down(1, len),
                Action::Up => self.browse.detail.up(1),
                Action::PageDown => self.browse.detail.down(page, len),
                Action::PageUp => self.browse.detail.up(page),
                Action::Top => self.browse.detail.top(),
                _ => self.browse.detail.bottom(len),
            }
        } else {
            let len = self.browse.visible.len();
            match action {
                Action::Down => self.browse.cursor.down(1, len),
                Action::Up => self.browse.cursor.up(1),
                Action::PageDown => self.browse.cursor.down(page, len),
                Action::PageUp => self.browse.cursor.up(page),
                Action::Top => self.browse.cursor.top(),
                _ => self.browse.cursor.bottom(len),
            }
            self.browse.note_cursor(now);
        }
        self.dirty = true;
    }

    /// Moves the installed list's cursor, or scrolls the detail pane while
    /// it fills the screen. The drawn window follows at render time.
    fn scroll_installed(&mut self, action: Action) {
        let page = self.page_rows();
        if self.installed.detail_open {
            let len = self.installed.detail_rows;
            match action {
                Action::Down => self.installed.detail.down(1, len),
                Action::Up => self.installed.detail.up(1),
                Action::PageDown => self.installed.detail.down(page, len),
                Action::PageUp => self.installed.detail.up(page),
                Action::Top => self.installed.detail.top(),
                _ => self.installed.detail.bottom(len),
            }
        } else {
            let len = self.installed.visible.len();
            match action {
                Action::Down => self.installed.cursor.down(1, len),
                Action::Up => self.installed.cursor.up(1),
                Action::PageDown => self.installed.cursor.down(page, len),
                Action::PageUp => self.installed.cursor.up(page),
                Action::Top => self.installed.cursor.top(),
                _ => self.installed.cursor.bottom(len),
            }
        }
        self.dirty = true;
    }

    /// Whether the skipped list replaces the updates table, so keys
    /// aimed at its rows must not reach them.
    fn updates_table_hidden(&self) -> bool {
        self.updates.show_skipped && !self.updates.skipped.is_empty()
    }

    /// Moves the updates list's cursor. The drawn window follows at render
    /// time, and the cursor clamps to the empty list while it loads.
    fn scroll_updates(&mut self, action: Action) {
        if self.updates_table_hidden() {
            return;
        }
        let page = self.page_rows();
        let len = self.updates.ready_len();
        match action {
            Action::Down => self.updates.cursor.down(1, len),
            Action::Up => self.updates.cursor.up(1),
            Action::PageDown => self.updates.cursor.down(page, len),
            Action::PageUp => self.updates.cursor.up(page),
            Action::Top => self.updates.cursor.top(),
            _ => self.updates.cursor.bottom(len),
        }
        self.dirty = true;
    }

    /// Moves the repositories list's cursor. The drawn window follows at
    /// render time, and the cursor clamps to the empty list while it
    /// loads.
    fn scroll_repos(&mut self, action: Action) {
        let page = self.page_rows();
        let len = self.repos.ready_len();
        match action {
            Action::Down => self.repos.cursor.down(1, len),
            Action::Up => self.repos.cursor.up(1),
            Action::PageDown => self.repos.cursor.down(page, len),
            Action::PageUp => self.repos.cursor.up(page),
            Action::Top => self.repos.cursor.top(),
            _ => self.repos.cursor.bottom(len),
        }
        self.dirty = true;
    }

    /// Moves the paths pane's cursor over the loaded paths list. The
    /// drawn window follows at render time.
    fn scroll_health(&mut self, action: Action) {
        let page = self.page_rows();
        let len = self.health.paths_len();
        match action {
            Action::Down => self.health.cursor.down(1, len),
            Action::Up => self.health.cursor.up(1),
            Action::PageDown => self.health.cursor.down(page, len),
            Action::PageUp => self.health.cursor.up(page),
            Action::Top => self.health.cursor.top(),
            _ => self.health.cursor.bottom(len),
        }
        self.dirty = true;
    }

    /// Moves the plan body's cursor. The drawn window follows at render
    /// time, and the length comes from the last frame the body drew.
    fn scroll_apply(&mut self, action: Action) {
        let page = self.page_rows();
        let len = self.apply.body_rows;
        match action {
            Action::Down => self.apply.cursor.down(1, len),
            Action::Up => self.apply.cursor.up(1),
            Action::PageDown => self.apply.cursor.down(page, len),
            Action::PageUp => self.apply.cursor.up(page),
            Action::Top => self.apply.cursor.top(),
            _ => self.apply.cursor.bottom(len),
        }
        self.dirty = true;
    }

    /// Carries the Jobs view's abort key out on the job under the cursor:
    /// the running one goes through the same confirm as the global key, a
    /// queued one leaves the list, and a finished one stays.
    fn abort_selected(&mut self, now: Instant) {
        let Some(job) = self.selected_job() else {
            self.toast(ToastKind::Info, "Nothing is running", now);
            return;
        };
        if matches!(job.state, JobState::Running { .. }) {
            self.request_abort(now);
            return;
        }
        if !matches!(job.state, JobState::Queued) {
            return;
        }
        let pos = self.jobs.list.len() - 1 - self.jobs_view.cursor.selected;
        let title = self.jobs.list[pos].title.clone();
        self.jobs.list.remove(pos);
        self.jobs_view.cursor.clamp(self.jobs.list.len());
        self.dirty = true;
        self.toast(
            ToastKind::Info,
            format!("Removed {title} from the queue"),
            now,
        );
    }

    /// The job the Jobs view points at. The list shows newest first, so the
    /// cursor counts from the end.
    fn selected_job(&self) -> Option<&Job> {
        let len = self.jobs.list.len();
        let selected = self.jobs_view.cursor.selected;
        (selected < len)
            .then(|| len - 1 - selected)
            .and_then(|pos| self.jobs.list.get(pos))
    }

    /// Scrolls the pane the jobs view holds focus on. Moving up or to the
    /// top stops the log from following its end; the bottom key resumes it.
    fn scroll_jobs(&mut self, action: Action) {
        if self.view != ViewId::Jobs {
            return;
        }
        match self.jobs_view.focus {
            JobsFocus::List => {
                let page = self.page_rows();
                let len = self.jobs.list.len();
                match action {
                    Action::Down => self.jobs_view.cursor.down(1, len),
                    Action::Up => self.jobs_view.cursor.up(1),
                    Action::PageDown => self.jobs_view.cursor.down(page, len),
                    Action::PageUp => self.jobs_view.cursor.up(page),
                    Action::Top => self.jobs_view.cursor.top(),
                    _ => self.jobs_view.cursor.bottom(len),
                }
            }
            JobsFocus::Detail => {
                let page = self.page_rows();
                let len = self.jobs_view.detail_rows;
                match action {
                    Action::Down => self.jobs_view.detail.down(1, len),
                    Action::Up => self.jobs_view.detail.up(1),
                    Action::PageDown => self.jobs_view.detail.down(page, len),
                    Action::PageUp => self.jobs_view.detail.up(page),
                    Action::Top => self.jobs_view.detail.top(),
                    _ => self.jobs_view.detail.bottom(len),
                }
            }
            JobsFocus::Log => {
                let page = self.page_rows();
                let len = self.log.lines.len();
                match action {
                    Action::Down => {
                        self.log.follow = false;
                        self.log.cursor.down(1, len);
                    }
                    Action::Up => {
                        self.log.follow = false;
                        self.log.cursor.up(1);
                    }
                    Action::PageDown => {
                        self.log.follow = false;
                        self.log.cursor.down(page, len);
                    }
                    Action::PageUp => {
                        self.log.follow = false;
                        self.log.cursor.up(page);
                    }
                    Action::Top => {
                        self.log.follow = false;
                        self.log.cursor.top();
                    }
                    _ => {
                        self.log.follow = true;
                        self.log.cursor.bottom(len);
                    }
                }
            }
        }
        self.dirty = true;
    }

    /// Handles a quit request: with a job running, the guard dialog; with a
    /// quit already pending, the force-quit confirm; otherwise quit now. A
    /// queued job always has a running job ahead of it, so one check
    /// covers both.
    fn on_quit(&mut self) {
        if self.quit_when_idle {
            self.overlay = Some(Overlay::Confirm(Confirm::new(
                "Quit",
                vec![Line::raw(
                    "Force quit now? The running job stops wherever it is.",
                )],
                "Force quit",
                true,
                OnAccept::QuitForce,
            )));
        } else if let Some(job) = self.running_job().or_else(|| self.authorizing_job()) {
            let title = job.title.clone();
            let queued = self.queued_count();
            self.overlay = Some(Overlay::QuitGuard(QuitGuard::new(&title, queued)));
        } else {
            self.quit = true;
        }
        self.dirty = true;
    }

    /// Shows one view, starts the work the view asks for on entering, and
    /// marks the screen for a redraw. Opening the Jobs view clears the
    /// unseen-failure flag, since the failure is now seen.
    fn switch_to(&mut self, view: ViewId, now: Instant) -> Vec<Effect> {
        if view == ViewId::Jobs {
            self.jobs.attention = false;
        }
        self.view = view;
        let effects = self.on_enter_view(view, now);
        self.dirty = true;
        effects
    }

    /// The work the view starts when it comes on screen. Views that load
    /// their data on the first visit request it here, so one switch shows
    /// the view and starts its load. Later views use it too.
    fn on_enter_view(&mut self, view: ViewId, now: Instant) -> Vec<Effect> {
        match view {
            ViewId::Updates if self.updates.list.is_idle() => {
                vec![self.check_effect(None, now)]
            }
            ViewId::Apply if self.apply.plan.is_idle() => vec![self.apply_effect()],
            ViewId::Repos if self.repos.list.is_idle() => {
                self.repos.start();
                vec![Effect::LoadRepos]
            }
            ViewId::Health if self.health.report.is_idle() => {
                self.health.start();
                vec![Effect::LoadHealth]
            }
            _ => Vec::new(),
        }
    }
}

/// One picker row per candidate package: `family/name  version  repo
/// type`, without the family when there is none.
fn variant_rows(candidates: &[Package]) -> Vec<Line<'static>> {
    candidates
        .iter()
        .map(|package| {
            let name = match package.pkg_family.as_deref() {
                Some(family) => format!("{family}/{}", package.pkg_name),
                None => package.pkg_name.clone(),
            };
            let kind = package.pkg_type.as_deref().unwrap_or("");
            Line::raw(format!(
                "{name}  {}  {}  {kind}",
                package.version, package.repo_name
            ))
        })
        .collect()
}

/// One picker row per installed variant: `family/name  version  repo
/// type  size`, without the family when there is none, and the active
/// one marked.
fn variant_pick_rows(variants: &[VariantInfo], theme: &Theme) -> Vec<Line<'static>> {
    variants
        .iter()
        .map(|variant| {
            let package = &variant.package;
            let name = match package.pkg_family.as_deref() {
                Some(family) => format!("{family}/{}", package.pkg_name),
                None => package.pkg_name.clone(),
            };
            let kind = package.pkg_type.as_deref().unwrap_or("");
            let mut spans = vec![Span::raw(format!(
                "{name}  {}  {}  {kind}  {}",
                package.version,
                package.repo_name,
                format::size(package.size)
            ))];
            if variant.is_active {
                spans.push(Span::styled(
                    format!(" {} active", theme.sym().marked),
                    theme.success(),
                ));
            }
            Line::from(spans)
        })
        .collect()
}

/// The exact query naming one package variant, `family/name@version:repo`,
/// without the family when there is none.
fn variant_query(package: &Package) -> String {
    match package.pkg_family.as_deref() {
        Some(family) => {
            format!(
                "{family}/{}@{}:{}",
                package.pkg_name, package.version, package.repo_name
            )
        }
        None => {
            format!(
                "{}@{}:{}",
                package.pkg_name, package.version, package.repo_name
            )
        }
    }
}

/// One confirm line for a package: `name  version  repo  size`, without
/// the size when the repository gives none.
fn package_line(package: &Package) -> Line<'static> {
    let mut text = format!(
        "{}  {}  {}",
        package.pkg_name, package.version, package.repo_name
    );
    if package.ghcr_size.or(package.size).is_some() {
        let bytes = package_size(package);
        text.push_str(&format!("  {}", format::size(bytes)));
    }
    Line::raw(text)
}

/// One confirm line for an installed package: `name  version  repo
/// size`, with the size on disk that removing it frees.
fn installed_package_line(entry: &InstalledEntry) -> Line<'static> {
    Line::raw(format!(
        "{}  {}  {}  {}",
        entry.package.pkg_name,
        entry.package.version,
        entry.package.repo_name,
        format::size(entry.disk_size)
    ))
}

/// The download size shown for a package: the registry image size when
/// the repository gives one, else the artifact size, else nothing.
fn package_size(package: &Package) -> u64 {
    package.ghcr_size.or(package.size).unwrap_or(0)
}

/// The line summarizing the options that differ from a plain install, or
/// `None` when every choice is the default.
fn options_line(choices: &InstallChoices) -> Option<Line<'static>> {
    let mut parts: Vec<String> = Vec::new();
    if choices.force {
        parts.push("force".to_string());
    }
    if choices.binary_only {
        parts.push("binary only".to_string());
    }
    if choices.no_verify {
        parts.push("skip verification".to_string());
    }
    match choices.portable.as_deref() {
        Some("") => parts.push("portable".to_string()),
        Some(path) => parts.push(format!("portable: {path}")),
        None => {}
    }
    (!parts.is_empty()).then(|| {
        let summary = parts.join(", ");
        Line::raw(format!("Options: {summary}"))
    })
}

#[cfg(test)]
mod tests {
    use std::{
        path::{Path, PathBuf},
        time::{Duration, Instant},
    };

    use ratatui::{
        crossterm::event::{
            Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
            MouseEventKind,
        },
        text::Line,
    };
    use soar_core::{database::models::Package, package::install::InstallTarget};
    use soar_events::{SoarEvent, SyncStage};
    use soar_operations::{
        AmbiguousPackage, ApplyDiff, BrokenPackage, HealthReport, InstalledListResult,
        PrepareRunResult, UpdateInfo, VariantInfo,
        privilege::{EscalationKind, EscalationTool},
    };

    use super::{App, AppEnv};
    use crate::{
        action::Action,
        effect::{Effect, Suspended},
        jobs::{
            InstallChoices, JobId, JobKind, JobOutcome, JobSpec, JobState, OutcomeLine, RepoChange,
            SystemMode,
        },
        loadable::Loadable,
        msg::{Loaded, Msg, Resolution},
        privileged::EscalationProbe,
        progress::Phase,
        test_support::{
            apply_plan, catalog_row, code, ctrl, health_paths, health_report, install_entry,
            installed_entry, installed_package, key, package, screen, update_info,
        },
        theme::Theme,
        views::{ViewId, apply::ApplyPlan, browse::CatalogRow, repos::RepoRow},
        widgets::{
            confirm::Confirm,
            overlay::{OnAccept, Overlay},
            picker::Picker,
            text_input::Prompt,
            toast::ToastKind,
        },
    };

    fn app() -> App {
        App::new(
            AppEnv {
                version: String::new(),
                profile: String::new(),
                system: false,
                config_path: PathBuf::from("/tmp/soar-test-config.toml"),
                packages_path: PathBuf::from("/tmp/soar-test-packages.toml"),
            },
            Theme::new(true, true),
        )
    }

    /// A catalog reply for the load the app still expects before any
    /// reload has started.
    fn catalog_loaded(rows: Vec<CatalogRow>) -> Msg {
        Msg::Loaded(Loaded::Catalog {
            check: 0,
            result: Ok(rows),
        })
    }

    /// A repositories list, loaded.
    fn repos_loaded(rows: Vec<RepoRow>) -> Msg {
        Msg::Loaded(Loaded::Repos(Ok(rows)))
    }

    /// One repository row with the given enabled flag, never synced.
    fn repo_row(name: &str, enabled: bool) -> RepoRow {
        RepoRow {
            name: name.to_string(),
            url: format!("https://example.com/{name}.db"),
            enabled,
            signed: true,
            desktop: true,
            sync_interval: "3h".to_string(),
            synced: None,
        }
    }

    /// An installed list over the given entries, answering the load the
    /// app still expects before any reload has started.
    fn installed_loaded(entries: Vec<soar_operations::InstalledEntry>) -> Msg {
        let total_count = entries.len();
        let total_size = entries.iter().map(|entry| entry.disk_size).sum();
        Msg::Loaded(Loaded::Installed {
            check: 0,
            result: Ok(InstalledListResult {
                packages: entries,
                total_count,
                total_size,
            }),
        })
    }

    /// A variants result for one package name.
    fn variants_loaded(name: &str, variants: Vec<VariantInfo>) -> Msg {
        Msg::Loaded(Loaded::Variants {
            name: name.to_string(),
            result: Box::new(Ok(variants)),
        })
    }

    /// One installed variant of a package, with its database id.
    fn variant(id: u64, name: &str, version: &str, repo: &str, active: bool) -> VariantInfo {
        let mut package = installed_package(name, version, repo);
        package.id = id;
        VariantInfo {
            package,
            is_active: active,
        }
    }

    /// A resolve result with the default choices.
    fn resolved(list: Vec<Resolution>) -> Msg {
        Msg::Loaded(Loaded::Resolved {
            choices: InstallChoices::default(),
            result: Ok(list),
        })
    }

    /// An install target for one package.
    fn target(package: Package) -> InstallTarget {
        InstallTarget {
            package,
            ..Default::default()
        }
    }

    /// The one resolve the effects hold, failing the test otherwise.
    fn resolve_of(effects: &[Effect]) -> &Vec<String> {
        let [
            Effect::Resolve {
                queries, ..
            },
        ] = effects
        else {
            panic!("expected one resolve");
        };
        queries
    }

    fn press(code: KeyCode, modifiers: KeyModifiers, kind: KeyEventKind) -> Msg {
        Msg::Input(Event::Key(KeyEvent::new_with_kind(code, modifiers, kind)))
    }

    #[test]
    fn pressing_three_selects_updates() {
        let mut app = app();
        app.update(
            press(KeyCode::Char('3'), KeyModifiers::NONE, KeyEventKind::Press),
            Instant::now(),
        );
        assert_eq!(app.view, ViewId::Updates);
    }

    #[test]
    fn tab_from_jobs_selects_browse() {
        let mut app = app();
        app.view = ViewId::Jobs;
        app.update(
            press(KeyCode::Tab, KeyModifiers::NONE, KeyEventKind::Press),
            Instant::now(),
        );
        assert_eq!(app.view, ViewId::Browse);
    }

    #[test]
    fn q_sets_quit() {
        let mut app = app();
        app.update(
            press(KeyCode::Char('q'), KeyModifiers::NONE, KeyEventKind::Press),
            Instant::now(),
        );
        assert!(app.quit);
    }

    #[test]
    fn release_kind_changes_nothing() {
        let mut app = app();
        app.update(
            press(
                KeyCode::Char('q'),
                KeyModifiers::NONE,
                KeyEventKind::Release,
            ),
            Instant::now(),
        );
        assert!(!app.quit);
        assert_eq!(app.view, ViewId::Browse);
    }

    #[test]
    fn startup_sets_the_catalog_loading_and_returns_the_load_effect() {
        let mut app = app();
        let effects = app.startup();
        assert!(matches!(
            effects.as_slice(),
            [Effect::LoadCatalog { .. }, Effect::LoadInstalled { .. }]
        ));
        assert!(app.browse.catalog.is_loading());
        assert!(app.installed.list.is_loading());
    }

    #[test]
    fn a_loaded_catalog_replaces_the_view_state() {
        let mut app = app();
        app.update(
            Msg::Loaded(Loaded::Catalog {
                check: 0,
                result: Ok(Vec::new()),
            }),
            Instant::now(),
        );
        assert!(app.browse.catalog.ready().is_some());
        assert!(app.dirty);
        app.dirty = false;
        app.update(
            Msg::Loaded(Loaded::Catalog {
                check: 0,
                result: Err("offline".to_string()),
            }),
            Instant::now(),
        );
        assert!(app.browse.catalog.ready().is_none());
        assert!(app.dirty);
    }

    #[test]
    fn the_first_catalog_load_quietly_starts_an_update_check() {
        let mut app = app();
        let effects = app.update(catalog_loaded(Vec::new()), Instant::now());
        assert!(matches!(
            effects.as_slice(),
            [Effect::CheckUpdates {
                names: None,
                ..
            }]
        ));
        assert!(app.updates.list.is_loading());
    }

    #[test]
    fn page_keys_move_by_half_the_drawn_list() {
        let rows: Vec<CatalogRow> = (0..40)
            .map(|n| catalog_row(&format!("pkg{n}"), "1.0.0", "bincache"))
            .collect();
        let mut app = app();
        app.update(catalog_loaded(rows), Instant::now());
        let now = Instant::now();
        screen(&mut app, 80, 24);
        assert_eq!(app.browse.cursor.selected, 0);
        let page = app.page_rows();
        let rows = app.layout.list.expect("the list drew").0.height as usize;
        assert_eq!(page, (rows / 2).max(1));
        app.update(code(KeyCode::PageDown), now);
        assert_eq!(app.browse.cursor.selected, page);
        app.update(ctrl('d'), now);
        assert_eq!(app.browse.cursor.selected, page * 2);
        app.update(code(KeyCode::PageUp), now);
        assert_eq!(app.browse.cursor.selected, page);
        app.update(ctrl('u'), now);
        assert_eq!(app.browse.cursor.selected, 0);
    }

    #[test]
    fn later_catalog_loads_start_no_further_check() {
        let mut app = app();
        app.update(catalog_loaded(Vec::new()), Instant::now());
        let effects = app.update(catalog_loaded(Vec::new()), Instant::now());
        assert!(effects.is_empty());
    }

    #[test]
    fn a_failed_catalog_load_starts_no_check() {
        let mut app = app();
        let effects = app.update(
            Msg::Loaded(Loaded::Catalog {
                check: 0,
                result: Err("offline".to_string()),
            }),
            Instant::now(),
        );
        assert!(effects.is_empty());
        assert!(app.updates.list.is_idle());
    }

    #[test]
    fn ctrl_r_on_browse_reloads_the_catalog() {
        let mut app = app();
        let effects = app.update(
            press(
                KeyCode::Char('r'),
                KeyModifiers::CONTROL,
                KeyEventKind::Press,
            ),
            Instant::now(),
        );
        assert!(matches!(effects.as_slice(), [Effect::LoadCatalog { .. }]));
        assert!(app.browse.catalog.is_loading());
    }

    #[test]
    fn ctrl_r_on_another_view_does_nothing_yet() {
        let mut app = app();
        app.view = ViewId::Jobs;
        let effects = app.update(
            press(
                KeyCode::Char('r'),
                KeyModifiers::CONTROL,
                KeyEventKind::Press,
            ),
            Instant::now(),
        );
        assert!(effects.is_empty());
    }

    #[test]
    fn an_open_overlay_captures_every_key() {
        let mut app = app();
        app.overlay = Some(Overlay::Confirm(Confirm::new(
            "Remove 2 packages",
            Vec::new(),
            "Remove",
            true,
            OnAccept::Test,
        )));
        app.update(
            press(KeyCode::Char('q'), KeyModifiers::NONE, KeyEventKind::Press),
            Instant::now(),
        );
        assert!(!app.quit);
        assert!(app.overlay.is_some());
        app.update(
            press(KeyCode::Char('y'), KeyModifiers::NONE, KeyEventKind::Press),
            Instant::now(),
        );
        assert!(app.overlay.is_none());
    }

    #[test]
    fn help_opens_and_captures_keys_until_it_closes() {
        let mut app = app();
        app.update(
            press(KeyCode::Char('?'), KeyModifiers::NONE, KeyEventKind::Press),
            Instant::now(),
        );
        assert!(matches!(app.overlay, Some(Overlay::Help(_))));
        app.update(
            press(KeyCode::Char('1'), KeyModifiers::NONE, KeyEventKind::Press),
            Instant::now(),
        );
        assert_eq!(app.view, ViewId::Browse);
        app.update(
            press(KeyCode::Esc, KeyModifiers::NONE, KeyEventKind::Press),
            Instant::now(),
        );
        assert!(app.overlay.is_none());
    }

    #[test]
    fn a_toast_marks_the_screen_dirty() {
        let mut app = app();
        app.dirty = false;
        app.toast(ToastKind::Info, "installed fd", Instant::now());
        assert!(app.dirty);
        assert_eq!(app.toasts.iter().count(), 1);
    }

    #[test]
    fn a_tick_expires_toasts_and_marks_the_screen_dirty() {
        let mut app = app();
        let now = Instant::now();
        app.toast(ToastKind::Info, "installed fd", now);
        app.dirty = false;
        app.update(Msg::Tick, now + Duration::from_secs(5));
        assert!(app.dirty);
        assert_eq!(app.toasts.iter().count(), 0);
    }

    fn finished(id: JobId, ok: bool, headline: &str) -> Msg {
        Msg::JobFinished {
            id,
            outcome: JobOutcome {
                ok,
                aborted: false,
                headline: headline.to_string(),
                lines: Vec::new(),
            },
        }
    }

    /// An install of one package by name, for the abort flows.
    fn install_spec(name: &str) -> JobSpec {
        JobSpec::Install {
            targets: Vec::new(),
            picked: vec![package(name, "1.0.0", "bincache")],
            options: InstallChoices::default(),
        }
    }

    #[test]
    fn enqueueing_two_jobs_while_idle_starts_only_the_first() {
        let mut app = app();
        let now = Instant::now();
        let effects = app.enqueue(JobSpec::Sync, now);
        assert!(matches!(
            effects.as_slice(),
            [Effect::StartJob {
                id: 0,
                spec: JobSpec::Sync
            }]
        ));
        assert!(app.enqueue(JobSpec::CleanCache, now).is_empty());
        assert!(matches!(app.jobs.list[0].state, JobState::Running { .. }));
        assert!(matches!(app.jobs.list[1].state, JobState::Queued));
    }

    #[test]
    fn enqueue_refuses_new_work_once_a_wait_and_quit_is_pending() {
        let mut app = app();
        let now = Instant::now();
        app.enqueue(install_spec("bat"), now);
        app.quit_when_idle = true;
        let effects = app.enqueue(install_spec("fd"), now);
        assert!(effects.is_empty());
        assert_eq!(app.jobs.list.len(), 1, "no job may join a quitting queue");
        let toasts = format!("{:?}", app.toasts);
        assert!(
            toasts.contains("Quitting soon, so no new work is accepted"),
            "toasts are {toasts:?}"
        );
    }

    #[test]
    fn nothing_starts_while_a_job_waits_for_authorization() {
        let mut app = app();
        let now = Instant::now();
        app.enqueue(JobSpec::CleanCache, now);
        app.jobs.list[0].state = JobState::Authorizing;
        let effects = app.enqueue(JobSpec::Sync, now);
        assert!(effects.is_empty());
        assert!(matches!(app.jobs.list[1].state, JobState::Queued));
        // Once the wait is over the queue moves again.
        app.jobs.list.remove(0);
        let effects = app.enqueue(JobSpec::CleanCache, now);
        assert!(matches!(
            effects.as_slice(),
            [Effect::StartJob {
                id: 1,
                spec: JobSpec::Sync
            }]
        ));
    }

    #[test]
    fn finishing_a_job_starts_the_next_queued_one() {
        let mut app = app();
        let now = Instant::now();
        app.enqueue(JobSpec::CleanCache, now);
        app.enqueue(JobSpec::Sync, now);
        let effects = app.update(finished(0, true, "Cache cleaned"), now);
        assert!(matches!(
            effects.as_slice(),
            [
                Effect::LoadHealth,
                Effect::StartJob {
                    id: 1,
                    spec: JobSpec::Sync
                }
            ]
        ));
        assert!(app.health.report.is_loading());
        assert!(matches!(app.jobs.list[1].state, JobState::Running { .. }));
        assert_eq!(app.toasts.iter().count(), 1);
    }

    #[test]
    fn finishing_a_sync_rebuilds_the_context_before_the_next_job_starts() {
        let mut app = app();
        let now = Instant::now();
        app.enqueue(JobSpec::Sync, now);
        app.enqueue(JobSpec::CleanCache, now);
        let effects = app.update(finished(0, true, "Repositories synced"), now);
        assert!(matches!(effects[0], Effect::RebuildContext));
        assert!(matches!(effects[1], Effect::LoadCatalog { .. }));
        assert!(matches!(effects[2], Effect::LoadInstalled { .. }));
        assert!(matches!(effects[3], Effect::LoadRepos));
        assert!(matches!(
            effects[4..],
            [Effect::StartJob {
                id: 1,
                spec: JobSpec::CleanCache
            }]
        ));
    }

    #[test]
    fn pressing_s_once_enqueues_one_sync_and_the_second_presses_only_toast() {
        let mut app = app();
        let now = Instant::now();
        app.view = ViewId::Repos;
        let effects = app.update(key('s'), now);
        assert!(matches!(
            effects.as_slice(),
            [Effect::StartJob {
                spec: JobSpec::Sync,
                ..
            }]
        ));
        let effects = app.update(key('s'), now);
        assert!(effects.is_empty());
        assert_eq!(app.toasts.iter().count(), 1);
        assert!(format!("{:?}", app.toasts).contains("Sync already pending"));
    }

    #[test]
    fn pressing_e_on_repos_suspends_to_edit_the_config() {
        let mut app = app();
        let now = Instant::now();
        app.view = ViewId::Repos;
        let effects = app.update(key('E'), now);
        let [Effect::Suspend(Suspended::Edit(path))] = &effects[..] else {
            panic!("expected a suspend");
        };
        assert_eq!(path, &app.env.config_path);
        assert!(app.dirty);
    }

    #[test]
    fn pressing_e_while_a_job_runs_toasts_instead_of_editing() {
        let mut app = app();
        let now = Instant::now();
        app.view = ViewId::Repos;
        app.enqueue(JobSpec::Sync, now);
        let effects = app.update(key('E'), now);
        assert!(effects.is_empty());
        assert_eq!(app.toasts.iter().count(), 1);
        assert!(format!("{:?}", app.toasts).contains("Wait for the running job"));
    }

    #[test]
    fn finishing_a_repository_change_reloads_the_catalog() {
        let mut app = app();
        let now = Instant::now();
        app.enqueue(
            JobSpec::Repo(RepoChange::SetEnabled {
                name: "pkgforge".to_string(),
                enabled: false,
            }),
            now,
        );
        let effects = app.update(finished(0, true, "Repository pkgforge disabled"), now);
        assert!(matches!(
            effects.as_slice(),
            [
                Effect::RebuildContext,
                Effect::LoadRepos,
                Effect::LoadCatalog { .. },
                Effect::LoadInstalled { .. }
            ]
        ));
    }

    #[test]
    fn editing_the_config_reloads_the_context_and_the_views() {
        let mut app = app();
        let now = Instant::now();
        let effects = app.update(Msg::Edited(app.env.config_path.clone()), now);
        assert!(matches!(
            effects.as_slice(),
            [
                Effect::RebuildContext,
                Effect::LoadRepos,
                Effect::LoadCatalog { .. },
                Effect::LoadInstalled { .. }
            ]
        ));
        assert!(app.dirty);
    }

    #[test]
    fn editing_another_file_only_redraws() {
        let mut app = app();
        let now = Instant::now();
        let effects = app.update(Msg::Edited(PathBuf::from("/tmp/other.toml")), now);
        assert!(effects.is_empty());
        assert!(app.dirty);
    }

    #[test]
    fn n_with_an_existing_packages_file_does_nothing() {
        let mut app = app();
        let now = Instant::now();
        app.view = ViewId::Apply;
        app.update(apply_loaded(apply_plan()), now);
        let effects = app.update(key('n'), now);
        assert!(effects.is_empty());
    }

    #[test]
    fn n_with_a_missing_packages_file_enqueues_the_create_job() {
        let mut app = app();
        let now = Instant::now();
        app.view = ViewId::Apply;
        let mut missing = apply_plan();
        missing.diff = None;
        app.update(apply_loaded(missing), now);
        let effects = app.update(key('n'), now);
        assert!(matches!(
            effects.as_slice(),
            [Effect::StartJob {
                spec: JobSpec::CreatePackagesConfig,
                ..
            }]
        ));
    }

    #[test]
    fn e_opens_the_packages_file_in_the_editor() {
        let mut app = app();
        let now = Instant::now();
        app.view = ViewId::Apply;
        let plan = apply_plan();
        let path = plan.path.clone();
        app.update(apply_loaded(plan), now);
        let effects = app.update(key('e'), now);
        assert!(matches!(
            effects.as_slice(),
            [Effect::Suspend(Suspended::Edit(edited))] if *edited == path
        ));
    }

    #[test]
    fn e_before_the_plan_loads_uses_the_startup_path() {
        let mut app = app();
        let now = Instant::now();
        app.view = ViewId::Apply;
        let effects = app.update(key('e'), now);
        assert!(matches!(
            effects.as_slice(),
            [Effect::Suspend(Suspended::Edit(edited))] if *edited == app.env.packages_path
        ));
    }

    #[test]
    fn editing_the_packages_file_recomputes_the_plan() {
        let mut app = app();
        let now = Instant::now();
        let effects = app.update(Msg::Edited(app.env.packages_path.clone()), now);
        assert!(matches!(
            effects.as_slice(),
            [Effect::ComputeApplyDiff {
                prune: false,
                ..
            }]
        ));
        assert!(app.apply.plan.is_loading());
    }

    #[test]
    fn the_create_hint_shows_only_for_a_missing_file() {
        let mut app = app();
        app.view = ViewId::Apply;
        app.update(apply_loaded(apply_plan()), Instant::now());
        assert!(!app.hint_visible(&Action::CreatePackages));
        let mut missing = apply_plan();
        missing.diff = None;
        app.update(apply_loaded(missing), Instant::now());
        assert!(app.hint_visible(&Action::CreatePackages));
    }

    #[test]
    fn e_on_an_enabled_repo_disables_it_after_a_confirm() {
        let mut app = app();
        let now = Instant::now();
        app.view = ViewId::Repos;
        app.update(repos_loaded(vec![repo_row("bincache", true)]), now);
        app.update(key('e'), now);
        let screen = screen(&mut app, 80, 24);
        assert!(
            screen.contains("Its packages will no longer appear in Browse."),
            "screen is {screen:?}"
        );
        let effects = app.update(key('y'), now);
        assert!(matches!(
            effects.as_slice(),
            [Effect::StartJob {
                spec: JobSpec::Repo(RepoChange::SetEnabled {
                    enabled: false,
                    ..
                }),
                ..
            }]
        ));
    }

    #[test]
    fn e_on_a_disabled_repo_enables_it_and_queues_a_sync() {
        let mut app = app();
        let now = Instant::now();
        app.view = ViewId::Repos;
        app.update(repos_loaded(vec![repo_row("bincache", false)]), now);
        app.update(key('e'), now);
        let effects = app.update(key('y'), now);
        assert!(matches!(
            effects.as_slice(),
            [Effect::StartJob {
                spec: JobSpec::Repo(RepoChange::SetEnabled {
                    enabled: true,
                    ..
                }),
                ..
            }]
        ));
        let queued = app
            .jobs
            .list
            .iter()
            .filter(|job| matches!(job.state, JobState::Queued) && job.kind == JobKind::Sync)
            .count();
        assert_eq!(queued, 1);
    }

    #[test]
    fn d_on_a_repo_removes_it_after_a_danger_confirm() {
        let mut app = app();
        let now = Instant::now();
        app.view = ViewId::Repos;
        app.update(repos_loaded(vec![repo_row("bincache", true)]), now);
        app.update(key('d'), now);
        let screen = screen(&mut app, 80, 24);
        assert!(
            screen.contains("This edits config.toml."),
            "screen is {screen:?}"
        );
        let effects = app.update(key('y'), now);
        assert!(matches!(
            effects.as_slice(),
            [Effect::StartJob {
                spec: JobSpec::Repo(RepoChange::Remove(name)),
                ..
            }] if name == "bincache"
        ));
    }

    #[test]
    fn a_on_repos_opens_the_add_repository_form() {
        let mut app = app();
        let now = Instant::now();
        app.view = ViewId::Repos;
        app.update(repos_loaded(vec![repo_row("bincache", true)]), now);
        app.update(key('a'), now);
        let screen = screen(&mut app, 80, 24);
        for text in [
            "Add repository",
            "Name",
            "URL",
            "Public key",
            "Desktop integration",
            "Sync interval",
            "[ Add ]",
        ] {
            assert!(screen.contains(text), "screen is {screen:?}");
        }
    }

    #[test]
    fn accepting_the_add_form_queues_the_repository_and_a_sync() {
        let mut app = app();
        let now = Instant::now();
        app.view = ViewId::Repos;
        app.update(repos_loaded(vec![repo_row("bincache", true)]), now);
        app.update(key('a'), now);
        app.update(
            press(KeyCode::Char('x'), KeyModifiers::NONE, KeyEventKind::Press),
            now,
        );
        app.update(
            press(KeyCode::Tab, KeyModifiers::NONE, KeyEventKind::Press),
            now,
        );
        for c in "https://example.com/m.json".chars() {
            app.update(
                press(KeyCode::Char(c), KeyModifiers::NONE, KeyEventKind::Press),
                now,
            );
        }
        for _ in 0..6 {
            app.update(
                press(KeyCode::Tab, KeyModifiers::NONE, KeyEventKind::Press),
                now,
            );
        }
        let effects = app.update(
            press(KeyCode::Enter, KeyModifiers::NONE, KeyEventKind::Press),
            now,
        );
        assert!(matches!(
            effects.as_slice(),
            [Effect::StartJob {
                spec: JobSpec::Repo(RepoChange::Add(repo)),
                ..
            }] if repo.name == "x"
                && repo.url == "https://example.com/m.json"
                && repo.pubkey.is_none()
                && repo.signature_verification.is_none()
                && repo.enabled == Some(true)
                && repo.desktop_integration == Some(true)
                && repo.sync_interval.is_none()
        ));
        let queued = app
            .jobs
            .list
            .iter()
            .filter(|job| matches!(job.state, JobState::Queued) && job.kind == JobKind::Sync)
            .count();
        assert_eq!(queued, 1);
    }

    #[test]
    fn a_failed_job_marks_attention_until_the_jobs_view_opens() {
        let mut app = app();
        let now = Instant::now();
        app.enqueue(JobSpec::Sync, now);
        app.update(finished(0, false, "Sync failed"), now);
        assert!(app.jobs.attention);
        assert!(app.jobs_attention());
        app.update(
            press(KeyCode::Char('7'), KeyModifiers::NONE, KeyEventKind::Press),
            now,
        );
        assert!(!app.jobs.attention);
        assert!(!app.jobs_attention());
    }

    #[test]
    fn download_progress_while_a_job_runs_updates_that_jobs_tracker() {
        let mut app = app();
        let now = Instant::now();
        app.enqueue(JobSpec::Sync, now);
        app.dirty = false;
        app.update(
            Msg::Soar(SoarEvent::DownloadProgress {
                op_id: 7,
                pkg_name: "bat".to_string(),
                current: 250,
                total: 1000,
            }),
            now,
        );
        assert!(app.dirty);
        let job = app.running_job().unwrap();
        assert_eq!(job.tracker.ops.len(), 1);
        assert_eq!(job.tracker.ops[0].name, "bat");
        assert_eq!(
            job.tracker.ops[0].phase,
            Phase::Downloading {
                current: 250,
                total: 1000
            }
        );
        assert!(app.background.ops.is_empty());
    }

    #[test]
    fn soar_events_without_a_running_job_reach_the_background_tracker() {
        let mut app = app();
        let now = Instant::now();
        app.update(
            Msg::Soar(SoarEvent::SyncProgress {
                repo_name: "bincache".to_string(),
                stage: SyncStage::Fetching,
            }),
            now,
        );
        assert_eq!(
            app.background.sync,
            vec![("bincache".to_string(), "fetching".to_string())]
        );
        assert!(app.background.ops.is_empty());
    }

    #[test]
    fn q_with_a_running_job_opens_the_guard_instead_of_quitting() {
        let mut app = app();
        let now = Instant::now();
        app.enqueue(JobSpec::Sync, now);
        app.update(
            press(KeyCode::Char('q'), KeyModifiers::NONE, KeyEventKind::Press),
            now,
        );
        assert!(!app.quit);
        assert!(matches!(app.overlay, Some(Overlay::QuitGuard(_))));
    }

    #[test]
    fn waiting_and_quitting_finishes_the_job_then_quits() {
        let mut app = app();
        let now = Instant::now();
        app.enqueue(JobSpec::Sync, now);
        app.jobs.list.push(crate::jobs::Job {
            id: 1,
            kind: crate::jobs::JobKind::Sync,
            title: "Sync repositories".into(),
            state: JobState::Queued,
            spec: None,
            tracker: Default::default(),
        });
        app.update(
            press(KeyCode::Char('q'), KeyModifiers::NONE, KeyEventKind::Press),
            now,
        );
        // Right moves to wait and quit, enter accepts it.
        for code in [KeyCode::Right, KeyCode::Enter] {
            app.update(press(code, KeyModifiers::NONE, KeyEventKind::Press), now);
        }
        assert!(app.quit_when_idle);
        assert_eq!(app.queued_count(), 0);
        assert!(format!("{:?}", app.toasts).contains("Quitting after the current job finishes"));
        assert!(!app.quit);
        app.update(finished(0, true, "Synced 3 repositories"), now);
        assert!(app.quit);
    }

    #[test]
    fn q_while_waiting_offers_force_quit() {
        let mut app = app();
        let now = Instant::now();
        app.enqueue(JobSpec::Sync, now);
        app.update(
            press(KeyCode::Char('q'), KeyModifiers::NONE, KeyEventKind::Press),
            now,
        );
        for code in [KeyCode::Right, KeyCode::Enter] {
            app.update(press(code, KeyModifiers::NONE, KeyEventKind::Press), now);
        }
        assert!(app.quit_when_idle);
        app.update(
            press(KeyCode::Char('q'), KeyModifiers::NONE, KeyEventKind::Press),
            now,
        );
        assert!(matches!(app.overlay, Some(Overlay::Confirm(_))));
        assert!(!app.quit);
        app.update(
            press(KeyCode::Char('y'), KeyModifiers::NONE, KeyEventKind::Press),
            now,
        );
        assert!(app.quit);
    }

    #[test]
    fn ctrl_x_with_a_running_install_opens_the_abort_confirm() {
        let mut app = app();
        let now = Instant::now();
        app.enqueue(install_spec("bat"), now);
        app.update(
            press(
                KeyCode::Char('x'),
                KeyModifiers::CONTROL,
                KeyEventKind::Press,
            ),
            now,
        );
        assert!(matches!(app.overlay, Some(Overlay::Confirm(_))));
        let screen = screen(&mut app, 80, 24);
        assert!(
            screen.contains("Abort Install bat?"),
            "screen is {screen:?}"
        );
        assert!(
            screen.contains("Packages that already finished stay installed."),
            "screen is {screen:?}"
        );
    }

    #[test]
    fn accepting_the_abort_confirm_sets_aborting_and_returns_the_effect() {
        let mut app = app();
        let now = Instant::now();
        app.enqueue(install_spec("bat"), now);
        app.update(ctrl('x'), now);
        let effects = app.update(key('y'), now);
        assert!(matches!(effects.as_slice(), [Effect::AbortJob]));
        assert!(app.overlay.is_none());
        assert!(matches!(
            app.running_job().map(|job| &job.state),
            Some(JobState::Running {
                aborting: true,
                ..
            })
        ));
        app.update(ctrl('x'), now);
        assert!(app.overlay.is_none());
    }

    #[test]
    fn the_abort_confirm_retires_when_its_job_finishes() {
        let mut app = app();
        let now = Instant::now();
        app.enqueue(install_spec("bat"), now);
        app.update(ctrl('x'), now);
        assert!(matches!(app.overlay, Some(Overlay::Confirm(_))));
        assert_eq!(app.abort_pick, Some(0));
        app.enqueue(install_spec("fd"), now);
        app.update(finished(0, true, "Installed bat 1.0.0"), now);
        assert!(app.overlay.is_none(), "the confirm cannot outlive its job");
        assert!(app.abort_pick.is_none());
        assert!(matches!(app.jobs.list[1].state, JobState::Running { .. }));
    }

    #[test]
    fn accepting_a_retired_abort_confirm_cannot_stop_the_next_job() {
        let mut app = app();
        let now = Instant::now();
        app.enqueue(install_spec("bat"), now);
        app.update(ctrl('x'), now);
        app.enqueue(install_spec("fd"), now);
        app.update(finished(0, true, "Installed bat 1.0.0"), now);
        let effects = app.update(key('y'), now);
        assert!(effects.is_empty());
        assert!(matches!(
            app.running_job().map(|job| &job.state),
            Some(JobState::Running {
                aborting: false,
                ..
            })
        ));
    }

    #[test]
    fn ctrl_x_with_a_running_sync_says_it_cannot_be_aborted() {
        let mut app = app();
        let now = Instant::now();
        app.enqueue(JobSpec::Sync, now);
        let effects = app.update(
            press(
                KeyCode::Char('x'),
                KeyModifiers::CONTROL,
                KeyEventKind::Press,
            ),
            now,
        );
        assert!(effects.is_empty());
        assert!(app.overlay.is_none());
        assert!(format!("{:?}", app.toasts).contains("Sync cannot be aborted"));
    }

    #[test]
    fn ctrl_x_with_nothing_running_says_so() {
        let mut app = app();
        let now = Instant::now();
        let effects = app.update(
            press(
                KeyCode::Char('x'),
                KeyModifiers::CONTROL,
                KeyEventKind::Press,
            ),
            now,
        );
        assert!(effects.is_empty());
        assert!(app.overlay.is_none());
        assert!(format!("{:?}", app.toasts).contains("Nothing is running"));
    }

    #[test]
    fn abort_and_quit_stops_the_job_then_quits_when_it_stops() {
        let mut app = app();
        let now = Instant::now();
        app.enqueue(install_spec("bat"), now);
        app.enqueue(JobSpec::CleanCache, now);
        app.update(
            press(KeyCode::Char('q'), KeyModifiers::NONE, KeyEventKind::Press),
            now,
        );
        // Right twice moves to abort and quit, enter accepts it.
        let mut effects = Vec::new();
        for code in [KeyCode::Right, KeyCode::Right, KeyCode::Enter] {
            effects = app.update(press(code, KeyModifiers::NONE, KeyEventKind::Press), now);
        }
        assert!(matches!(effects.as_slice(), [Effect::AbortJob]));
        assert!(app.quit_when_idle);
        assert_eq!(app.queued_count(), 0);
        assert!(matches!(
            app.running_job().map(|job| &job.state),
            Some(JobState::Running {
                aborting: true,
                ..
            })
        ));
        app.update(finished(0, true, "Installed bat 1.0.0"), now);
        assert!(app.quit);
    }

    #[test]
    fn abort_and_quit_with_a_running_sync_waits_instead() {
        let mut app = app();
        let now = Instant::now();
        app.enqueue(JobSpec::Sync, now);
        app.update(
            press(KeyCode::Char('q'), KeyModifiers::NONE, KeyEventKind::Press),
            now,
        );
        let mut effects = Vec::new();
        for code in [KeyCode::Right, KeyCode::Right, KeyCode::Enter] {
            effects = app.update(press(code, KeyModifiers::NONE, KeyEventKind::Press), now);
        }
        assert!(effects.is_empty());
        assert!(app.quit_when_idle);
        assert!(matches!(
            app.running_job().map(|job| &job.state),
            Some(JobState::Running {
                aborting: false,
                ..
            })
        ));
        app.update(finished(0, true, "Repositories synced"), now);
        assert!(app.quit);
    }

    #[test]
    fn an_aborted_job_toasts_as_a_warning_without_attention() {
        let mut app = app();
        let now = Instant::now();
        app.enqueue(JobSpec::Sync, now);
        app.update(
            Msg::JobFinished {
                id: 0,
                outcome: JobOutcome {
                    ok: true,
                    aborted: true,
                    headline: "Aborted: 1 installed, 2 skipped".to_string(),
                    lines: Vec::new(),
                },
            },
            now,
        );
        let debug = format!("{:?}", app.toasts);
        assert!(debug.contains("Warning"), "toasts are {debug:?}");
        assert!(debug.contains("Aborted: 1 installed, 2 skipped"));
        assert!(!app.jobs.attention);
    }

    #[test]
    fn x_on_a_queued_job_removes_it_from_the_queue() {
        let mut app = app();
        let now = Instant::now();
        app.enqueue(JobSpec::Sync, now);
        app.enqueue(JobSpec::CleanCache, now);
        app.view = ViewId::Jobs;
        app.update(
            press(KeyCode::Char('x'), KeyModifiers::NONE, KeyEventKind::Press),
            now,
        );
        assert_eq!(app.jobs.list.len(), 1);
        assert_eq!(app.queued_count(), 0);
        assert!(
            format!("{:?}", app.toasts).contains("Removed Clean cache from the queue"),
            "toasts are {:?}",
            app.toasts
        );
    }

    #[test]
    fn x_on_a_finished_job_changes_nothing() {
        let mut app = app();
        let now = Instant::now();
        app.enqueue(JobSpec::Sync, now);
        app.update(finished(0, true, "Repositories synced"), now);
        app.view = ViewId::Jobs;
        let toasts_before = app.toasts.iter().count();
        app.update(
            press(KeyCode::Char('x'), KeyModifiers::NONE, KeyEventKind::Press),
            now,
        );
        assert_eq!(app.jobs.list.len(), 1);
        assert_eq!(app.toasts.iter().count(), toasts_before);
    }

    #[test]
    fn i_on_a_not_installed_row_resolves_its_exact_query() {
        let mut app = app();
        let now = Instant::now();
        app.update(
            catalog_loaded(vec![catalog_row("bat", "0.24.0", "bincache")]),
            now,
        );
        let effects = app.update(key('i'), now);
        assert_eq!(resolve_of(&effects), &["bat@0.24.0:bincache".to_string()]);
        assert!(app.pending.is_none());
    }

    #[test]
    fn i_on_an_installed_row_only_toasts() {
        let mut rows = vec![catalog_row("bat", "0.24.0", "bincache")];
        rows[0].installed = true;
        let mut app = app();
        let now = Instant::now();
        app.update(catalog_loaded(rows), now);
        let effects = app.update(key('i'), now);
        assert!(effects.is_empty());
        assert!(app.pending.is_none());
        assert!(
            format!("{:?}", app.toasts)
                .contains("bat is already installed. Use I and Force reinstall to reinstall."),
            "toasts are {:?}",
            app.toasts
        );
    }

    #[test]
    fn i_reports_installed_rows_and_resolves_the_rest() {
        let mut rows = vec![
            catalog_row("bat", "0.24.0", "bincache"),
            catalog_row("fd", "10.2.0", "bincache"),
            catalog_row("jq", "1.7.1", "bincache"),
        ];
        rows[0].installed = true;
        rows[1].installed = true;
        let mut app = app();
        let now = Instant::now();
        app.update(catalog_loaded(rows), now);
        for _ in 0..3 {
            app.update(key(' '), now);
        }
        let effects = app.update(key('i'), now);
        assert_eq!(resolve_of(&effects), &["jq@1.7.1:bincache".to_string()]);
        assert!(
            format!("{:?}", app.toasts).contains(
                "2 packages are already installed. Use I and Force reinstall to reinstall."
            ),
            "toasts are {:?}",
            app.toasts
        );
    }

    #[test]
    fn a_resolved_target_opens_a_confirm_and_y_installs() {
        let mut app = app();
        let now = Instant::now();
        app.update(
            catalog_loaded(vec![
                catalog_row("bat", "0.24.0", "bincache"),
                catalog_row("fd", "10.2.0", "pkgforge"),
            ]),
            now,
        );
        app.update(key(' '), now);
        let effects = app.update(key('i'), now);
        assert_eq!(resolve_of(&effects), &["bat@0.24.0:bincache".to_string()]);
        let effects = app.update(
            resolved(vec![Resolution::Ready(vec![target(package(
                "bat", "0.24.0", "bincache",
            ))])]),
            now,
        );
        assert!(effects.is_empty());
        assert!(app.pending.is_some());
        assert!(matches!(app.overlay, Some(Overlay::Confirm(_))));
        let open = screen(&mut app, 80, 24);
        assert!(open.contains("Install bat"), "screen is {open:?}");
        let effects = app.update(key('y'), now);
        let [
            Effect::StartJob {
                spec:
                    JobSpec::Install {
                        targets,
                        picked,
                        ..
                    },
                ..
            },
        ] = &effects[..]
        else {
            panic!("expected the install to start");
        };
        assert_eq!(targets.len(), 1);
        assert!(picked.is_empty());
        assert!(app.pending.is_none());
        assert!(app.overlay.is_none());
        assert!(app.browse.marks.is_empty());
    }

    #[test]
    fn an_ambiguous_query_opens_a_picker_then_the_confirm() {
        let mut app = app();
        let now = Instant::now();
        app.update(
            catalog_loaded(vec![catalog_row("ffmpeg", "7.1", "bincache")]),
            now,
        );
        app.update(key('i'), now);
        let mut old = package("ffmpeg", "7.0.2", "pkgcache");
        old.pkg_family = Some("jellyfin".to_string());
        app.update(
            resolved(vec![Resolution::Ambiguous {
                query: "ffmpeg".to_string(),
                candidates: vec![package("ffmpeg", "7.1", "bincache"), old],
            }]),
            now,
        );
        assert!(app.pending.is_some());
        assert!(matches!(app.overlay, Some(Overlay::Picker(_))));
        let picker = screen(&mut app, 80, 24);
        assert!(
            picker.contains("Choose a variant of \"ffmpeg\""),
            "screen is {picker:?}"
        );
        assert!(
            picker.contains("jellyfin/ffmpeg  7.0.2"),
            "screen is {picker:?}"
        );
        app.update(key('j'), now);
        let effects = app.update(code(KeyCode::Enter), now);
        assert!(effects.is_empty());
        assert!(matches!(app.overlay, Some(Overlay::Confirm(_))));
        let effects = app.update(key('y'), now);
        let [
            Effect::StartJob {
                spec: JobSpec::Install {
                    picked, ..
                },
                ..
            },
        ] = &effects[..]
        else {
            panic!("expected the install to start");
        };
        assert_eq!(picked.len(), 1);
        assert_eq!(picked[0].version, "7.0.2");
    }

    #[test]
    fn esc_on_the_variant_picker_drops_the_query_and_the_flow() {
        let mut app = app();
        let now = Instant::now();
        app.update(
            catalog_loaded(vec![catalog_row("ffmpeg", "7.1", "bincache")]),
            now,
        );
        app.update(key('i'), now);
        app.update(
            resolved(vec![Resolution::Ambiguous {
                query: "ffmpeg".to_string(),
                candidates: vec![package("ffmpeg", "7.1", "bincache")],
            }]),
            now,
        );
        assert!(matches!(app.overlay, Some(Overlay::Picker(_))));
        app.update(code(KeyCode::Esc), now);
        assert!(app.overlay.is_none());
        assert!(app.pending.is_none());
        assert!(app.jobs.list.is_empty());
    }

    #[test]
    fn esc_moves_through_the_queries_and_the_confirm_cancel_ends_the_flow() {
        let mut app = app();
        let now = Instant::now();
        app.update(
            catalog_loaded(vec![catalog_row("ffmpeg", "7.1", "bincache")]),
            now,
        );
        app.update(key('i'), now);
        app.update(
            resolved(vec![
                Resolution::Ambiguous {
                    query: "ffmpeg".to_string(),
                    candidates: vec![package("ffmpeg", "7.1", "bincache")],
                },
                Resolution::Ambiguous {
                    query: "python".to_string(),
                    candidates: vec![package("python", "3.12.0", "pkgcache")],
                },
            ]),
            now,
        );
        assert!(matches!(app.overlay, Some(Overlay::Picker(_))));
        app.update(code(KeyCode::Esc), now);
        let picker = screen(&mut app, 80, 24);
        assert!(
            picker.contains("Choose a variant of \"python\""),
            "screen is {picker:?}"
        );
        assert!(app.pending.is_some());
        app.update(code(KeyCode::Enter), now);
        assert!(matches!(app.overlay, Some(Overlay::Confirm(_))));
        app.update(code(KeyCode::Esc), now);
        assert!(app.overlay.is_none());
        assert!(app.pending.is_none());
        assert!(app.jobs.list.is_empty());
    }

    #[test]
    fn the_options_form_with_force_resolves_an_installed_row() {
        let mut rows = vec![catalog_row("bat", "0.24.0", "bincache")];
        rows[0].installed = true;
        let mut app = app();
        let now = Instant::now();
        app.update(catalog_loaded(rows), now);
        app.update(key('I'), now);
        assert!(matches!(app.overlay, Some(Overlay::Form(_))));
        let open = screen(&mut app, 80, 24);
        assert!(
            open.contains("Install bat with options"),
            "screen is {open:?}"
        );
        app.update(key(' '), now);
        for _ in 0..6 {
            app.update(code(KeyCode::Tab), now);
        }
        let effects = app.update(code(KeyCode::Enter), now);
        let [
            Effect::Resolve {
                choices, ..
            },
        ] = &effects[..]
        else {
            panic!("expected a resolve");
        };
        assert!(choices.force);
        assert_eq!(resolve_of(&effects), &["bat@0.24.0:bincache".to_string()]);
    }

    #[test]
    fn the_confirm_lists_sizes_and_the_options_summary() {
        let mut app = app();
        let now = Instant::now();
        app.update(
            catalog_loaded(vec![catalog_row("bat", "0.24.0", "bincache")]),
            now,
        );
        app.update(key('I'), now);
        app.update(key(' '), now);
        app.update(code(KeyCode::Tab), now);
        app.update(key(' '), now);
        for _ in 0..5 {
            app.update(code(KeyCode::Tab), now);
        }
        let effects = app.update(code(KeyCode::Enter), now);
        let [
            Effect::Resolve {
                choices, ..
            },
        ] = &effects[..]
        else {
            panic!("expected a resolve");
        };
        let mut bat = package("bat", "0.24.0", "bincache");
        bat.ghcr_size = Some(1_048_576);
        app.update(
            Msg::Loaded(Loaded::Resolved {
                choices: choices.clone(),
                result: Ok(vec![Resolution::Ready(vec![target(bat)])]),
            }),
            now,
        );
        assert!(matches!(app.overlay, Some(Overlay::Confirm(_))));
        let open = screen(&mut app, 80, 24);
        assert!(
            open.contains("bat  0.24.0  bincache  1.0 MiB"),
            "screen is {open:?}"
        );
        assert!(
            open.contains("Options: force, binary only"),
            "screen is {open:?}"
        );
        assert!(
            open.contains("Download size: 1.0 MiB"),
            "screen is {open:?}"
        );
    }

    #[test]
    fn picking_the_second_version_targets_that_version() {
        let mut row = catalog_row("bat", "0.24.0", "bincache");
        row.other_versions = vec!["0.23.0".to_string()];
        let mut app = app();
        let now = Instant::now();
        app.update(catalog_loaded(vec![row]), now);
        app.update(key('v'), now);
        assert!(matches!(app.overlay, Some(Overlay::Picker(_))));
        let picker = screen(&mut app, 80, 24);
        assert!(picker.contains("Versions of bat"), "screen is {picker:?}");
        assert!(picker.contains("latest"), "screen is {picker:?}");
        app.update(key('j'), now);
        let effects = app.update(code(KeyCode::Enter), now);
        assert_eq!(resolve_of(&effects), &["bat@0.23.0:bincache".to_string()]);
    }

    #[test]
    fn a_version_pick_survives_a_catalog_reload_under_the_picker() {
        let mut bat = catalog_row("bat", "0.24.0", "bincache");
        bat.other_versions = vec!["0.23.0".to_string()];
        let fd = catalog_row("fd", "10.2.0", "pkgforge");
        let mut app = app();
        let now = Instant::now();
        app.update(catalog_loaded(vec![fd, bat]), now);
        app.update(key('j'), now);
        app.update(key('v'), now);
        assert!(matches!(app.overlay, Some(Overlay::Picker(_))));
        // The catalog swaps while the picker is open, moving the cursor.
        app.update(
            catalog_loaded(vec![catalog_row("jq", "1.7.1", "bincache")]),
            now,
        );
        assert_eq!(app.browse.cursor.selected, 0);
        app.update(key('j'), now);
        let effects = app.update(code(KeyCode::Enter), now);
        assert_eq!(
            resolve_of(&effects),
            &["bat@0.23.0:bincache".to_string()],
            "the pick must target the package the picker named"
        );
    }

    #[test]
    fn the_run_prompt_runs_the_package_it_was_opened_for() {
        let mut app = app();
        let now = Instant::now();
        app.update(
            catalog_loaded(vec![
                catalog_row("jq", "1.7.1", "bincache"),
                catalog_row("fd", "10.2.0", "pkgforge"),
            ]),
            now,
        );
        app.update(key('j'), now);
        app.update(key('x'), now);
        let prompt = screen(&mut app, 80, 24);
        assert!(
            prompt.contains("Run fd with arguments"),
            "screen is {prompt:?}"
        );
        // The catalog swaps while the prompt is open, moving the cursor.
        app.update(
            catalog_loaded(vec![catalog_row("jq", "1.7.1", "bincache")]),
            now,
        );
        for c in ['-', 'v', 'e'] {
            app.update(key(c), now);
        }
        let effects = app.update(code(KeyCode::Enter), now);
        let [
            Effect::PrepareRun {
                query,
                args,
                ..
            },
        ] = &effects[..]
        else {
            panic!("expected a run");
        };
        assert_eq!(query, "fd:pkgforge", "the prompt's package runs");
        assert_eq!(args, &["-ve".to_string()]);
    }

    #[test]
    fn v_without_other_versions_says_so() {
        let mut app = app();
        let now = Instant::now();
        app.update(
            catalog_loaded(vec![catalog_row("bat", "0.24.0", "bincache")]),
            now,
        );
        let effects = app.update(key('v'), now);
        assert!(effects.is_empty());
        assert!(app.overlay.is_none());
        assert!(
            format!("{:?}", app.toasts).contains("bat has only one version"),
            "toasts are {:?}",
            app.toasts
        );
    }

    #[test]
    fn the_prompt_splits_its_text_into_queries() {
        let mut app = app();
        let now = Instant::now();
        app.update(
            catalog_loaded(vec![catalog_row("bat", "0.24.0", "bincache")]),
            now,
        );
        app.update(key('+'), now);
        assert!(matches!(app.overlay, Some(Overlay::Prompt(_))));
        for c in "bat ".chars() {
            app.update(key(c), now);
        }
        for c in "https://example.com/x.AppImage".chars() {
            app.update(key(c), now);
        }
        let effects = app.update(code(KeyCode::Enter), now);
        let queries = resolve_of(&effects);
        assert_eq!(queries.len(), 2);
        assert_eq!(queries[0], "bat");
        assert_eq!(queries[1], "https://example.com/x.AppImage");
    }

    /// The one prepare-run the effects hold, failing the test otherwise.
    fn prepare_run_of(
        effects: &[Effect],
    ) -> (&String, &Option<String>, &Option<String>, &Vec<String>) {
        let [
            Effect::PrepareRun {
                query,
                repo,
                pkg_id,
                args,
            },
        ] = effects
        else {
            panic!("expected one prepare run");
        };
        (query, repo, pkg_id, args)
    }

    /// A run-ready result with the given arguments.
    fn run_ready(result: Result<PrepareRunResult, String>, args: Vec<String>) -> Msg {
        Msg::Loaded(Loaded::RunReady {
            args,
            result: Box::new(result),
        })
    }

    #[test]
    fn x_on_a_row_opens_the_run_prompt_asking_for_arguments() {
        let mut app = app();
        let now = Instant::now();
        app.update(
            catalog_loaded(vec![catalog_row("jq", "1.7.1", "bincache")]),
            now,
        );
        let effects = app.update(key('x'), now);
        assert!(effects.is_empty());
        assert!(matches!(app.overlay, Some(Overlay::Prompt(_))));
        let open = screen(&mut app, 80, 24);
        assert!(open.contains("Run jq with arguments"), "screen is {open:?}");
        assert!(
            open.contains("args are split on spaces"),
            "screen is {open:?}"
        );
    }

    #[test]
    fn x_without_a_row_opens_nothing() {
        let mut app = app();
        let now = Instant::now();
        let effects = app.update(key('x'), now);
        assert!(effects.is_empty());
        assert!(app.overlay.is_none());
    }

    #[test]
    fn accepting_the_run_prompt_prepares_the_rows_exact_query_with_the_arguments() {
        let mut row = catalog_row("ffmpeg", "7.1", "bincache");
        row.key.pkg_family = Some("jellyfin".to_string());
        row.key.pkg_id = Some("ffmpeg-1".to_string());
        let mut app = app();
        let now = Instant::now();
        app.update(catalog_loaded(vec![row]), now);
        app.update(key('x'), now);
        for c in "--version".chars() {
            app.update(key(c), now);
        }
        let effects = app.update(code(KeyCode::Enter), now);
        let (query, repo, pkg_id, args) = prepare_run_of(&effects);
        assert_eq!(query, "jellyfin/ffmpeg:bincache");
        assert_eq!(repo.as_deref(), Some("bincache"));
        assert_eq!(pkg_id.as_deref(), Some("ffmpeg-1"));
        assert_eq!(args, &["--version".to_string()]);
        assert!(app.overlay.is_none());
    }

    #[test]
    fn accepting_the_run_prompt_without_arguments_prepares_an_empty_run() {
        let mut app = app();
        let now = Instant::now();
        app.update(
            catalog_loaded(vec![catalog_row("jq", "1.7.1", "bincache")]),
            now,
        );
        app.update(key('x'), now);
        let effects = app.update(code(KeyCode::Enter), now);
        let (query, repo, pkg_id, args) = prepare_run_of(&effects);
        assert_eq!(query, "jq:bincache");
        assert_eq!(repo.as_deref(), Some("bincache"));
        assert!(pkg_id.is_none());
        assert!(args.is_empty());
    }

    #[test]
    fn a_ready_run_suspends_to_execute_the_binary_with_the_arguments() {
        let mut app = app();
        let now = Instant::now();
        let effects = app.update(
            run_ready(
                Ok(PrepareRunResult::Ready {
                    path: PathBuf::from("/cache/bin/jq"),
                    downloaded: false,
                }),
                vec!["--version".to_string()],
            ),
            now,
        );
        let [
            Effect::Suspend(Suspended::Exec {
                path,
                args,
            }),
        ] = &effects[..]
        else {
            panic!("expected a suspend");
        };
        assert_eq!(path, &PathBuf::from("/cache/bin/jq"));
        assert_eq!(args, &["--version".to_string()]);
        assert!(app.dirty);
    }

    #[test]
    fn an_ambiguous_run_opens_a_picker_and_reruns_the_chosen_variant() {
        let mut chosen = package("ffmpeg", "7.1", "bincache");
        chosen.pkg_id = Some("ffmpeg-1".to_string());
        let mut other = package("ffmpeg", "7.0.2", "pkgcache");
        other.pkg_family = Some("jellyfin".to_string());
        let mut app = app();
        let now = Instant::now();
        app.update(
            run_ready(
                Ok(PrepareRunResult::Ambiguous(AmbiguousPackage {
                    query: "ffmpeg".to_string(),
                    candidates: vec![chosen, other],
                })),
                vec!["-i".to_string()],
            ),
            now,
        );
        assert!(matches!(app.overlay, Some(Overlay::Picker(_))));
        assert!(app.run_pick.is_some());
        let picker = screen(&mut app, 80, 24);
        assert!(
            picker.contains("Choose a variant of \"ffmpeg\""),
            "screen is {picker:?}"
        );
        assert!(
            picker.contains("jellyfin/ffmpeg  7.0.2"),
            "screen is {picker:?}"
        );
        let effects = app.update(code(KeyCode::Enter), now);
        let (query, repo, pkg_id, args) = prepare_run_of(&effects);
        assert_eq!(query, "ffmpeg@7.1:bincache");
        assert_eq!(repo.as_deref(), Some("bincache"));
        assert_eq!(pkg_id.as_deref(), Some("ffmpeg-1"));
        assert_eq!(args, &["-i".to_string()]);
        assert!(app.run_pick.is_none());
        assert!(app.overlay.is_none());
    }

    #[test]
    fn esc_on_the_run_picker_drops_the_arguments_and_the_candidates() {
        let mut app = app();
        let now = Instant::now();
        app.update(
            run_ready(
                Ok(PrepareRunResult::Ambiguous(AmbiguousPackage {
                    query: "ffmpeg".to_string(),
                    candidates: vec![package("ffmpeg", "7.1", "bincache")],
                })),
                Vec::new(),
            ),
            now,
        );
        assert!(matches!(app.overlay, Some(Overlay::Picker(_))));
        app.update(code(KeyCode::Esc), now);
        assert!(app.overlay.is_none());
        assert!(app.run_pick.is_none());
        assert!(app.pending.is_none());
    }

    #[test]
    fn an_ambiguous_run_cannot_displace_a_password_dialog() {
        let (mut app, now) = authorizing_app();
        app.update(
            Msg::Loaded(Loaded::Escalation(EscalationProbe::Password)),
            now,
        );
        let mut chosen = package("ffmpeg", "7.1", "bincache");
        chosen.pkg_id = Some("ffmpeg-1".to_string());
        let mut other = package("ffmpeg", "7.0.2", "pkgcache");
        other.pkg_family = Some("jellyfin".to_string());
        app.update(
            run_ready(
                Ok(PrepareRunResult::Ambiguous(AmbiguousPackage {
                    query: "ffmpeg".to_string(),
                    candidates: vec![chosen, other],
                })),
                vec!["-i".to_string()],
            ),
            now,
        );
        assert!(
            matches!(app.overlay, Some(Overlay::Authorize(_))),
            "the pass dialog keeps its slot"
        );
        assert!(app.run_pick.is_none(), "the run waits for a retry");
    }

    #[test]
    fn a_failed_run_toasts_the_error_without_an_effect() {
        let mut app = app();
        let now = Instant::now();
        let effects = app.update(
            run_ready(Err("no candidate found".to_string()), Vec::new()),
            now,
        );
        assert!(effects.is_empty());
        assert!(app.overlay.is_none());
        let toasts = format!("{:?}", app.toasts);
        assert!(
            toasts.contains("no candidate found"),
            "toasts are {toasts:?}"
        );
    }

    #[test]
    fn a_redraw_message_marks_the_screen_dirty() {
        let mut app = app();
        app.dirty = false;
        app.update(Msg::Redraw, Instant::now());
        assert!(app.dirty);
    }

    #[test]
    fn a_dead_input_thread_sets_the_fatal_error_and_quits() {
        let mut app = app();
        app.dirty = false;
        let effects = app.update(
            Msg::InputFailed("got an error from the terminal".to_string()),
            Instant::now(),
        );
        assert!(effects.is_empty());
        assert!(app.quit, "a deaf interface has no way to stay open");
        assert!(app.dirty);
        assert_eq!(app.fatal.as_deref(), Some("got an error from the terminal"));
    }

    #[test]
    fn a_not_found_resolution_toasts_with_suggestions() {
        let mut app = app();
        let now = Instant::now();
        app.update(
            catalog_loaded(vec![catalog_row("bat", "0.24.0", "bincache")]),
            now,
        );
        app.update(key('i'), now);
        app.update(
            resolved(vec![
                Resolution::NotFound {
                    query: "batt".to_string(),
                    suggestions: vec!["bat".to_string(), "bass".to_string()],
                },
                Resolution::AlreadyInstalled {
                    name: "fd".to_string(),
                    repo: "pkgforge".to_string(),
                    version: "10.2.0".to_string(),
                },
            ]),
            now,
        );
        assert!(app.pending.is_none());
        assert!(app.overlay.is_none());
        let toasts = format!("{:?}", app.toasts);
        assert!(
            toasts.contains("Not found: batt. Did you mean bat, bass?"),
            "toasts are {toasts:?}"
        );
        assert!(
            toasts.contains("fd is already installed"),
            "toasts are {toasts:?}"
        );
    }

    #[test]
    fn a_failed_resolve_toasts_the_error_without_a_flow() {
        let mut app = app();
        let now = Instant::now();
        app.update(
            catalog_loaded(vec![catalog_row("bat", "0.24.0", "bincache")]),
            now,
        );
        app.update(key('i'), now);
        app.update(
            Msg::Loaded(Loaded::Resolved {
                choices: InstallChoices::default(),
                result: Err("metadata locked".to_string()),
            }),
            now,
        );
        assert!(app.pending.is_none());
        assert!(app.overlay.is_none());
        assert!(
            format!("{:?}", app.toasts).contains("metadata locked"),
            "toasts are {:?}",
            app.toasts
        );
    }

    #[test]
    fn a_finished_install_reloads_the_catalog_and_the_installed_list() {
        let mut app = app();
        let now = Instant::now();
        app.enqueue(install_spec("bat"), now);
        let effects = app.update(finished(0, true, "Installed bat 1.0.0"), now);
        assert!(matches!(
            effects.as_slice(),
            [Effect::LoadCatalog { .. }, Effect::LoadInstalled { .. }]
        ));
    }

    #[test]
    fn a_finished_remove_reloads_the_installed_list_alone() {
        let mut app = app();
        let now = Instant::now();
        app.enqueue(
            JobSpec::Remove {
                packages: vec![crate::test_support::installed_package(
                    "bat", "1.0.0", "bincache",
                )],
            },
            now,
        );
        let effects = app.update(finished(0, true, "Removed bat 1.0.0"), now);
        assert!(matches!(effects.as_slice(), [Effect::LoadInstalled { .. }]));
    }

    #[test]
    fn a_finished_cache_clean_reloads_health() {
        let mut app = app();
        let now = Instant::now();
        app.enqueue(JobSpec::CleanCache, now);
        let effects = app.update(finished(0, true, "Cache cleaned"), now);
        assert!(matches!(effects.as_slice(), [Effect::LoadHealth]));
        assert!(app.health.report.is_loading());
    }

    #[test]
    fn a_finished_remove_reloads_health_once_it_loaded() {
        let mut app = app();
        let now = Instant::now();
        app.view = ViewId::Health;
        app.update(
            Msg::Loaded(Loaded::Health(Ok((
                crate::test_support::health_report(),
                crate::test_support::health_paths(),
            )))),
            now,
        );
        app.view = ViewId::Installed;
        app.enqueue(
            JobSpec::Remove {
                packages: vec![installed_package("bat", "1.0.0", "bincache")],
            },
            now,
        );
        let effects = app.update(finished(0, true, "Removed bat 1.0.0"), now);
        assert!(matches!(
            effects.as_slice(),
            [Effect::LoadInstalled { .. }, Effect::LoadHealth]
        ));
        assert!(app.health.report.is_loading());
    }

    #[test]
    fn a_loaded_installed_list_replaces_the_view_state() {
        let mut app = app();
        let entries = vec![crate::test_support::installed_entry(
            "bat", "0.24.0", "bincache",
        )];
        let total_size = entries.iter().map(|entry| entry.disk_size).sum();
        app.update(
            Msg::Loaded(Loaded::Installed {
                check: 0,
                result: Ok(soar_operations::InstalledListResult {
                    packages: entries,
                    total_count: 1,
                    total_size,
                }),
            }),
            Instant::now(),
        );
        assert!(app.installed.list.ready().is_some());
        assert_eq!(app.installed.visible, vec![0]);
        assert!(app.dirty);
        app.dirty = false;
        app.update(
            Msg::Loaded(Loaded::Installed {
                check: 0,
                result: Err("offline".to_string()),
            }),
            Instant::now(),
        );
        assert!(app.installed.list.ready().is_none());
        assert!(app.dirty);
    }

    #[test]
    fn ctrl_r_on_installed_reloads_the_installed_list() {
        let mut app = app();
        app.view = ViewId::Installed;
        let effects = app.update(
            press(
                KeyCode::Char('r'),
                KeyModifiers::CONTROL,
                KeyEventKind::Press,
            ),
            Instant::now(),
        );
        assert!(matches!(effects.as_slice(), [Effect::LoadInstalled { .. }]));
        assert!(app.installed.list.is_loading());
    }

    #[test]
    fn s_on_the_installed_list_loads_the_rows_variants() {
        let mut app = app();
        let now = Instant::now();
        app.view = ViewId::Installed;
        app.update(
            installed_loaded(vec![installed_entry("bat", "0.24.0", "bincache")]),
            now,
        );
        let effects = app.update(key('s'), now);
        let [
            Effect::LoadVariants {
                name,
            },
        ] = &effects[..]
        else {
            panic!("expected a variant load");
        };
        assert_eq!(name, "bat");
    }

    #[test]
    fn u_on_the_installed_list_scopes_the_updates_check() {
        let mut app = app();
        let now = Instant::now();
        app.view = ViewId::Installed;
        app.update(
            installed_loaded(vec![installed_entry("bat", "0.24.0", "bincache")]),
            now,
        );
        let effects = app.update(key('u'), now);
        assert_eq!(app.view, ViewId::Updates);
        assert_eq!(
            app.updates.scope.as_deref(),
            Some(["bat".to_string()].as_slice())
        );
        assert!(matches!(
            effects.as_slice(),
            [Effect::CheckUpdates { names: Some(names), .. }] if names == &["bat".to_string()]
        ));
    }

    #[test]
    fn esc_in_a_scoped_updates_list_checks_everything() {
        let mut app = app();
        let now = Instant::now();
        app.view = ViewId::Installed;
        app.update(
            installed_loaded(vec![installed_entry("bat", "0.24.0", "bincache")]),
            now,
        );
        app.update(key('u'), now);
        let effects = app.update(code(KeyCode::Esc), now);
        assert!(app.updates.scope.is_none());
        assert!(matches!(
            effects.as_slice(),
            [Effect::CheckUpdates {
                names: None,
                ..
            }]
        ));
    }

    #[test]
    fn esc_without_a_scope_checks_nothing() {
        let mut app = app();
        let now = Instant::now();
        app.view = ViewId::Updates;
        let effects = app.update(code(KeyCode::Esc), now);
        assert!(effects.is_empty());
    }

    #[test]
    fn an_empty_variant_list_toasts_that_the_package_is_not_installed() {
        let mut app = app();
        let now = Instant::now();
        let effects = app.update(
            Msg::Loaded(Loaded::Variants {
                name: "bat".to_string(),
                result: Box::new(Ok(Vec::new())),
            }),
            now,
        );
        assert!(effects.is_empty());
        assert!(app.overlay.is_none());
        assert!(app.variant_pick.is_none());
        let toasts = format!("{:?}", app.toasts);
        assert!(
            toasts.contains("bat is not installed"),
            "toasts are {toasts:?}"
        );
        assert!(toasts.contains("Info"), "toasts are {toasts:?}");
    }

    #[test]
    fn a_sole_active_variant_toasts_that_no_other_exists() {
        let mut app = app();
        let now = Instant::now();
        let effects = app.update(
            variants_loaded("bat", vec![variant(7, "bat", "0.24.0", "bincache", true)]),
            now,
        );
        assert!(effects.is_empty());
        assert!(app.overlay.is_none());
        assert!(app.variant_pick.is_none());
        assert!(format!("{:?}", app.toasts).contains("bat has no other variants"));
    }

    #[test]
    fn a_sole_inactive_variant_confirms_then_switches_to_it() {
        let mut app = app();
        let now = Instant::now();
        let effects = app.update(
            variants_loaded("bat", vec![variant(7, "bat", "0.24.0", "bincache", false)]),
            now,
        );
        assert!(effects.is_empty());
        assert!(app.variant_pick.is_none());
        assert!(matches!(app.overlay, Some(Overlay::Confirm(_))));
        let open = screen(&mut app, 80, 24);
        assert!(open.contains("Switch bat to 0.24.0"), "screen is {open:?}");
        assert!(open.contains("bat  0.24.0  bincache"), "screen is {open:?}");
        let effects = app.update(key('y'), now);
        let [
            Effect::StartJob {
                spec:
                    JobSpec::Switch {
                        id,
                        name,
                    },
                ..
            },
        ] = &effects[..]
        else {
            panic!("expected the switch to start");
        };
        assert_eq!(*id, 7);
        assert_eq!(name, "bat");
        assert!(app.overlay.is_none());
    }

    #[test]
    fn several_variants_open_a_picker_and_the_inactive_one_switches() {
        let mut app = app();
        let now = Instant::now();
        let mut active = variant(7, "bat", "0.24.0", "bincache", true);
        active.package.pkg_family = Some("jellyfin".to_string());
        let other = variant(9, "bat", "0.23.0", "pkgcache", false);
        let effects = app.update(variants_loaded("bat", vec![active, other]), now);
        assert!(effects.is_empty());
        assert!(app.variant_pick.is_some());
        assert!(matches!(app.overlay, Some(Overlay::Picker(_))));
        let open = screen(&mut app, 80, 24);
        assert!(open.contains("Variants of bat"), "screen is {open:?}");
        assert!(open.contains("jellyfin/bat  0.24.0"), "screen is {open:?}");
        assert!(open.contains("● active"), "screen is {open:?}");
        app.update(key('j'), now);
        let effects = app.update(code(KeyCode::Enter), now);
        let [
            Effect::StartJob {
                spec:
                    JobSpec::Switch {
                        id,
                        name,
                    },
                ..
            },
        ] = &effects[..]
        else {
            panic!("expected the switch to start");
        };
        assert_eq!(*id, 9);
        assert_eq!(name, "bat");
        assert!(app.variant_pick.is_none());
        assert!(app.overlay.is_none());
    }

    #[test]
    fn picking_the_active_variant_closes_without_a_job() {
        let mut app = app();
        let now = Instant::now();
        let active = variant(7, "bat", "0.24.0", "bincache", true);
        let other = variant(9, "bat", "0.23.0", "pkgcache", false);
        app.update(variants_loaded("bat", vec![active, other]), now);
        assert!(matches!(app.overlay, Some(Overlay::Picker(_))));
        let effects = app.update(code(KeyCode::Enter), now);
        assert!(effects.is_empty());
        assert!(app.jobs.list.is_empty());
        assert!(app.variant_pick.is_none());
        assert!(app.overlay.is_none());
    }

    #[test]
    fn a_variants_picker_cannot_displace_a_password_dialog() {
        let (mut app, now) = authorizing_app();
        app.update(
            Msg::Loaded(Loaded::Escalation(EscalationProbe::Password)),
            now,
        );
        let active = variant(7, "bat", "0.24.0", "bincache", true);
        let other = variant(9, "bat", "0.23.0", "pkgcache", false);
        app.update(variants_loaded("bat", vec![active, other]), now);
        assert!(
            matches!(app.overlay, Some(Overlay::Authorize(_))),
            "the pass dialog keeps its slot"
        );
        assert!(app.variant_pick.is_none(), "the switch waits for a retry");
    }

    #[test]
    fn a_sole_variant_switch_cannot_displace_a_password_dialog() {
        let (mut app, now) = authorizing_app();
        app.update(
            Msg::Loaded(Loaded::Escalation(EscalationProbe::Password)),
            now,
        );
        let effects = app.update(
            variants_loaded("bat", vec![variant(7, "bat", "0.24.0", "bincache", false)]),
            now,
        );
        assert!(effects.is_empty());
        assert!(
            matches!(app.overlay, Some(Overlay::Authorize(_))),
            "the pass dialog keeps its slot"
        );
        assert!(app.variant_pick.is_none(), "the switch waits for a retry");
        let toasts = format!("{:?}", app.toasts);
        assert!(
            toasts.contains("Try again once it finishes"),
            "toasts are {toasts:?}"
        );
    }

    #[test]
    fn esc_on_the_variants_picker_drops_the_flow() {
        let mut app = app();
        let now = Instant::now();
        let active = variant(7, "bat", "0.24.0", "bincache", true);
        let other = variant(9, "bat", "0.23.0", "pkgcache", false);
        app.update(variants_loaded("bat", vec![active, other]), now);
        app.update(code(KeyCode::Esc), now);
        assert!(app.overlay.is_none());
        assert!(app.variant_pick.is_none());
        assert!(app.jobs.list.is_empty());
    }

    #[test]
    fn a_failed_variant_load_toasts_the_error() {
        let mut app = app();
        let now = Instant::now();
        let effects = app.update(
            Msg::Loaded(Loaded::Variants {
                name: "bat".to_string(),
                result: Box::new(Err("metadata locked".to_string())),
            }),
            now,
        );
        assert!(effects.is_empty());
        assert!(app.overlay.is_none());
        let toasts = format!("{:?}", app.toasts);
        assert!(toasts.contains("metadata locked"), "toasts are {toasts:?}");
        assert!(toasts.contains("Error"), "toasts are {toasts:?}");
    }

    /// A ready updates list holding the given entries, answering the
    /// current check.
    fn updates_loaded(app: &App, list: Vec<UpdateInfo>) -> Msg {
        Msg::Loaded(Loaded::Updates {
            check: app.updates.check,
            result: Ok(list),
        })
    }

    /// Three checked updates, the usual fixtures.
    fn three_updates() -> Vec<UpdateInfo> {
        vec![
            update_info("bat", "0.23.0", "0.24.0", "bincache"),
            update_info("fd", "10.1.0", "10.2.0", "bincache"),
            update_info("yq", "4.44.2", "4.44.3", "bincache"),
        ]
    }

    #[test]
    fn u_with_two_of_three_marked_confirms_then_updates_those_two() {
        let mut app = app();
        let now = Instant::now();
        app.view = ViewId::Updates;
        app.update(updates_loaded(&app, three_updates()), now);
        app.update(key(' '), now);
        app.update(key(' '), now);
        assert_eq!(app.updates.marks, [0, 1].into_iter().collect());
        let effects = app.update(key('u'), now);
        assert!(effects.is_empty());
        assert!(matches!(app.overlay, Some(Overlay::Confirm(_))));
        assert_eq!(app.update_pick.as_deref(), Some(&[0, 1][..]));
        let open = screen(&mut app, 80, 24);
        assert!(open.contains("Update 2 packages"), "screen is {open:?}");
        assert!(open.contains("bat  0.23.0"), "screen is {open:?}");
        assert!(open.contains(app.theme.sym().arrow), "screen is {open:?}");
        assert!(
            !open.contains("Keep old versions: yes"),
            "screen is {open:?}"
        );
        let effects = app.update(key('y'), now);
        let [
            Effect::StartJob {
                spec:
                    JobSpec::Update {
                        updates,
                        keep_old,
                    },
                ..
            },
        ] = &effects[..]
        else {
            panic!("expected the update to start");
        };
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].pkg_name, "bat");
        assert_eq!(updates[1].pkg_name, "fd");
        assert!(!*keep_old);
        assert!(app.update_pick.is_none());
        assert!(app.overlay.is_none());
        assert!(app.updates.marks.is_empty());
        assert!(app.updates.list.is_loading());
        assert_eq!(app.jobs.list[0].kind, JobKind::Update);
    }

    #[test]
    fn u_with_one_marked_update_confirms_under_the_package_name() {
        let mut app = app();
        let now = Instant::now();
        app.view = ViewId::Updates;
        app.update(updates_loaded(&app, three_updates()), now);
        app.update(key('j'), now);
        app.update(key(' '), now);
        app.update(key('u'), now);
        let open = screen(&mut app, 80, 24);
        assert!(open.contains("Update fd"), "screen is {open:?}");
        assert!(open.contains("fd  10.1.0"), "screen is {open:?}");
        let effects = app.update(key('y'), now);
        let [
            Effect::StartJob {
                spec: JobSpec::Update {
                    updates, ..
                },
                ..
            },
        ] = &effects[..]
        else {
            panic!("expected the update to start");
        };
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].pkg_name, "fd");
    }

    #[test]
    fn u_without_marks_confirms_every_row() {
        let mut app = app();
        let now = Instant::now();
        app.view = ViewId::Updates;
        app.update(updates_loaded(&app, three_updates()), now);
        let effects = app.update(key('u'), now);
        assert!(effects.is_empty());
        assert_eq!(app.update_pick.as_deref(), Some(&[0, 1, 2][..]));
        let open = screen(&mut app, 80, 24);
        assert!(open.contains("Update 3 packages"), "screen is {open:?}");
        let effects = app.update(key('y'), now);
        let [
            Effect::StartJob {
                spec: JobSpec::Update {
                    updates, ..
                },
                ..
            },
        ] = &effects[..]
        else {
            panic!("expected the update to start");
        };
        assert_eq!(updates.len(), 3);
        assert_eq!(updates[2].pkg_name, "yq");
    }

    #[test]
    fn u_with_an_empty_list_only_toasts() {
        let mut app = app();
        let now = Instant::now();
        app.view = ViewId::Updates;
        app.update(updates_loaded(&app, Vec::new()), now);
        let effects = app.update(key('u'), now);
        assert!(effects.is_empty());
        assert!(app.overlay.is_none());
        assert!(app.update_pick.is_none());
        assert!(app.jobs.list.is_empty());
        let toasts = format!("{:?}", app.toasts);
        assert!(
            toasts.contains("Nothing to update"),
            "toasts are {toasts:?}"
        );
    }

    #[test]
    fn keep_old_shows_in_the_confirm_and_reaches_the_job() {
        let mut app = app();
        let now = Instant::now();
        app.view = ViewId::Updates;
        app.update(updates_loaded(&app, three_updates()), now);
        app.update(key('o'), now);
        assert!(app.updates.keep_old);
        app.update(key('u'), now);
        let open = screen(&mut app, 80, 24);
        assert!(
            open.contains("Keep old versions: yes"),
            "screen is {open:?}"
        );
        let effects = app.update(key('y'), now);
        let [
            Effect::StartJob {
                spec: JobSpec::Update {
                    keep_old, ..
                },
                ..
            },
        ] = &effects[..]
        else {
            panic!("expected the update to start");
        };
        assert!(*keep_old);
    }

    #[test]
    fn esc_on_the_update_confirm_leaves_the_list_unchanged() {
        let mut app = app();
        let now = Instant::now();
        app.view = ViewId::Updates;
        app.update(updates_loaded(&app, three_updates()), now);
        app.update(key('u'), now);
        app.update(code(KeyCode::Esc), now);
        assert!(app.overlay.is_none());
        assert!(app.update_pick.is_none());
        assert!(app.jobs.list.is_empty());
        let Loadable::Ready(list) = &app.updates.list else {
            panic!("expected the list to stay ready");
        };
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].pkg_name, "bat");
    }

    #[test]
    fn a_finished_update_rechecks_with_the_scope_and_reloads_everything() {
        let mut app = app();
        let now = Instant::now();
        app.updates.scope = Some(vec!["bat".to_string()]);
        app.enqueue(
            JobSpec::Update {
                updates: vec![update_info("bat", "0.23.0", "0.24.0", "bincache")],
                keep_old: false,
            },
            now,
        );
        let effects = app.update(finished(0, true, "Updated bat 0.24.0"), now);
        let [
            Effect::CheckUpdates {
                names, ..
            },
            Effect::LoadInstalled {
                ..
            },
            Effect::LoadCatalog {
                ..
            },
        ] = &effects[..]
        else {
            panic!("expected a recheck with the reloads");
        };
        assert_eq!(names, &Some(vec!["bat".to_string()]));
        assert!(app.updates.list.is_loading());
        assert_eq!(app.updates.checked, 0);
        assert!(app.updates.skipped.is_empty());
    }

    #[test]
    fn a_finished_install_rechecks_when_updates_was_visited() {
        let mut app = app();
        let now = Instant::now();
        app.view = ViewId::Updates;
        app.update(updates_loaded(&app, three_updates()), now);
        app.view = ViewId::Browse;
        app.enqueue(install_spec("bat"), now);
        let effects = app.update(finished(0, true, "Installed bat 1.0.0"), now);
        assert!(matches!(
            effects.as_slice(),
            [
                Effect::LoadCatalog { .. },
                Effect::LoadInstalled { .. },
                Effect::CheckUpdates { .. }
            ]
        ));
    }

    #[test]
    fn a_finished_remove_rechecks_when_updates_was_visited() {
        let mut app = app();
        let now = Instant::now();
        app.view = ViewId::Updates;
        app.update(updates_loaded(&app, three_updates()), now);
        app.view = ViewId::Installed;
        app.enqueue(
            JobSpec::Remove {
                packages: vec![installed_package("bat", "1.0.0", "bincache")],
            },
            now,
        );
        let effects = app.update(finished(0, true, "Removed bat 1.0.0"), now);
        assert!(matches!(
            effects.as_slice(),
            [Effect::LoadInstalled { .. }, Effect::CheckUpdates { .. }]
        ));
    }

    #[test]
    fn a_finished_switch_rechecks_when_updates_was_visited() {
        let mut app = app();
        let now = Instant::now();
        app.view = ViewId::Updates;
        app.update(updates_loaded(&app, three_updates()), now);
        app.view = ViewId::Installed;
        app.enqueue(
            JobSpec::Switch {
                id: 7,
                name: "bat".to_string(),
            },
            now,
        );
        let effects = app.update(finished(0, true, "Switched bat"), now);
        assert!(matches!(
            effects.as_slice(),
            [Effect::LoadInstalled { .. }, Effect::CheckUpdates { .. }]
        ));
    }

    /// A ready health load carrying the given report and the usual paths.
    fn health_loaded(report: HealthReport) -> Msg {
        health_loaded_with_paths(report, health_paths())
    }

    /// A session on the health view with the given report loaded.
    fn health_app(report: HealthReport) -> (App, Instant) {
        let mut app = app();
        app.view = ViewId::Health;
        app.update(health_loaded(report), Instant::now());
        (app, Instant::now())
    }

    /// One broken package with the given name and install path.
    fn broken_package(name: &str, path: &str) -> BrokenPackage {
        BrokenPackage {
            pkg_name: name.to_string(),
            installed_path: path.to_string(),
        }
    }

    #[test]
    fn b_without_broken_symlinks_only_leaves_a_note() {
        let (mut app, now) = health_app(health_report());
        let effects = app.update(key('b'), now);
        assert!(effects.is_empty());
        assert!(app.overlay.is_none());
        let toasts = format!("{:?}", app.toasts);
        assert!(
            toasts.contains("No broken symlinks"),
            "toasts are {toasts:?}"
        );
    }

    #[test]
    fn b_with_three_broken_symlinks_confirms_then_removes_them() {
        let mut report = health_report();
        report.broken_symlinks = (1..=3)
            .map(|n| Path::new("/home/user/.local/bin").join(format!("tool{n}")))
            .collect();
        let (mut app, now) = health_app(report);
        let effects = app.update(key('b'), now);
        assert!(effects.is_empty());
        assert!(matches!(app.overlay, Some(Overlay::Confirm(_))));
        let open = screen(&mut app, 80, 24);
        assert!(
            open.contains("Remove 3 broken symlinks"),
            "screen is {open:?}"
        );
        assert!(open.contains("tool1"), "screen is {open:?}");
        assert!(open.contains("tool3"), "screen is {open:?}");
        let effects = app.update(key('y'), now);
        let [
            Effect::StartJob {
                spec: JobSpec::RemoveBrokenSymlinks,
                ..
            },
        ] = &effects[..]
        else {
            panic!("expected the symlink fix to start");
        };
        assert!(app.overlay.is_none());
    }

    #[test]
    fn p_with_two_broken_packages_confirms_then_removes_them() {
        let mut report = health_report();
        report.broken_packages = vec![
            broken_package("obsidian", "/gone/obsidian"),
            broken_package("zellij", "/gone/zellij"),
        ];
        let (mut app, now) = health_app(report);
        let effects = app.update(key('p'), now);
        assert!(effects.is_empty());
        assert!(matches!(app.overlay, Some(Overlay::Confirm(_))));
        let open = screen(&mut app, 80, 24);
        assert!(
            open.contains("Remove 2 broken packages"),
            "screen is {open:?}"
        );
        assert!(
            open.contains("obsidian  /gone/obsidian"),
            "screen is {open:?}"
        );
        assert!(open.contains("zellij  /gone/zellij"), "screen is {open:?}");
        let effects = app.update(key('y'), now);
        let [
            Effect::StartJob {
                spec: JobSpec::RemoveBrokenPackages,
                ..
            },
        ] = &effects[..]
        else {
            panic!("expected the package fix to start");
        };
        assert!(app.overlay.is_none());
    }

    #[test]
    fn p_without_broken_packages_only_leaves_a_note() {
        let (mut app, now) = health_app(health_report());
        let effects = app.update(key('p'), now);
        assert!(effects.is_empty());
        assert!(app.overlay.is_none());
        let toasts = format!("{:?}", app.toasts);
        assert!(
            toasts.contains("No broken packages"),
            "toasts are {toasts:?}"
        );
    }

    #[test]
    fn c_names_the_cache_path_then_cleans_it() {
        let mut app = app();
        let now = Instant::now();
        app.view = ViewId::Health;
        let mut paths = health_paths();
        let Ok(home) = std::env::var("HOME") else {
            return;
        };
        if home.is_empty() {
            return;
        }
        paths[4] = (
            "Cache".to_string(),
            Path::new(&home).join(".local/share/soar/cache"),
        );
        app.update(health_loaded_with_paths(health_report(), paths), now);
        let effects = app.update(key('c'), now);
        assert!(effects.is_empty());
        assert!(matches!(app.overlay, Some(Overlay::Confirm(_))));
        let open = screen(&mut app, 80, 24);
        assert!(open.contains("Clean cache"), "screen is {open:?}");
        assert!(
            open.contains("~/.local/share/soar/cache"),
            "screen is {open:?}"
        );
        assert!(open.contains("[ Clean ]"), "screen is {open:?}");
        let effects = app.update(key('y'), now);
        let [
            Effect::StartJob {
                spec: JobSpec::CleanCache,
                ..
            },
        ] = &effects[..]
        else {
            panic!("expected the cache clean to start");
        };
        assert!(app.overlay.is_none());
    }

    /// A ready health load carrying the given report and paths.
    fn health_loaded_with_paths(report: HealthReport, paths: Vec<(String, PathBuf)>) -> Msg {
        Msg::Loaded(Loaded::Health(Ok((report, paths))))
    }

    #[test]
    fn the_eleventh_confirm_row_folds_into_a_count() {
        let mut report = health_report();
        report.broken_symlinks = (1..=11)
            .map(|n| Path::new("/home/user/.local/bin").join(format!("tool{n}")))
            .collect();
        let (mut app, now) = health_app(report);
        app.update(key('b'), now);
        let open = screen(&mut app, 80, 24);
        assert!(open.contains("tool10"), "screen is {open:?}");
        assert!(open.contains("… and 1 more"), "screen is {open:?}");
        assert!(!open.contains("tool11"), "screen is {open:?}");
    }

    #[test]
    fn the_fixes_do_nothing_while_the_report_loads() {
        let mut app = app();
        app.view = ViewId::Health;
        let now = Instant::now();
        for pressed in ['b', 'p', 'c'] {
            let effects = app.update(key(pressed), now);
            assert!(effects.is_empty());
            assert!(app.overlay.is_none());
        }
    }

    /// The apply view with the given plan loaded.
    fn apply_app(plan: ApplyPlan) -> (App, Instant) {
        let mut app = app();
        app.view = ViewId::Apply;
        app.update(apply_loaded(plan), Instant::now());
        (app, Instant::now())
    }

    /// A finished computation for the given plan, answering the check the
    /// view still expects before any recompute has started.
    fn apply_loaded(plan: ApplyPlan) -> Msg {
        Msg::Loaded(Loaded::ApplyDiff {
            check: 0,
            result: Ok(plan),
        })
    }

    /// A plan whose only change is removing the named package.
    fn removal_plan(name: &str) -> ApplyPlan {
        let mut pruned = apply_plan();
        if let Some(diff) = pruned.diff.as_mut() {
            diff.to_install.clear();
            diff.to_update.clear();
            diff.to_remove = vec![installed_package(name, "1.7.7", "pkgcache")];
            diff.in_sync.clear();
        }
        pruned
    }

    #[test]
    fn enter_with_a_removal_opens_a_danger_confirm_naming_it() {
        let (mut app, now) = apply_app(removal_plan("obsidian"));
        app.apply.prune = true;
        let effects = app.update(code(KeyCode::Enter), now);
        assert!(effects.is_empty());
        assert!(matches!(app.overlay, Some(Overlay::Confirm(_))));
        let open = screen(&mut app, 80, 24);
        assert!(open.contains("Apply packages.toml"), "screen is {open:?}");
        assert!(open.contains("Remove 1"), "screen is {open:?}");
        assert!(open.contains("obsidian"), "screen is {open:?}");
        assert!(open.contains("[ Apply ]"), "screen is {open:?}");
        let effects = app.update(code(KeyCode::Enter), now);
        assert!(effects.is_empty());
        assert!(app.overlay.is_none());
        assert!(app.jobs.list.is_empty());
    }

    #[test]
    fn accepting_the_confirm_moves_the_diff_into_the_job() {
        let (mut app, now) = apply_app(apply_plan());
        app.update(code(KeyCode::Enter), now);
        let effects = app.update(key('y'), now);
        let [
            Effect::StartJob {
                spec:
                    JobSpec::Apply {
                        diff,
                        packages_path,
                    },
                ..
            },
        ] = &effects[..]
        else {
            panic!("expected the apply to start");
        };
        assert_eq!(diff.to_install.len(), 1);
        assert_eq!(diff.to_update.len(), 1);
        assert_eq!(packages_path, "/home/user/.config/soar/packages.toml");
        assert_eq!(app.jobs.list[0].kind, JobKind::Apply);
        assert!(app.apply.plan.is_loading());
        assert!(app.overlay.is_none());
    }

    #[test]
    fn p_flips_prune_and_recomputes_with_the_new_value() {
        let (mut app, now) = apply_app(apply_plan());
        assert!(!app.apply.prune);
        let effects = app.update(key('p'), now);
        let [
            Effect::ComputeApplyDiff {
                prune, ..
            },
        ] = &effects[..]
        else {
            panic!("expected a recomputation");
        };
        assert!(*prune);
        assert!(app.apply.prune);
        assert!(app.apply.plan.is_loading());
    }

    #[test]
    fn enter_with_nothing_to_apply_only_leaves_a_note() {
        let mut settled = apply_plan();
        if let Some(diff) = settled.diff.as_mut() {
            diff.to_install.clear();
            diff.to_update.clear();
            diff.in_sync = vec!["fd".to_string()];
        }
        let (mut app, now) = apply_app(settled);
        let effects = app.update(code(KeyCode::Enter), now);
        assert!(effects.is_empty());
        assert!(app.overlay.is_none());
        assert!(app.jobs.list.is_empty());
        let toasts = format!("{:?}", app.toasts);
        assert!(toasts.contains("Nothing to apply"), "toasts are {toasts:?}");
    }

    #[test]
    fn the_confirm_names_the_counts_and_the_system_pass() {
        let mut full = apply_plan();
        full.system = vec!["docker-compose".to_string()];
        if let Some(diff) = full.diff.as_mut() {
            diff.to_install.push(install_entry("jq", "1.7.1"));
        }
        let (mut app, now) = apply_app(full);
        app.update(code(KeyCode::Enter), now);
        let open = screen(&mut app, 80, 24);
        assert!(open.contains("Install 2"), "screen is {open:?}");
        assert!(open.contains("Update 1"), "screen is {open:?}");
        assert!(
            open.contains("System 1 (asks for administrator rights)"),
            "screen is {open:?}"
        );
        assert!(!open.contains("Remove"), "screen is {open:?}");
    }

    #[test]
    fn esc_on_the_apply_confirm_leaves_the_plan_ready_and_unchanged() {
        let (mut app, now) = apply_app(apply_plan());
        app.update(code(KeyCode::Enter), now);
        app.update(code(KeyCode::Esc), now);
        assert!(app.overlay.is_none());
        assert!(app.jobs.list.is_empty());
        let Loadable::Ready(plan) = &app.apply.plan else {
            panic!("expected the plan to stay ready");
        };
        let Some(diff) = plan.diff.as_ref() else {
            panic!("expected the plan to keep its diff");
        };
        assert_eq!(diff.to_install.len(), 1);
        assert!(diff.has_changes());
    }

    #[test]
    fn a_fresh_plan_under_the_apply_confirm_closes_it_with_a_note() {
        let (mut app, now) = apply_app(apply_plan());
        app.update(code(KeyCode::Enter), now);
        assert!(app.overlay.is_some());
        assert_eq!(app.apply_pick, Some(app.apply.check));
        // An edit of the packages file recomputes while the confirm is
        // open, exactly the reload that can land under it.
        app.update(Msg::Edited(app.env.packages_path.clone()), now);
        assert!(app.apply.plan.is_loading());
        let effects = app.update(
            Msg::Loaded(Loaded::ApplyDiff {
                check: app.apply.check,
                result: Ok(apply_plan()),
            }),
            now,
        );
        assert!(effects.is_empty());
        assert!(app.overlay.is_none(), "the confirm cannot outlive its plan");
        assert!(app.apply_pick.is_none());
        assert!(app.jobs.list.is_empty(), "nothing was queued from it");
        let toasts = format!("{:?}", app.toasts);
        assert!(
            toasts.contains("The plan changed, so the confirm closed"),
            "toasts are {toasts:?}"
        );
    }

    #[test]
    fn a_stale_plan_result_under_the_apply_confirm_changes_nothing() {
        let (mut app, now) = apply_app(apply_plan());
        app.update(code(KeyCode::Enter), now);
        assert!(app.overlay.is_some());
        let stale = app.apply.check;
        app.update(Msg::Edited(app.env.packages_path.clone()), now);
        let effects = app.update(
            Msg::Loaded(Loaded::ApplyDiff {
                check: stale,
                result: Ok(removal_plan("obsidian")),
            }),
            now,
        );
        assert!(effects.is_empty());
        assert!(
            app.apply.plan.is_loading(),
            "a stale result must not fill the view"
        );
        assert!(
            app.overlay.is_some(),
            "a stale result leaves the confirm alone"
        );
        assert_eq!(app.apply_pick, Some(stale));
    }

    #[test]
    fn a_finished_apply_recomputes_the_plan_and_reloads_the_lists() {
        let mut app = app();
        app.view = ViewId::Apply;
        app.apply.prune = true;
        let now = Instant::now();
        app.enqueue(
            JobSpec::Apply {
                diff: ApplyDiff::default(),
                packages_path: "packages.toml".to_string(),
            },
            now,
        );
        let effects = app.update(finished(0, true, "Applied packages.toml"), now);
        let [
            Effect::ComputeApplyDiff {
                prune, ..
            },
            Effect::LoadCatalog {
                ..
            },
            Effect::LoadInstalled {
                ..
            },
        ] = &effects[..]
        else {
            panic!("expected the plan to recompute and the lists to reload");
        };
        assert!(*prune);
        assert!(app.apply.plan.is_loading());
    }

    #[test]
    fn a_finished_apply_rechecks_updates_once_loaded() {
        let mut app = app();
        let now = Instant::now();
        app.view = ViewId::Updates;
        app.update(updates_loaded(&app, three_updates()), now);
        app.view = ViewId::Apply;
        app.enqueue(
            JobSpec::Apply {
                diff: ApplyDiff::default(),
                packages_path: "packages.toml".to_string(),
            },
            now,
        );
        let effects = app.update(finished(0, true, "Applied packages.toml"), now);
        assert!(matches!(
            effects.as_slice(),
            [
                Effect::ComputeApplyDiff { .. },
                Effect::LoadCatalog { .. },
                Effect::LoadInstalled { .. },
                Effect::CheckUpdates { .. }
            ]
        ));
    }

    /// A system pass over one declared package.
    fn system_spec() -> JobSpec {
        JobSpec::ApplySystem {
            names: vec!["jq".to_string()],
            packages_path: "packages.toml".to_string(),
            mode: SystemMode::Pending,
        }
    }

    /// The doas binary as the probe would have found it.
    fn doas_tool() -> EscalationTool {
        EscalationTool {
            path: PathBuf::from("/usr/bin/doas"),
            kind: EscalationKind::Doas,
        }
    }

    /// Enqueues a system pass and answers its probe, so the job sits in
    /// the authorizing state.
    fn authorizing_app() -> (App, Instant) {
        let mut app = app();
        let now = Instant::now();
        app.enqueue(system_spec(), now);
        assert!(matches!(app.jobs.list[0].state, JobState::Authorizing));
        (app, now)
    }

    #[test]
    fn a_queued_system_pass_probes_escalation_and_holds_the_queue() {
        let mut app = app();
        let now = Instant::now();
        let effects = app.enqueue(system_spec(), now);
        assert!(matches!(effects.as_slice(), [Effect::ProbeEscalation]));
        assert!(matches!(app.jobs.list[0].state, JobState::Authorizing));
        let effects = app.enqueue(JobSpec::Sync, now);
        assert!(effects.is_empty());
        assert_eq!(app.jobs.list[1].kind, JobKind::Sync);
        assert!(matches!(app.jobs.list[1].state, JobState::Queued));
    }

    #[test]
    fn a_ready_probe_starts_the_pass_directly() {
        let (mut app, now) = authorizing_app();
        let effects = app.update(Msg::Loaded(Loaded::Escalation(EscalationProbe::Ready)), now);
        let [
            Effect::StartJob {
                spec: JobSpec::ApplySystem {
                    mode, ..
                },
                ..
            },
        ] = &effects[..]
        else {
            panic!("expected the pass to start");
        };
        assert!(matches!(mode, SystemMode::Direct));
        assert!(matches!(app.jobs.list[0].state, JobState::Running { .. }));
    }

    #[test]
    fn a_password_probe_opens_the_authorize_dialog() {
        let (mut app, now) = authorizing_app();
        let effects = app.update(
            Msg::Loaded(Loaded::Escalation(EscalationProbe::Password)),
            now,
        );
        assert!(effects.is_empty());
        assert!(matches!(app.overlay, Some(Overlay::Authorize(_))));
        let open = screen(&mut app, 80, 24);
        assert!(
            open.contains("Administrator rights needed"),
            "screen is {open:?}"
        );
        assert!(open.contains("jq"), "screen is {open:?}");
        assert!(open.contains("[ Authorize ]"), "screen is {open:?}");
    }

    #[test]
    fn the_authorize_dialog_drops_the_state_of_the_overlay_it_replaces() {
        let (mut app, now) = authorizing_app();
        let mut bat = catalog_row("bat", "0.24.0", "bincache");
        bat.other_versions = vec!["0.23.0".to_string()];
        app.update(catalog_loaded(vec![bat]), now);
        app.update(key('v'), now);
        assert!(matches!(app.overlay, Some(Overlay::Picker(_))));
        assert!(app.version_pick.is_some());
        app.update(
            Msg::Loaded(Loaded::Escalation(EscalationProbe::Password)),
            now,
        );
        assert!(matches!(app.overlay, Some(Overlay::Authorize(_))));
        assert!(app.version_pick.is_none());
    }

    #[test]
    fn the_terminal_confirm_drops_the_state_of_the_overlay_it_replaces() {
        let (mut app, now) = authorizing_app();
        let mut bat = catalog_row("bat", "0.24.0", "bincache");
        bat.other_versions = vec!["0.23.0".to_string()];
        app.update(catalog_loaded(vec![bat]), now);
        app.update(key('v'), now);
        assert!(matches!(app.overlay, Some(Overlay::Picker(_))));
        assert!(app.version_pick.is_some());
        app.update(
            Msg::Loaded(Loaded::Escalation(EscalationProbe::Terminal(doas_tool()))),
            now,
        );
        assert!(matches!(app.overlay, Some(Overlay::Confirm(_))));
        assert!(app.version_pick.is_none());
    }

    #[test]
    fn a_submitted_password_reaches_the_runner_and_waits() {
        let (mut app, now) = authorizing_app();
        app.update(
            Msg::Loaded(Loaded::Escalation(EscalationProbe::Password)),
            now,
        );
        let effects = app.update(key('x'), now);
        assert!(effects.is_empty());
        let effects = app.update(code(KeyCode::Enter), now);
        let [effect] = &effects[..] else {
            panic!("expected the password check to run");
        };
        assert!(matches!(effect, Effect::Authenticate { .. }));
        assert!(matches!(app.overlay, Some(Overlay::Authorize(_))));
        let open = screen(&mut app, 80, 24);
        assert!(open.contains("Checking password"), "screen is {open:?}");
    }

    #[test]
    fn a_refused_password_shows_the_error_for_another_try() {
        let (mut app, now) = authorizing_app();
        app.update(
            Msg::Loaded(Loaded::Escalation(EscalationProbe::Password)),
            now,
        );
        app.update(key('x'), now);
        app.update(code(KeyCode::Enter), now);
        let check = app.pass_check;
        let effects = app.update(
            Msg::Loaded(Loaded::Authenticated {
                check,
                result: Err("Incorrect password, try again".to_string()),
            }),
            now,
        );
        assert!(effects.is_empty());
        assert!(matches!(app.overlay, Some(Overlay::Authorize(_))));
        let open = screen(&mut app, 80, 24);
        assert!(
            open.contains("Incorrect password, try again"),
            "screen is {open:?}"
        );
        assert!(open.contains("[ Authorize ]"), "screen is {open:?}");
    }

    #[test]
    fn a_refusal_from_a_withdrawn_dialog_spares_the_next_one() {
        let (mut app, now) = authorizing_app();
        app.update(
            Msg::Loaded(Loaded::Escalation(EscalationProbe::Password)),
            now,
        );
        app.update(key('x'), now);
        app.update(code(KeyCode::Enter), now);
        let stale = app.pass_check;
        // The user gives up on the first dialog and starts a second pass.
        app.update(code(KeyCode::Esc), now);
        app.enqueue(system_spec(), now);
        app.update(
            Msg::Loaded(Loaded::Escalation(EscalationProbe::Password)),
            now,
        );
        assert!(matches!(app.overlay, Some(Overlay::Authorize(_))));
        // The first check's refusal arrives under the second dialog.
        app.update(
            Msg::Loaded(Loaded::Authenticated {
                check: stale,
                result: Err("Incorrect password, try again".to_string()),
            }),
            now,
        );
        let open = screen(&mut app, 80, 24);
        assert!(
            !open.contains("Incorrect password"),
            "the refusal belongs to a withdrawn dialog, screen is {open:?}"
        );
        assert!(open.contains("[ Authorize ]"), "screen is {open:?}");
    }

    #[test]
    fn an_accepted_password_starts_the_pass_directly() {
        let (mut app, now) = authorizing_app();
        app.update(
            Msg::Loaded(Loaded::Escalation(EscalationProbe::Password)),
            now,
        );
        app.update(key('x'), now);
        app.update(code(KeyCode::Enter), now);
        let check = app.pass_check;
        let effects = app.update(
            Msg::Loaded(Loaded::Authenticated {
                check,
                result: Ok(()),
            }),
            now,
        );
        let [
            Effect::StartJob {
                spec: JobSpec::ApplySystem {
                    mode, ..
                },
                ..
            },
        ] = &effects[..]
        else {
            panic!("expected the pass to start");
        };
        assert!(matches!(mode, SystemMode::Direct));
        assert!(app.overlay.is_none());
        assert!(matches!(app.jobs.list[0].state, JobState::Running { .. }));
    }

    #[test]
    fn cancelling_the_dialog_skips_the_pass_and_starts_the_next_job() {
        let mut app = app();
        let now = Instant::now();
        app.enqueue(system_spec(), now);
        app.enqueue(JobSpec::Sync, now);
        app.update(
            Msg::Loaded(Loaded::Escalation(EscalationProbe::Password)),
            now,
        );
        let effects = app.update(code(KeyCode::Esc), now);
        let [
            Effect::ComputeApplyDiff {
                ..
            },
            Effect::StartJob {
                spec: JobSpec::Sync,
                ..
            },
        ] = &effects[..]
        else {
            panic!("expected the sync to start, got something else");
        };
        let JobState::Finished {
            outcome, ..
        } = &app.jobs.list[0].state
        else {
            panic!("expected the pass to be over");
        };
        assert!(outcome.aborted);
        assert_eq!(outcome.headline, "System packages skipped");
        assert!(app.overlay.is_none());
    }

    #[test]
    fn a_terminal_probe_confirms_then_hands_over_the_terminal() {
        let (mut app, now) = authorizing_app();
        let effects = app.update(
            Msg::Loaded(Loaded::Escalation(EscalationProbe::Terminal(doas_tool()))),
            now,
        );
        assert!(effects.is_empty());
        assert!(matches!(app.overlay, Some(Overlay::Confirm(_))));
        let open = screen(&mut app, 80, 24);
        assert!(
            open.contains("Administrator rights needed"),
            "screen is {open:?}"
        );
        assert!(
            open.contains("doas will ask for your password in the terminal."),
            "screen is {open:?}"
        );
        assert!(
            open.contains("pauses until it finishes."),
            "screen is {open:?}"
        );
        assert!(open.contains("[ Continue ]"), "screen is {open:?}");
        let effects = app.update(key('y'), now);
        let [
            Effect::Suspend(Suspended::SystemPass {
                id,
                tool,
                packages_path,
                names,
            }),
        ] = &effects[..]
        else {
            panic!("expected the terminal pass, got something else");
        };
        assert_eq!(*id, 0);
        assert_eq!(tool.path, PathBuf::from("/usr/bin/doas"));
        assert_eq!(packages_path, "packages.toml");
        assert_eq!(names, &["jq".to_string()]);
        assert!(app.overlay.is_none());
        assert!(matches!(app.jobs.list[0].state, JobState::Running { .. }));
    }

    #[test]
    fn an_unavailable_probe_fails_the_job_and_starts_the_next() {
        let mut app = app();
        let now = Instant::now();
        app.enqueue(system_spec(), now);
        app.enqueue(JobSpec::Sync, now);
        let effects = app.update(
            Msg::Loaded(Loaded::Escalation(EscalationProbe::Unavailable(
                "no tool here".to_string(),
            ))),
            now,
        );
        let [
            Effect::ComputeApplyDiff {
                ..
            },
            Effect::StartJob {
                spec: JobSpec::Sync,
                ..
            },
        ] = &effects[..]
        else {
            panic!("expected the sync to start, got something else");
        };
        let JobState::Finished {
            outcome, ..
        } = &app.jobs.list[0].state
        else {
            panic!("expected the pass to be over");
        };
        assert!(!outcome.ok);
        assert_eq!(outcome.headline, "Apply 1 system package failed");
        assert!(
            matches!(&outcome.lines[0], OutcomeLine::Failed { error, .. } if error == "no tool here")
        );
    }

    #[test]
    fn ctrl_x_with_a_running_system_pass_says_it_cannot_be_aborted() {
        let (mut app, now) = authorizing_app();
        app.update(Msg::Loaded(Loaded::Escalation(EscalationProbe::Ready)), now);
        let effects = app.update(ctrl('x'), now);
        assert!(effects.is_empty());
        assert!(app.overlay.is_none());
        let toasts = format!("{:?}", app.toasts);
        assert!(
            toasts.contains("System apply cannot be aborted"),
            "toasts are {toasts:?}"
        );
    }

    #[test]
    fn quitting_while_a_pass_authorizes_opens_the_guard() {
        let (mut app, now) = authorizing_app();
        app.update(key('q'), now);
        assert!(!app.quit, "the pass must not be dropped silently");
        assert!(matches!(app.overlay, Some(Overlay::QuitGuard(_))));
        let open = screen(&mut app, 80, 24);
        assert!(
            open.contains("Apply 1 system package"),
            "screen is {open:?}"
        );
    }

    #[test]
    fn waiting_to_quit_over_a_pass_alone_withdraws_it_and_quits() {
        let (mut app, now) = authorizing_app();
        app.update(
            Msg::Loaded(Loaded::Escalation(EscalationProbe::Password)),
            now,
        );
        assert!(matches!(app.overlay, Some(Overlay::Authorize(_))));
        let effects = app.wait_and_quit(now, false);
        assert!(effects.is_empty());
        assert!(app.quit, "nothing is left to wait for");
        assert!(
            app.overlay.is_none(),
            "the dialog has no pass left to serve"
        );
        assert!(app.jobs.list.is_empty());
    }

    #[test]
    fn abort_and_quit_leaves_a_system_pass_running_to_its_end() {
        let (mut app, now) = authorizing_app();
        app.update(Msg::Loaded(Loaded::Escalation(EscalationProbe::Ready)), now);
        let effects = app.wait_and_quit(now, true);
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, Effect::AbortJob)),
            "the system pass cannot be aborted, so no abort may be sent"
        );
        assert!(app.quit_when_idle);
        let JobState::Running {
            aborting, ..
        } = app.jobs.list[0].state
        else {
            panic!("the pass keeps running");
        };
        assert!(!aborting);
    }

    #[test]
    fn the_apply_confirm_queues_the_system_pass_behind_the_user_packages() {
        let mut full = apply_plan();
        full.system = vec!["jq".to_string()];
        let mut app = app();
        app.view = ViewId::Apply;
        let now = Instant::now();
        app.update(
            Msg::Loaded(Loaded::ApplyDiff {
                check: 0,
                result: Ok(full),
            }),
            now,
        );
        app.update(code(KeyCode::Enter), now);
        let effects = app.update(key('y'), now);
        let [
            Effect::StartJob {
                spec: JobSpec::Apply {
                    ..
                },
                ..
            },
        ] = &effects[..]
        else {
            panic!("expected the user apply to start");
        };
        assert_eq!(app.jobs.list.len(), 2);
        assert_eq!(app.jobs.list[0].kind, JobKind::Apply);
        assert_eq!(app.jobs.list[1].kind, JobKind::ApplySystem);
        assert!(matches!(app.jobs.list[1].state, JobState::Queued));
    }

    #[test]
    fn a_finished_user_apply_probes_the_system_pass_turn() {
        let mut full = apply_plan();
        full.system = vec!["jq".to_string()];
        let mut app = app();
        app.view = ViewId::Apply;
        let now = Instant::now();
        app.update(
            Msg::Loaded(Loaded::ApplyDiff {
                check: 0,
                result: Ok(full),
            }),
            now,
        );
        app.update(code(KeyCode::Enter), now);
        app.update(key('y'), now);
        let effects = app.update(finished(0, true, "Applied: 1 installed"), now);
        assert!(matches!(
            effects.as_slice(),
            [_, _, _, Effect::ProbeEscalation]
        ));
        assert!(matches!(app.jobs.list[1].state, JobState::Authorizing));
    }

    #[test]
    fn a_result_from_a_replaced_check_is_dropped() {
        let mut app = app();
        let now = Instant::now();
        app.update(key('3'), now);
        let replaced = app.updates.check;
        app.update(ctrl('r'), now);
        assert_eq!(app.updates.check, replaced.wrapping_add(1));
        let effects = app.update(
            Msg::Loaded(Loaded::Updates {
                check: replaced,
                result: Ok(Vec::new()),
            }),
            now,
        );
        assert!(effects.is_empty());
        assert!(
            app.updates.list.is_loading(),
            "a stale result must not fill the list"
        );
    }

    #[test]
    fn a_catalog_reply_from_a_replaced_load_is_dropped() {
        let mut app = app();
        let now = Instant::now();
        app.update(ctrl('r'), now);
        let replaced = app.catalog_load;
        app.update(ctrl('r'), now);
        let effects = app.update(
            Msg::Loaded(Loaded::Catalog {
                check: replaced,
                result: Ok(vec![catalog_row("bat", "0.24.0", "bincache")]),
            }),
            now,
        );
        assert!(effects.is_empty());
        assert!(
            app.browse.catalog.is_loading(),
            "a stale reply must not fill the catalog"
        );
        assert!(app.updates.list.is_idle(), "a stale reply starts no check");
        let effects = app.update(
            Msg::Loaded(Loaded::Catalog {
                check: app.catalog_load,
                result: Ok(vec![catalog_row("bat", "0.24.0", "bincache")]),
            }),
            now,
        );
        assert!(matches!(effects.as_slice(), [Effect::CheckUpdates { .. }]));
        assert!(app.browse.catalog.ready().is_some());
    }

    #[test]
    fn an_installed_reply_from_a_replaced_load_is_dropped() {
        let mut app = app();
        app.view = ViewId::Installed;
        let now = Instant::now();
        app.update(ctrl('r'), now);
        let replaced = app.installed_load;
        app.update(ctrl('r'), now);
        app.update(
            Msg::Loaded(Loaded::Installed {
                check: replaced,
                result: Ok(InstalledListResult {
                    packages: Vec::new(),
                    total_count: 0,
                    total_size: 0,
                }),
            }),
            now,
        );
        assert!(
            app.installed.list.is_loading(),
            "a stale reply must not fill the list"
        );
        app.update(
            Msg::Loaded(Loaded::Installed {
                check: app.installed_load,
                result: Ok(InstalledListResult {
                    packages: vec![installed_entry("bat", "0.24.0", "bincache")],
                    total_count: 1,
                    total_size: 0,
                }),
            }),
            now,
        );
        assert!(app.installed.list.ready().is_some());
    }

    #[test]
    fn a_fresh_check_under_the_update_confirm_withdraws_it() {
        let mut app = app();
        let now = Instant::now();
        app.update(key('3'), now);
        app.update(updates_loaded(&app, three_updates()), now);
        app.update(key('u'), now);
        assert!(app.update_pick.is_some());
        assert!(matches!(app.overlay, Some(Overlay::Confirm(_))));
        app.update(key('3'), now);
        app.update(updates_loaded(&app, Vec::new()), now);
        assert!(app.update_pick.is_none(), "the moved list invalidates it");
        assert!(app.overlay.is_none(), "the confirm cannot be trusted");
        let effects = app.update(key('y'), now);
        assert!(effects.is_empty(), "nothing is queued from a dead confirm");
    }

    #[test]
    fn a_recheck_started_under_the_update_confirm_closes_it_with_a_note() {
        let mut app = app();
        let now = Instant::now();
        let started = app.enqueue(JobSpec::Sync, now);
        assert!(matches!(started.as_slice(), [Effect::StartJob { .. }]));
        app.update(key('3'), now);
        app.update(updates_loaded(&app, three_updates()), now);
        app.update(key('u'), now);
        assert!(app.update_pick.is_some());
        assert!(matches!(app.overlay, Some(Overlay::Confirm(_))));
        // The job's finish starts a recheck while the confirm is open.
        let effects = app.update(finished(0, true, "Synced 3 repositories"), now);
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, Effect::CheckUpdates { .. })),
            "the finished job starts the recheck"
        );
        assert!(app.update_pick.is_none());
        assert!(app.overlay.is_none(), "the confirm cannot outlive its list");
        assert!(app.updates.list.is_loading());
        let toasts = format!("{:?}", app.toasts);
        assert!(
            toasts.contains("the confirm closed"),
            "toasts are {toasts:?}"
        );
    }

    #[test]
    fn an_update_accepted_while_the_list_rechecks_toasts_and_queues_nothing() {
        let mut app = app();
        let now = Instant::now();
        app.update(key('3'), now);
        app.update(updates_loaded(&app, three_updates()), now);
        app.update(ctrl('r'), now);
        app.update_pick = Some(vec![0]);
        let effects = app.update_now(now);
        assert!(effects.is_empty(), "nothing can update a moving list");
        assert!(app.update_pick.is_none());
        assert!(app.updates.list.is_loading());
        let toasts = format!("{:?}", app.toasts);
        assert!(toasts.contains("rechecked"), "toasts are {toasts:?}");
    }

    #[test]
    fn a_stale_authenticated_answer_changes_nothing() {
        let mut app = app();
        let now = Instant::now();
        let effects = app.update(
            Msg::Loaded(Loaded::Authenticated {
                check: 7,
                result: Ok(()),
            }),
            now,
        );
        assert!(effects.is_empty());
        assert!(app.overlay.is_none());
    }

    /// A pointer event at the given cell.
    fn mouse(kind: MouseEventKind, column: u16, row: u16) -> Msg {
        Msg::Input(Event::Mouse(MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }))
    }

    /// A catalog of `count` rows, loaded, so the list has rows to hit.
    fn catalog(count: usize) -> Msg {
        let rows: Vec<CatalogRow> = (0..count)
            .map(|n| catalog_row(&format!("pkg{n}"), "1.0.0", "bincache"))
            .collect();
        catalog_loaded(rows)
    }

    /// A confirm naming one package, as the removal flow opens it.
    fn remove_confirm() -> Overlay {
        Overlay::Confirm(Confirm::new(
            "Remove jq",
            vec![Line::raw("jq 1.7.1")],
            "Remove",
            true,
            OnAccept::Test,
        ))
    }

    #[test]
    fn an_install_resolution_cannot_displace_a_password_dialog() {
        let (mut app, now) = authorizing_app();
        app.update(
            Msg::Loaded(Loaded::Escalation(EscalationProbe::Password)),
            now,
        );
        app.update(
            resolved(vec![Resolution::Ready(vec![target(package(
                "bat", "0.24.0", "bincache",
            ))])]),
            now,
        );
        assert!(
            matches!(app.overlay, Some(Overlay::Authorize(_))),
            "the pass dialog keeps its slot"
        );
        assert!(app.pending.is_none(), "the install waits for a retry");
    }

    #[test]
    fn a_left_click_on_the_second_tab_switches_to_installed() {
        let mut app = app();
        screen(&mut app, 100, 24);
        let (rect, view) = app.layout.tabs[1];
        assert_eq!(view, ViewId::Installed);
        let now = Instant::now();
        let effects = app.update(
            mouse(MouseEventKind::Down(MouseButton::Left), rect.x, rect.y),
            now,
        );
        assert!(effects.is_empty());
        assert_eq!(app.view, ViewId::Installed);
    }

    #[test]
    fn a_tab_click_enters_the_view_like_the_number_keys() {
        let mut app = app();
        screen(&mut app, 100, 24);
        let (rect, view) = app.layout.tabs[4];
        assert_eq!(view, ViewId::Repos);
        let now = Instant::now();
        let effects = app.update(
            mouse(MouseEventKind::Down(MouseButton::Left), rect.x, rect.y),
            now,
        );
        assert_eq!(app.view, ViewId::Repos);
        assert!(matches!(effects.as_slice(), [Effect::LoadRepos]));
    }

    #[test]
    fn a_wheel_event_scrolls_the_list_by_three_rows() {
        let mut app = app();
        app.update(catalog(10), Instant::now());
        screen(&mut app, 80, 24);
        let (rect, offset) = app.layout.list.expect("the list hit area after a render");
        assert_eq!(offset, 0);
        let now = Instant::now();
        let effects = app.update(mouse(MouseEventKind::ScrollDown, rect.x, rect.y), now);
        assert!(effects.is_empty());
        assert_eq!(app.browse.cursor.selected, 3);
        app.update(mouse(MouseEventKind::ScrollUp, rect.x, rect.y), now);
        assert_eq!(app.browse.cursor.selected, 0);
        app.update(mouse(MouseEventKind::ScrollUp, rect.x, rect.y), now);
        assert_eq!(app.browse.cursor.selected, 0);
    }

    #[test]
    fn a_wheel_outside_the_rows_moves_nothing() {
        let mut app = app();
        app.update(catalog(10), Instant::now());
        screen(&mut app, 80, 24);
        let (rect, _) = app.layout.list.expect("the list hit area after a render");
        let now = Instant::now();
        app.update(mouse(MouseEventKind::ScrollDown, rect.x, rect.y - 1), now);
        assert_eq!(app.browse.cursor.selected, 0);
    }

    #[test]
    fn the_wheel_scrolls_the_full_screen_detail() {
        let mut app = app();
        let now = Instant::now();
        let mut row = catalog_row("bat", "0.24.0", "bincache");
        row.description = "word ".repeat(80);
        app.update(catalog_loaded(vec![row]), now);
        app.update(code(KeyCode::Enter), now);
        screen(&mut app, 80, 24);
        let rect = app.layout.detail.expect("the detail hit area");
        app.update(mouse(MouseEventKind::ScrollDown, rect.x, rect.y), now);
        assert_eq!(
            app.browse.detail.selected, 3,
            "the detail follows the wheel"
        );
        app.update(mouse(MouseEventKind::ScrollUp, rect.x, rect.y), now);
        assert_eq!(app.browse.detail.selected, 0);
    }

    #[test]
    fn the_wheel_scrolls_the_log_pane_under_the_pointer() {
        let mut app = app();
        let now = Instant::now();
        app.enqueue(JobSpec::Sync, now);
        for n in 0..60 {
            app.log
                .push(crate::log::LogLevel::Info, format!("line {n}"));
        }
        app.update(key('7'), now);
        screen(&mut app, 80, 24);
        let rect = app.layout.log.expect("the log hit area");
        let following_end = app.log.cursor.selected;
        app.update(mouse(MouseEventKind::ScrollUp, rect.x, rect.y), now);
        assert!(!app.log.follow, "scrolling up stops following the end");
        assert_eq!(
            app.log.cursor.selected,
            following_end.saturating_sub(3),
            "the log follows the wheel"
        );
    }

    #[test]
    fn a_click_on_a_list_row_moves_the_cursor_there() {
        let mut app = app();
        app.update(catalog(5), Instant::now());
        screen(&mut app, 100, 24);
        let (rect, offset) = app.layout.list.expect("the list hit area after a render");
        let now = Instant::now();
        let effects = app.update(
            mouse(
                MouseEventKind::Down(MouseButton::Left),
                rect.x,
                rect.y + u16::try_from(offset).unwrap() + 2,
            ),
            now,
        );
        assert!(effects.is_empty());
        assert!(!app.browse.detail_open);
        assert_eq!(app.browse.cursor.selected, offset + 2);
    }

    #[test]
    fn a_click_past_the_last_list_row_moves_nothing() {
        let mut app = app();
        app.update(catalog(3), Instant::now());
        screen(&mut app, 80, 24);
        let (rect, offset) = app.layout.list.expect("the list hit area after a render");
        let now = Instant::now();
        app.update(
            mouse(
                MouseEventKind::Down(MouseButton::Left),
                rect.x,
                rect.y + u16::try_from(offset).unwrap() + 10,
            ),
            now,
        );
        assert_eq!(app.browse.cursor.selected, 0);
    }

    #[test]
    fn a_click_on_the_cursor_row_toggles_the_browse_detail() {
        let mut app = app();
        app.update(catalog(5), Instant::now());
        screen(&mut app, 100, 24);
        let (rect, offset) = app.layout.list.expect("the list hit area after a render");
        let now = Instant::now();
        let effects = app.update(
            mouse(
                MouseEventKind::Down(MouseButton::Left),
                rect.x,
                rect.y + u16::try_from(offset).unwrap(),
            ),
            now,
        );
        assert!(effects.is_empty());
        assert!(app.browse.detail_open);
        screen(&mut app, 100, 24);
        assert!(app.layout.list.is_none());
        app.update(
            mouse(
                MouseEventKind::Down(MouseButton::Left),
                rect.x,
                rect.y + u16::try_from(offset).unwrap(),
            ),
            now,
        );
        assert!(app.browse.detail_open);
    }

    #[test]
    fn a_click_on_the_cursor_row_toggles_the_installed_detail() {
        let mut app = app();
        app.view = ViewId::Installed;
        let entries: Vec<_> = (0..3)
            .map(|n| installed_entry(&format!("pkg{n}"), "1.0.0", "bincache"))
            .collect();
        app.update(installed_loaded(entries), Instant::now());
        screen(&mut app, 100, 24);
        let (rect, offset) = app.layout.list.expect("the list hit area after a render");
        let now = Instant::now();
        app.update(
            mouse(
                MouseEventKind::Down(MouseButton::Left),
                rect.x,
                rect.y + u16::try_from(offset).unwrap(),
            ),
            now,
        );
        assert!(app.installed.detail_open);
    }

    #[test]
    fn the_hit_areas_reset_between_frames() {
        let mut app = app();
        app.update(catalog(5), Instant::now());
        screen(&mut app, 80, 24);
        assert!(app.layout.list.is_some());
        app.view = ViewId::Jobs;
        screen(&mut app, 80, 24);
        assert!(app.layout.list.is_none());
        assert!(app.layout.overlay.is_none());
    }

    #[test]
    fn a_click_outside_a_confirm_closes_it_like_esc() {
        let mut app = app();
        app.overlay = Some(remove_confirm());
        screen(&mut app, 80, 24);
        let rect = app
            .layout
            .overlay
            .expect("the overlay hit area after a render");
        assert!(rect.y > 0);
        let now = Instant::now();
        let effects = app.update(mouse(MouseEventKind::Down(MouseButton::Left), 0, 0), now);
        assert!(effects.is_empty());
        assert!(app.overlay.is_none());
    }

    #[test]
    fn a_click_inside_a_confirm_keeps_it_open() {
        let mut app = app();
        app.overlay = Some(remove_confirm());
        screen(&mut app, 80, 24);
        let rect = app
            .layout
            .overlay
            .expect("the overlay hit area after a render");
        let now = Instant::now();
        app.update(
            mouse(MouseEventKind::Down(MouseButton::Left), rect.x, rect.y),
            now,
        );
        assert!(matches!(app.overlay, Some(Overlay::Confirm(_))));
    }

    #[test]
    fn a_click_outside_a_picker_closes_it_and_drops_what_it_kept() {
        let mut app = app();
        app.run_pick = Some((vec!["--verbose".to_string()], Vec::new()));
        app.overlay = Some(Overlay::Picker(Picker::new(
            "Pick a variant",
            vec![Line::raw("jq 1.7.1")],
            OnAccept::Test,
        )));
        screen(&mut app, 80, 24);
        let now = Instant::now();
        app.update(mouse(MouseEventKind::Down(MouseButton::Left), 0, 0), now);
        assert!(app.overlay.is_none());
        assert!(app.run_pick.is_none());
    }

    #[test]
    fn mouse_events_do_nothing_while_a_prompt_is_open() {
        let mut app = app();
        app.update(catalog(5), Instant::now());
        app.overlay = Some(Overlay::Prompt(Prompt::new(
            "Install by name, URL, or path",
            OnAccept::Test,
        )));
        screen(&mut app, 80, 24);
        let rect = app
            .layout
            .overlay
            .expect("the overlay hit area after a render");
        let now = Instant::now();
        app.update(mouse(MouseEventKind::ScrollDown, rect.x, rect.y), now);
        app.update(mouse(MouseEventKind::Down(MouseButton::Left), 0, 0), now);
        app.update(
            mouse(MouseEventKind::Down(MouseButton::Left), rect.x, rect.y),
            now,
        );
        assert!(matches!(app.overlay, Some(Overlay::Prompt(_))));
        assert_eq!(app.browse.cursor.selected, 0);
    }
}
