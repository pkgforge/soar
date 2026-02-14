use std::{env, fs, io::Read, process::Command, sync::Arc};

use apply::apply_packages;
use clap::Parser;
use cli::Args;
use download::{create_regex_patterns, download, DownloadContext};
use health::{display_health, remove_broken_packages};
use inspect::{inspect_log, InspectType};
use install::install_packages;
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
use ureq::Proxy;
use use_package::use_alternate_package;
use utils::{progress_enabled, COLOR};

mod apply;
mod cli;
mod download;
mod health;
mod inspect;
mod install;
mod list;
mod logging;
mod progress;
mod remove;
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
    let config = get_config();

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

    configure_http_client(|config| {
        if let Some(proxy) = proxy.as_deref() {
            config.proxy = Some(Proxy::new(proxy).unwrap());
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

            let (ctx, progress_guard) = create_context();

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
                    no_verify,
                } => {
                    update_packages(&ctx, packages, keep, ask, no_verify).await?;
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
                    command,
                    pkg_id,
                    repo_name,
                } => {
                    run_package(
                        &ctx,
                        command.as_ref(),
                        yes,
                        repo_name.as_deref(),
                        pkg_id.as_deref(),
                    )
                    .await?;
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
                    let pb = create_download_job("");
                    let progress_callback: Arc<dyn Fn(soar_dl::types::Progress) + Send + Sync> = {
                        let pb = pb.clone();
                        Arc::new(move |state| handle_download_progress(state, &pb))
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
                cli::Commands::Env => {
                    let config = get_config();

                    info!("SOAR_CONFIG={}", CONFIG_PATH.read()?.display());
                    info!("SOAR_BIN={}", config.get_bin_path()?.display());
                    info!("SOAR_DB={}", config.get_db_path()?.display());
                    info!("SOAR_CACHE={}", config.get_cache_path()?.display());
                    info!(
                        "SOAR_PACKAGES={}",
                        config.get_packages_path(None)?.display()
                    );
                    info!(
                        "SOAR_REPOSITORIES={}",
                        config.get_repositories_path()?.display()
                    );
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
                _ => unreachable!(),
            }

            // Drop context first to close the event channel, then join the
            // progress handler thread so remaining events are fully drained.
            drop(ctx);
            if let Some(guard) = progress_guard {
                guard.finish();
            }
            crate::progress::stop();
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
    }
}
