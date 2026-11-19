//! The job queue: what runs, one operation at a time, and what each job
//! reports when it is over.

use std::time::Instant;

use soar_config::repository::Repository;
use soar_core::{
    database::models::{InstalledPackage, Package},
    package::install::InstallTarget,
};
use soar_operations::{ApplyDiff, InstallOptions, UpdateInfo};

use crate::progress::Tracker;

/// Identifies a job within the session.
pub(crate) type JobId = u64;

/// The install options the interface holds while arranging an install. The
/// runner turns them into [`soar_operations::InstallOptions`], whose other
/// fields keep their defaults.
#[derive(Debug, Clone, Default)]
pub(crate) struct InstallChoices {
    /// Install again even when the package is already installed.
    pub(crate) force: bool,
    /// Keep only the binary, skipping desktop integration and extras.
    pub(crate) binary_only: bool,
    /// Skip checksum verification.
    pub(crate) no_verify: bool,
    /// Install into the named portable directory instead of the shared tree.
    pub(crate) portable: Option<String>,
}

impl InstallChoices {
    /// The operation's options for these choices. The fields the interface
    /// does not hold keep the operation's defaults.
    pub(crate) fn options(&self) -> InstallOptions {
        InstallOptions {
            force: self.force,
            binary_only: self.binary_only,
            no_verify: self.no_verify,
            portable: self.portable.clone(),
            ..Default::default()
        }
    }
}

/// A mutating operation and everything it needs to run. The views build
/// specs as their actions are accepted; until then only tests do.
#[allow(dead_code)]
pub(crate) enum JobSpec {
    /// Installs the targets resolved for the packages the user picked.
    Install {
        /// The resolved install plans, one per package.
        targets: Vec<InstallTarget>,
        /// The catalog packages behind the targets, empty for installs from
        /// a URL or a local path.
        picked: Vec<Package>,
        /// The options the install form gathered.
        options: InstallChoices,
    },
    /// Removes the installed packages the user marked.
    Remove {
        /// The database records of the packages to remove.
        packages: Vec<InstalledPackage>,
    },
    /// Applies the listed updates, optionally keeping the old versions.
    Update {
        /// The checked updates to apply.
        updates: Vec<UpdateInfo>,
        /// Whether superseded versions stay on disk.
        keep_old: bool,
    },
    /// Links another installed variant of a package as the active one.
    Switch {
        /// The database id of the variant to link.
        id: u64,
        /// The package name, for the job's title.
        name: String,
    },
    /// Refetches every enabled repository's metadata.
    Sync,
    /// Applies the declarative packages.toml.
    Apply {
        /// The plan computed beforehand, with everything to change.
        diff: ApplyDiff,
        /// Where the declared packages are written.
        packages_path: String,
    },
    /// Applies the packages marked `system = true` through a privileged
    /// child. The authorization for it happens when the job's turn comes,
    /// before anything runs.
    ApplySystem {
        /// The declared packages the system pass applies.
        names: Vec<String>,
        /// Where the declared packages are written.
        packages_path: String,
        /// How the pass runs, decided by what the escalation probe found.
        mode: SystemMode,
    },
    /// Clears the download cache.
    CleanCache,
    /// Deletes links in the bin directory that point nowhere.
    RemoveBrokenSymlinks,
    /// Removes installed packages whose files are gone.
    RemoveBrokenPackages,
    /// Changes the repository list in the config file.
    Repo(RepoChange),
    /// Writes the default packages.toml when it is missing.
    CreatePackagesConfig,
}

