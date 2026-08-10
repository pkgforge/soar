use std::{
    env, fs,
    io::Read,
    process::Command,
    sync::{Arc, Mutex},
};

use apply::apply_packages;
use clap::{CommandFactory, Parser};
use cli::Args;
use download::{create_regex_patterns, download, DownloadContext};
use health::{display_health, remove_broken_packages};
use indicatif::ProgressBar;
use inspect::{inspect_log, InspectType};
use install::install_packages;
use json2db::json_to_db;
use list::{list_installed_packages, list_packages, query_package, search_packages};
use logging::setup_logging;
use progress::{create_download_job, handle_download_progress, spawn_event_handler, ProgressGuard};
use remove::remove_packages;
use run::run_package;
use soar_config::config::{
    self, enable_system_mode, generate_default_config, get_config, set_current_profile, Config,
    CONFIG_PATH,
};
use soar_core::{
    error::{ErrorContext, SoarError},
    utils::{cleanup_cache, remove_broken_symlinks, setup_required_paths},
    SoarResult,
};
use soar_dl::http_client::configure_http_client;
use soar_events::EventSinkHandle;
use soar_operations::SoarContext;
use soar_utils::path::resolve_path;
use tracing::{debug, info, warn};
use update::update_packages;
use ureq::{config::IpFamily, Proxy};
use use_package::use_alternate_package;
use utils::{progress_enabled, COLOR};

mod apply;
mod cli;
mod download;
mod health;
mod inspect;
mod install;
mod json2db;
mod json_output;
mod list;
mod logging;
mod plugin_manifest;
mod progress;
mod remove;
mod repo;
mod run;
mod update;
#[path = "use.rs"]
mod use_package;
mod utils;

#[cfg(feature = "self")]
mod self_actions;

#[cfg(feature = "self")]
use self_actions::process_self_action;

pub fn create_context() -> (SoarContext, Option<ProgressGuard>) {
    create_context_for(false)
}

/// Build the context, putting the event stream where the command leaves room
/// for it: a command answering with one JSON document keeps stdout for the
/// answer, so its events go to stderr.
pub fn create_context_for(answers_with_document: bool) -> (SoarContext, Option<ProgressGuard>) {
    let config = get_config();

    if utils::json_enabled() {
        let events: EventSinkHandle = if answers_with_document {
            Arc::new(soar_events::JsonLinesSink::stderr())
        } else {
            Arc::new(soar_events::JsonLinesSink::stdout())
        };
        return (SoarContext::new(config, events), None);
    }

    if progress_enabled() {
        let (sink, receiver) = soar_events::ChannelSink::new();
        let events: EventSinkHandle = Arc::new(sink);
        let ctx = SoarContext::new(config, events);
        let guard = spawn_event_handler(receiver);
        (ctx, Some(guard))
    } else {
        let events: EventSinkHandle = Arc::new(soar_events::NullSink);
        let ctx = SoarContext::new(config, events);
        (ctx, None)
    }
}

/// Whether `--json` makes this command answer with a single JSON document.
fn answers_with_document(command: &cli::Commands) -> bool {
    matches!(
        command,
        cli::Commands::ListPackages { .. }
            | cli::Commands::ListInstalledPackages { .. }
            | cli::Commands::Search { .. }
            | cli::Commands::Query { .. }
            | cli::Commands::Env
            | cli::Commands::Update {
                check: true,
                ..
            }
            | cli::Commands::Apply {
                dry_run: true,
                ..
            }
            | cli::Commands::Repo {
                action: cli::RepoAction::List,
            }
    )
}