impl JobSpec {
    /// The kind of job this spec runs, kept after the spec is consumed.
    pub(crate) fn kind(&self) -> JobKind {
        match self {
            JobSpec::Install {
                ..
            } => JobKind::Install,
            JobSpec::Remove {
                ..
            } => JobKind::Remove,
            JobSpec::Update {
                ..
            } => JobKind::Update,
            JobSpec::Switch {
                ..
            } => JobKind::Switch,
            JobSpec::Sync => JobKind::Sync,
            JobSpec::Apply {
                ..
            } => JobKind::Apply,
            JobSpec::ApplySystem {
                ..
            } => JobKind::ApplySystem,
            JobSpec::CleanCache => JobKind::CleanCache,
            JobSpec::RemoveBrokenSymlinks => JobKind::RemoveBrokenSymlinks,
            JobSpec::RemoveBrokenPackages => JobKind::RemoveBrokenPackages,
            JobSpec::Repo(_) => JobKind::Repo,
            JobSpec::CreatePackagesConfig => JobKind::CreatePackagesConfig,
        }
    }

    /// The short name a job lists under.
    pub(crate) fn title(&self) -> String {
        match self {
            JobSpec::Install {
                targets,
                picked,
                ..
            } => {
                let count = if picked.is_empty() {
                    targets.len()
                } else {
                    picked.len()
                };
                if count > 1 {
                    format!("Install {count} packages")
                } else if let Some(pkg) = picked.first() {
                    format!("Install {}", pkg.pkg_name)
                } else if let Some(target) = targets.first() {
                    format!("Install {}", target.package.pkg_name)
                } else {
                    "Install package".to_string()
                }
            }
            JobSpec::Remove {
                packages,
            } => {
                match packages.as_slice() {
                    [one] => format!("Remove {}", one.pkg_name),
                    many => format!("Remove {} packages", many.len()),
                }
            }
            JobSpec::Update {
                updates, ..
            } => {
                match updates.as_slice() {
                    [one] => format!("Update {}", one.pkg_name),
                    many => format!("Update {} packages", many.len()),
                }
            }
            JobSpec::Switch {
                name, ..
            } => format!("Switch to {name}"),
            JobSpec::Sync => "Sync repositories".to_string(),
            JobSpec::Apply {
                ..
            } => "Apply packages.toml".to_string(),
            JobSpec::ApplySystem {
                names, ..
            } => {
                match names.len() {
                    1 => "Apply 1 system package".to_string(),
                    count => format!("Apply {count} system packages"),
                }
            }
            JobSpec::CleanCache => "Clean cache".to_string(),
            JobSpec::RemoveBrokenSymlinks => "Remove broken symlinks".to_string(),
            JobSpec::RemoveBrokenPackages => "Remove broken packages".to_string(),
            JobSpec::Repo(RepoChange::Add(repo)) => format!("Add repository {}", repo.name),
            JobSpec::Repo(RepoChange::SetEnabled {
                name,
                enabled: true,
            }) => {
                format!("Enable {name}")
            }
            JobSpec::Repo(RepoChange::SetEnabled {
                name,
                enabled: false,
            }) => {
                format!("Disable {name}")
            }
            JobSpec::Repo(RepoChange::Remove(name)) => format!("Remove repository {name}"),
            JobSpec::CreatePackagesConfig => "Create packages.toml".to_string(),
        }
    }
}

/// A change to the repository list in the config file. The repositories
/// view builds these once it exists; until then nothing constructs them.
#[allow(dead_code)]
pub(crate) enum RepoChange {
    /// Adds a repository to the config.
    Add(Repository),
    /// Enables or disables an existing repository.
    SetEnabled {
        /// The repository to change.
        name: String,
        /// Whether it becomes enabled.
        enabled: bool,
    },
    /// Removes a repository from the config.
    Remove(String),
}

/// How the system pass runs. A new job starts `Pending`; the escalation
/// probe sets the mode when the job's turn comes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SystemMode {
    /// Not decided yet: the probe runs first.
    Pending,
    /// Through the escalation tool with its input and output captured, so
    /// progress shows in the interface.
    Direct,
    /// With the terminal handed over, so the tool asks for a password
    /// itself.
    Terminal,
}

/// What kind of job ran, kept after the spec is consumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JobKind {
    Install,
    Remove,
    Update,
    Switch,
    Sync,
    Apply,
    ApplySystem,
    CleanCache,
    RemoveBrokenSymlinks,
    RemoveBrokenPackages,
    Repo,
    CreatePackagesConfig,
}

/// Where a job is in its life. The job views read the timings and the
/// outcome once they exist; until then the queue only writes them.
pub(crate) enum JobState {
    /// Waiting for its turn in the queue.
    Queued,
    /// Its turn has come, but it waits for the user to authorize the
    /// administrator pass. The spec stays with the job, and the queue
    /// holds still until the authorization ends one way or another.
    Authorizing,
    /// Running, with when it started and whether an abort was asked for.
    Running {
        /// When the job began running.
        started: Instant,
        /// Whether the user asked it to stop.
        aborting: bool,
    },
    /// Over, with when it ran and what it reports.
    Finished {
        /// When the job began running.
        started: Instant,
        /// When the job finished.
        ended: Instant,
        /// What the job reports.
        outcome: JobOutcome,
    },
}

/// One entry in the job list: what runs or ran, under what name, and how it
/// went.
pub(crate) struct Job {
    /// The number identifying the job in the queue and its messages.
    pub(crate) id: JobId,
    /// The kind of operation the job runs, kept after the spec is taken.
    pub(crate) kind: JobKind,
    /// The short name the job lists under, in the job list and the activity
    /// strip.
    pub(crate) title: String,
    /// Where the job is in its life.
    pub(crate) state: JobState,
    /// What to run. Held while the job is queued, taken when it starts.
    pub(crate) spec: Option<JobSpec>,
    /// The per-package progress the operation reported so far.
    pub(crate) tracker: Tracker,
}

/// What a finished job reports, independent of which operation ran.
pub(crate) struct JobOutcome {
    /// Whether everything the job touched succeeded.
    pub(crate) ok: bool,
    /// Whether the job stopped early because the user aborted it. The job
    /// views color such jobs as warnings.
    pub(crate) aborted: bool,
    /// The one-line summary the toast and the activity strip show.
    pub(crate) headline: String,
    /// The per-package lines the job views list.
    pub(crate) lines: Vec<OutcomeLine>,
}

/// One line of a finished job's report. The runner fills these from the
/// operation reports and the job views list them.
pub(crate) enum OutcomeLine {
    /// A package that succeeded, as `name version`.
    Done(String),
    /// A package passed over without running.
    Skipped(String),
    /// A package that failed, with its name and the error.
    Failed {
        /// The package that failed.
        name: String,
        /// What went wrong.
        error: String,
    },
    /// A note about a package, such as where its configuration lives.
    Note {
        /// The package the note is about.
        name: String,
        /// What the note says.
        text: String,
    },
    /// Something worth knowing that is not about one package.
    Warning(String),
}

/// The jobs of the session, in the order they were queued, with the flag
/// marking an unseen failure.
pub(crate) struct Jobs {
    /// Every job of the session, oldest first.
    pub(crate) list: Vec<Job>,
    /// The number the next job gets.
    pub(crate) next_id: JobId,
    /// Whether the last job failed without being seen.
    pub(crate) attention: bool,
}