/// Handle system mode - check for root privileges and re-exec with sudo/doas if needed
fn handle_system_mode() -> SoarResult<()> {
    if nix::unistd::geteuid().is_root() {
        enable_system_mode();
        return Ok(());
    }

    let current_exe = env::current_exe()
        .map_err(|e| SoarError::Custom(format!("Failed to get current executable path: {e}")))?;
    let args: Vec<String> = env::args().skip(1).collect();

    let escalation_cmd = if Command::new("doas").arg("true").status().is_ok() {
        "doas"
    } else if Command::new("sudo").arg("true").status().is_ok() {
        "sudo"
    } else {
        return Err(SoarError::Custom(
            "System mode requires root privileges. Neither 'doas' nor 'sudo' found.".into(),
        ));
    };

    debug!(
        "System mode requires root privileges. Re-executing with {}...",
        escalation_cmd
    );

    let status = Command::new(escalation_cmd)
        .arg(&current_exe)
        .args(&args)
        .status()
        .with_context(|| format!("executing {} {:?} {:?}", escalation_cmd, current_exe, args))?;

    std::process::exit(status.code().unwrap_or(1));
}

async fn handle_cli() -> SoarResult<()> {
    let mut args = env::args().collect::<Vec<_>>();

    let mut i = 0;
    while i < args.len() {
        if args[i] == "-" {
            let mut stdin = std::io::stdin();
            let mut buffer = String::new();
            if stdin.read_to_string(&mut buffer).is_ok() {
                let stdin_args = buffer.split_whitespace().collect::<Vec<&str>>();
                args.remove(i);
                args.splice(i..i, stdin_args.into_iter().map(String::from));
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    let args = Args::parse_from(args);

    setup_logging(&args);

    if args.no_color {
        let mut color = COLOR.write().unwrap();
        *color = false;
    }

    if args.no_progress {
        let mut progress = utils::PROGRESS.write().unwrap();
        *progress = false;
    }

    if args.json {
        *utils::JSON.write().unwrap() = true;
        // The progress display writes to the same stream as the events.
        *utils::PROGRESS.write().unwrap() = false;
    }

    if args.system {
        handle_system_mode()?;
    }

    if let Some(ref c) = args.config {
        {
            let mut config_path = CONFIG_PATH.write().unwrap();
            let path = resolve_path(c)?;
            let path = if path.is_absolute() {
                path
            } else {
                env::current_dir()
                    .with_context(|| "retrieving current directory".into())?
                    .join(path)
            };
            *config_path = path;
        }
    }

    let proxy = args.proxy.clone();
    let user_agent = args.user_agent.clone();
    let header = args.header.clone();
    let ip_family = match (args.ipv4, args.ipv6) {
        (true, _) => Some(IpFamily::Ipv4Only),
        (_, true) => Some(IpFamily::Ipv6Only),
        _ => None,
    };

    configure_http_client(|config| {
        if let Some(proxy) = proxy.as_deref() {
            config.proxy = Some(Proxy::new(proxy).unwrap());
        }

        if let Some(ip_family) = ip_family {
            config.ip_family = ip_family;
        }

        if let Some(user_agent) = user_agent {
            config.user_agent = Some(user_agent);
        }

        if let Some(headers) = header {
            let headers = headers
                .into_iter()
                .filter_map(|header| {
                    let (key, value) = header.split_once(':')?;
                    Some((key.parse().unwrap(), value.parse().unwrap()))
                })
                .collect();
            config.headers = Some(headers);
        }
    });

    match args.command {
        cli::Commands::DefConfig {
            repositories,
        } => generate_default_config(repositories.as_slice())?,
        command => {
            config::init()?;

            if let Some(ref profile) = args.profile {
                set_current_profile(profile)?;
            }

            setup_required_paths().unwrap();

            let (ctx, progress_guard) = create_context_for(answers_with_document(&command));
            let mut run_exit_code = None;

            match command {
                cli::Commands::Install {
                    packages,
                    force,
                    yes,
                    portable,
                    portable_home,
                    portable_config,
                    portable_share,
                    portable_cache,
                    no_notes,
                    binary_only,
                    ask,
                    no_verify,
                    name,
                    version,
                    pkg_type,
                    pkg_id,
                    show,
                } => {
                    let portable = portable.map(|p| p.unwrap_or_default());
                    let portable_home = portable_home.map(|p| p.unwrap_or_default());
                    let portable_config = portable_config.map(|p| p.unwrap_or_default());
                    let portable_share = portable_share.map(|p| p.unwrap_or_default());
                    let portable_cache = portable_cache.map(|p| p.unwrap_or_default());

                    install_packages(
                        &ctx,
                        &packages,
                        force,
                        yes,
                        portable,
                        portable_home,
                        portable_config,
                        portable_share,
                        portable_cache,
                        no_notes,
                        binary_only,
                        ask,
                        no_verify,
                        name,
                        version,
                        pkg_type,
                        pkg_id,
                        show,
                    )
                    .await?;
                }
                cli::Commands::Search {
                    query,
                    case_sensitive,
                    limit,
                } => {
                    search_packages(&ctx, query, case_sensitive, limit).await?;
                }
                cli::Commands::Query {
                    query,
                } => {
                    query_package(&ctx, query).await?;
                }
                cli::Commands::Remove {
                    packages,
                    yes,
                    all,
                } => {
                    remove_packages(&ctx, &packages, yes, all).await?;
                }
                cli::Commands::Sync => {
                    ctx.sync().await?;
                }
                cli::Commands::Update {
                    packages,
                    keep,
                    ask,
                    check,
                    no_verify,
                } => {
                    update_packages(&ctx, packages, keep, ask, check, no_verify).await?;
                }
                cli::Commands::ListInstalledPackages {
                    repo_name,
                    count,
                } => {
                    list_installed_packages(&ctx, repo_name, count).await?;
                }
                cli::Commands::ListPackages {
                    repo_name,
                } => {
                    list_packages(&ctx, repo_name).await?;
                }
                cli::Commands::Log {
                    package,
                } => inspect_log(&package, InspectType::BuildLog).await?,
                cli::Commands::Inspect {
                    package,
                } => inspect_log(&package, InspectType::BuildScript).await?,
                cli::Commands::Run {
                    yes,
                    no_verify,
                    command,
                    pkg_id,
                    repo_name,
                } => {
                    let code = run_package(
                        &ctx,
                        command.as_ref(),
                        yes,
                        no_verify,
                        repo_name.as_deref(),
                        pkg_id.as_deref(),
                    )
                    .await?;
                    if code != 0 {
                        run_exit_code = Some(code);
                    }
                }
                cli::Commands::Use {
                    package_name,
                } => {
                    use_alternate_package(&ctx, &package_name).await?;
                }
                cli::Commands::Download {
                    links,
                    yes,
                    output,
                    regexes,
                    globs,
                    match_keywords,
                    exclude_keywords,
                    github,
                    gitlab,
                    ghcr,
                    exact_case,
                    extract,
                    extract_dir,
                    skip_existing,
                    force_overwrite,
                } => {
                    let progress_callback: Arc<dyn Fn(soar_dl::types::Progress) + Send + Sync> = {
                        let pb: Arc<Mutex<Option<ProgressBar>>> = Arc::new(Mutex::new(None));
                        Arc::new(move |state| {
                            let mut guard = pb.lock().unwrap();
                            let bar = guard.get_or_insert_with(|| create_download_job(""));
                            handle_download_progress(state, bar);
                        })
                    };
                    let regexes = create_regex_patterns(regexes)?;
                    let globs = globs.unwrap_or_default();
                    let match_keywords = match_keywords.unwrap_or_default();
                    let exclude_keywords = exclude_keywords.unwrap_or_default();

                    let context = DownloadContext {
                        regexes,
                        globs,
                        match_keywords,
                        exclude_keywords,
                        output: output.clone(),
                        yes,
                        progress_callback: progress_callback.clone(),
                        exact_case,
                        extract,
                        extract_dir,
                        skip_existing,
                        force_overwrite,
                    };

                    download(context, links, github, gitlab, ghcr).await?;
                }
                cli::Commands::Health => display_health(&ctx).await?,
                cli::Commands::Repo {
                    action,
                } => {
                    repo::handle_repo_action(&ctx, action)?;
                }
                cli::Commands::PluginManifest => {
                    let profiles: Vec<String> = get_config().profile.keys().cloned().collect();
                    print!("{}", plugin_manifest::manifest(&profiles));
                }
                cli::Commands::Env => {
                    let config = get_config();
                    let paths = json_output::EnvJson {
                        config: CONFIG_PATH.read()?.display().to_string(),
                        packages_config: soar_config::packages::PACKAGES_CONFIG_PATH
                            .read()?
                            .display()
                            .to_string(),
                        bin: config.get_bin_path()?.display().to_string(),
                        db: config.get_db_path()?.display().to_string(),
                        cache: config.get_cache_path()?.display().to_string(),
                        packages: config.get_packages_path(None)?.display().to_string(),
                        repositories: config.get_repositories_path()?.display().to_string(),
                    };

                    if utils::json_enabled() {
                        json_output::emit(&paths);
                    } else {
                        info!("SOAR_CONFIG={}", paths.config);
                        info!("SOAR_PACKAGES_CONFIG={}", paths.packages_config);
                        info!("SOAR_BIN={}", paths.bin);
                        info!("SOAR_DB={}", paths.db);
                        info!("SOAR_CACHE={}", paths.cache);
                        info!("SOAR_PACKAGES={}", paths.packages);
                        info!("SOAR_REPOSITORIES={}", paths.repositories);
                    }
                }
                #[cfg(feature = "self")]
                cli::Commands::SelfCmd {
                    action,
                } => {
                    process_self_action(&action).await?;
                }
                cli::Commands::Clean {
                    cache,
                    broken_symlinks,
                    broken,
                } => {
                    let unspecified = !cache && !broken_symlinks && !broken;
                    if unspecified || cache {
                        cleanup_cache()?;
                    }
                    if unspecified || broken_symlinks {
                        remove_broken_symlinks()?;
                    }
                    if unspecified || broken {
                        remove_broken_packages(&ctx).await?;
                    }
                }
                cli::Commands::Config {
                    edit,
                } => {
                    let config_path = CONFIG_PATH.read().unwrap();
                    match edit {
                        Some(editor) => {
                            let editor = editor
                                .or_else(|| env::var("EDITOR").ok())
                                .unwrap_or_else(|| "vi".to_string());
                            Command::new(&editor)
                                .arg(&*config_path)
                                .status()
                                .with_context(|| {
                                    format!(
                                        "executing command {} {}",
                                        editor,
                                        config_path.display()
                                    )
                                })?;
                        }
                        None => {
                            let content = match fs::read_to_string(&*config_path) {
                                Ok(v) => v,
                                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                                    warn!("Config file {} not found", config_path.display());
                                    let def_config = Config::default_config::<&str>(&[]);
                                    toml::to_string_pretty(&def_config)?
                                }
                                Err(err) => {
                                    return Err(SoarError::IoError {
                                        action: "reading config".to_string(),
                                        source: err,
                                    });
                                }
                            };
                            info!("{}", content);
                            return Ok(());
                        }
                    };
                }
                cli::Commands::Apply {
                    prune,
                    dry_run,
                    yes,
                    packages_config,
                    no_verify,
                } => {
                    apply_packages(&ctx, prune, dry_run, yes, packages_config, no_verify).await?;
                }
                cli::Commands::DefPackages => {
                    soar_config::packages::generate_default_packages_config()?;
                }
                cli::Commands::Json2Db {
                    input,
                    output,
                    repo,
                } => {
                    json_to_db(&input, &output, repo.as_deref())?;
                }
                cli::Commands::Completions {
                    shell,
                } => {
                    let mut cmd = cli::Args::command();
                    clap_complete::generate(shell, &mut cmd, "soar", &mut std::io::stdout());
                }
                _ => unreachable!(),
            }

            // Drop context first to close the event channel, then join the
            // progress handler thread so remaining events are fully drained.
            drop(ctx);
            if let Some(guard) = progress_guard {
                guard.finish();
            }
            crate::progress::stop();

            if let Some(code) = run_exit_code {
                std::process::exit(code);
            }
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    // Install miette's fancy error handler for beautiful error output
    miette::set_hook(Box::new(|_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .terminal_links(true)
                .unicode(true)
                .context_lines(2)
                .build(),
        )
    }))
    .ok();

    if let Err(err) = handle_cli().await {
        // Use miette's error display for Diagnostic errors
        eprintln!("{:?}", miette::Report::new(err));
        // Anything driving soar reads the exit code to know whether the work
        // happened, so a failure has to say so.
        std::process::exit(1);
    }
}