impl Jobs {
    /// Creates the empty queue the session starts with.
    pub(crate) fn new() -> Jobs {
        Jobs {
            list: Vec::new(),
            next_id: 0,
            attention: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use soar_core::package::install::InstallTarget;

    use super::*;
    use crate::test_support::{installed_package, package};

    fn repo(name: &str) -> Repository {
        Repository {
            name: name.to_string(),
            url: String::new(),
            desktop_integration: None,
            pubkey: None,
            enabled: None,
            signature_verification: None,
            sync_interval: None,
        }
    }

    fn update_info(name: &str) -> UpdateInfo {
        UpdateInfo {
            pkg_name: name.to_string(),
            repo_name: "bincache".to_string(),
            current_version: "1.0.0".to_string(),
            new_version: "1.1.0".to_string(),
            target: InstallTarget::default(),
            update_toml_url: None,
        }
    }

    #[test]
    fn specs_carry_kind_and_title() {
        let cases: Vec<(JobSpec, JobKind, &str)> = vec![
            (
                JobSpec::Install {
                    targets: Vec::new(),
                    picked: vec![package("bat", "0.24.0", "bincache")],
                    options: InstallChoices::default(),
                },
                JobKind::Install,
                "Install bat",
            ),
            (
                JobSpec::Install {
                    targets: Vec::new(),
                    picked: vec![
                        package("bat", "0.24.0", "bincache"),
                        package("fd", "10.2.0", "bincache"),
                        package("jq", "1.7.1", "bincache"),
                    ],
                    options: InstallChoices::default(),
                },
                JobKind::Install,
                "Install 3 packages",
            ),
            (
                JobSpec::Install {
                    targets: vec![InstallTarget {
                        package: package("bat", "0.24.0", "bincache"),
                        ..Default::default()
                    }],
                    picked: Vec::new(),
                    options: InstallChoices::default(),
                },
                JobKind::Install,
                "Install bat",
            ),
            (
                JobSpec::Remove {
                    packages: vec![installed_package("obsidian", "1.7.7", "pkgcache")],
                },
                JobKind::Remove,
                "Remove obsidian",
            ),
            (
                JobSpec::Remove {
                    packages: vec![
                        installed_package("obsidian", "1.7.7", "pkgcache"),
                        installed_package("zellij", "0.41.2", "bincache"),
                    ],
                },
                JobKind::Remove,
                "Remove 2 packages",
            ),
            (
                JobSpec::Update {
                    updates: vec![update_info("yq")],
                    keep_old: false,
                },
                JobKind::Update,
                "Update yq",
            ),
            (
                JobSpec::Update {
                    updates: vec![update_info("yq"), update_info("fd")],
                    keep_old: false,
                },
                JobKind::Update,
                "Update 2 packages",
            ),
            (
                JobSpec::Switch {
                    id: 4,
                    name: "ffmpeg".to_string(),
                },
                JobKind::Switch,
                "Switch to ffmpeg",
            ),
            (JobSpec::Sync, JobKind::Sync, "Sync repositories"),
            (
                JobSpec::Apply {
                    diff: ApplyDiff::default(),
                    packages_path: "packages.toml".to_string(),
                },
                JobKind::Apply,
                "Apply packages.toml",
            ),
            (
                JobSpec::ApplySystem {
                    names: vec!["jq".to_string()],
                    packages_path: "packages.toml".to_string(),
                    mode: SystemMode::Pending,
                },
                JobKind::ApplySystem,
                "Apply 1 system package",
            ),
            (
                JobSpec::ApplySystem {
                    names: vec!["docker-compose".to_string(), "tailscale".to_string()],
                    packages_path: "packages.toml".to_string(),
                    mode: SystemMode::Pending,
                },
                JobKind::ApplySystem,
                "Apply 2 system packages",
            ),
            (JobSpec::CleanCache, JobKind::CleanCache, "Clean cache"),
            (
                JobSpec::RemoveBrokenSymlinks,
                JobKind::RemoveBrokenSymlinks,
                "Remove broken symlinks",
            ),
            (
                JobSpec::RemoveBrokenPackages,
                JobKind::RemoveBrokenPackages,
                "Remove broken packages",
            ),
            (
                JobSpec::Repo(RepoChange::Add(repo("foo"))),
                JobKind::Repo,
                "Add repository foo",
            ),
            (
                JobSpec::Repo(RepoChange::SetEnabled {
                    name: "foo".to_string(),
                    enabled: true,
                }),
                JobKind::Repo,
                "Enable foo",
            ),
            (
                JobSpec::Repo(RepoChange::SetEnabled {
                    name: "foo".to_string(),
                    enabled: false,
                }),
                JobKind::Repo,
                "Disable foo",
            ),
            (
                JobSpec::Repo(RepoChange::Remove("foo".to_string())),
                JobKind::Repo,
                "Remove repository foo",
            ),
            (
                JobSpec::CreatePackagesConfig,
                JobKind::CreatePackagesConfig,
                "Create packages.toml",
            ),
        ];
        for (spec, kind, title) in cases {
            assert_eq!(spec.kind(), kind, "title {title}");
            assert_eq!(spec.title(), title, "kind {kind:?}");
        }
    }
}
