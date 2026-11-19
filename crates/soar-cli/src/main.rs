use std::{
    env,
    ffi::OsString,
    fs,
    io::{IsTerminal as _, Read},
    path::Path,
    process::Command,
    sync::{Arc, Mutex},
};

use apply::apply_packages;
use clap::{CommandFactory, Parser};
use cli::Args;
use download::{DownloadContext, create_regex_patterns, download};
use health::{display_health, remove_broken_packages, remove_broken_symlinks};
use indicatif::ProgressBar;
use install::install_packages;
use json2db::json_to_db;
use list::{list_installed_packages, list_packages, query_package, search_packages};
use logging::setup_logging;
use progress::{ProgressGuard, create_download_job, handle_download_progress, spawn_event_handler};
use remove::remove_packages;
use run::run_package;
use soar_config::config::{
    self, CONFIG_PATH, Config, enable_system_mode, generate_default_config, get_config,
    path_env_var, set_current_profile,
};
use soar_core::{
    SoarResult,
    error::{ErrorContext, SoarError},
    utils::{cleanup_cache, setup_required_paths},
};
use soar_dl::http_client::configure_http_client;
use soar_events::EventSinkHandle;
use soar_operations::{
    SoarContext,
    privilege::{ESCALATION_DIRS, escalated_command, find_escalation_tool},
};
use soar_utils::path::resolve_path;
use tracing::{debug, info, warn};
use update::update_packages;
use ureq::{
    Proxy,
    config::IpFamily,
    http::{HeaderMap, HeaderName, HeaderValue},
};
use use_package::use_alternate_package;
use utils::{COLOR, progress_enabled};

mod apply;
mod cli;
mod download;
mod health;
mod install;
mod interrupt;
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
mod url_handler;
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
        let events: EventSinkHandle = Arc::new(progress::OutputOnlySink);
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

/// Whether this command writes to the system installation, and so needs root in
/// system mode.
///
/// Everything else only reads, and the system tree is readable without it, so
/// those run as the invoking user rather than asking for a password to answer a
/// query.
fn requires_root(command: &cli::Commands) -> bool {
    #[cfg(feature = "self")]
    if matches!(command, cli::Commands::SelfCmd { .. }) {
        return true;
    }

    #[cfg(feature = "tui")]
    if matches!(command, cli::Commands::Tui) {
        return true;
    }

    matches!(
        command,
        cli::Commands::Install { .. }
            | cli::Commands::Remove { .. }
            | cli::Commands::Sync
            | cli::Commands::Run { .. }
            | cli::Commands::Use { .. }
            | cli::Commands::Clean { .. }
            | cli::Commands::Url { .. }
            | cli::Commands::DefConfig { .. }
            | cli::Commands::DefPackages
            | cli::Commands::Update {
                check: false,
                ..
            }
            | cli::Commands::Apply {
                dry_run: false,
                ..
            }
            | cli::Commands::Config {
                edit: Some(_)
            }
            | cli::Commands::Repo {
                action: cli::RepoAction::Add { .. }
                    | cli::RepoAction::Update { .. }
                    | cli::RepoAction::Remove { .. },
            }
    )
}

/// Handle system mode - check for root privileges and re-exec with sudo/doas if needed
fn handle_system_mode(command: &cli::Commands) -> SoarResult<()> {
    if nix::unistd::geteuid().is_root() {
        enable_system_mode();
        return Ok(());
    }

    // A read-only command reads exactly what root would, so escalating buys
    // nothing: point it at the system tree and let it run as the user.
    if !requires_root(command) {
        enable_system_mode();
        return Ok(());
    }

    let current_exe = env::current_exe()
        .map_err(|e| SoarError::Custom(format!("Failed to get current executable path: {e}")))?;
    let args: Vec<String> = env::args().skip(1).collect();

    let tool = find_escalation_tool().ok_or_else(|| {
        SoarError::Custom(format!(
            "System mode requires root privileges. Neither 'doas' nor 'sudo' found in {}.",
            ESCALATION_DIRS.join(", ")
        ))
    })?;

    debug!(
        "System mode requires root privileges. Re-executing with {}...",
        tool.path.display()
    );

    let os_args: Vec<OsString> = args.iter().map(OsString::from).collect();
    let mut command = escalated_command(&tool, &[], &current_exe, &os_args);

    let status = command.status().with_context(|| {
        format!(
            "executing {} {:?} {:?}",
            tool.path.display(),
            current_exe,
            args
        )
    })?;

    std::process::exit(status.code().unwrap_or(1));
}

/// Rebuilds the global options a privileged soar child needs, so it reads
/// the same configuration and connections this process was given. The
/// configuration path is the resolved one, since the child starts in the
/// working directory the interface runs in, not the one the user typed in.
#[cfg(feature = "tui")]
fn global_args(args: &Args, config_path: &Path) -> Vec<OsString> {
    let mut forward: Vec<OsString> = Vec::new();
    if args.config.is_some() {
        forward.push("--config".into());
        forward.push(config_path.as_os_str().to_os_string());
    }
    if let Some(profile) = &args.profile {
        forward.push("--profile".into());
        forward.push(profile.into());
    }
    if let Some(proxy) = &args.proxy {
        forward.push("--proxy".into());
        forward.push(proxy.into());
    }
    if let Some(headers) = &args.header {
        for header in headers {
            forward.push("--header".into());
            forward.push(header.into());
        }
    }
    if let Some(user_agent) = &args.user_agent {
        forward.push("--user-agent".into());
        forward.push(user_agent.into());
    }
    if args.ipv4 {
        forward.push("-4".into());
    }
    if args.ipv6 {
        forward.push("-6".into());
    }
    if args.no_color {
        forward.push("--no-color".into());
    }
    forward
}

/// Replaces a `-` among `values` with the words `read` supplies.
///
/// This runs on lists clap has already parsed, so whatever stdin holds can
/// only ever be names in that list: a word read there never becomes a flag.
fn splice_stdin(
    values: &mut Vec<String>,
    read: impl FnOnce() -> SoarResult<String>,
) -> SoarResult<()> {
    let Some(pos) = values.iter().position(|value| value == "-") else {
        return Ok(());
    };
    let input = read()?;
    values.retain(|value| value != "-");
    values.splice(pos..pos, input.split_whitespace().map(String::from));
    Ok(())
}

fn read_stdin_words() -> SoarResult<String> {
    let mut stdin = std::io::stdin();
    if stdin.is_terminal() {
        return Err(SoarError::Custom(
            "`-` reads names from stdin, but stdin is a terminal".into(),
        ));
    }
    let mut buffer = String::new();
    stdin
        .read_to_string(&mut buffer)
        .with_context(|| "reading names from stdin".to_string())?;
    Ok(buffer)
}

/// Expands `-` in the package and link lists of `command` from stdin.
fn expand_stdin(command: &mut cli::Commands) -> SoarResult<()> {
    match command {
        cli::Commands::Install {
            packages, ..
        }
        | cli::Commands::Remove {
            packages, ..
        }
        | cli::Commands::Update {
            packages: Some(packages),
            ..
        } => splice_stdin(packages, read_stdin_words),
        cli::Commands::Download {
            links, ..
        } => splice_stdin(links, read_stdin_words),
        _ => Ok(()),
    }
}

/// Parses `--header` values, each `Name: value`.
fn parse_headers(headers: &[String]) -> SoarResult<HeaderMap> {
    headers
        .iter()
        .map(|header| {
            let invalid =
                |reason: &str| SoarError::Custom(format!("invalid --header {header:?}: {reason}"));
            let (name, value) = header
                .split_once(':')
                .ok_or_else(|| invalid("expected `Name: value`"))?;
            let name = HeaderName::from_bytes(name.trim().as_bytes())
                .map_err(|err| invalid(&err.to_string()))?;
            let value =
                HeaderValue::from_str(value.trim()).map_err(|err| invalid(&err.to_string()))?;
            Ok((name, value))
        })
        .collect()
}

/// Opens `path` in `editor`, which, like `$EDITOR`, may carry arguments of its
/// own such as `code --wait`. It is run through the shell, with the path
/// passed as an argument rather than spliced into the command line.
fn open_in_editor(editor: &str, path: &Path) -> SoarResult<()> {
    let status = Command::new("sh")
        .arg("-c")
        .arg(format!("{editor} \"$1\""))
        .arg("sh")
        .arg(path)
        .status()
        .with_context(|| format!("executing command {} {}", editor, path.display()))?;
    if !status.success() {
        return Err(SoarError::Custom(format!(
            "{editor} exited with {status} while editing {}",
            path.display()
        )));
    }
    Ok(())
}

async fn handle_cli() -> SoarResult<()> {
    let mut args = Args::parse();
    expand_stdin(&mut args.command)?;

    #[cfg(feature = "tui")]
    if !matches!(args.command, cli::Commands::Tui) {
        setup_logging(&args);
    }

    #[cfg(not(feature = "tui"))]
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
        handle_system_mode(&args.command)?;
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

    let proxy = args
        .proxy
        .as_deref()
        .map(|proxy| {
            Proxy::new(proxy)
                .map_err(|err| SoarError::Custom(format!("invalid --proxy {proxy:?}: {err}")))
        })
        .transpose()?;
    let headers = args.header.as_deref().map(parse_headers).transpose()?;
    let user_agent = args.user_agent.clone();
    let ip_family = match (args.ipv4, args.ipv6) {
        (true, _) => Some(IpFamily::Ipv4Only),
        (_, true) => Some(IpFamily::Ipv6Only),
        _ => None,
    };

    configure_http_client(|config| {
        if proxy.is_some() {
            config.proxy = proxy;
        }

        if let Some(ip_family) = ip_family {
            config.ip_family = ip_family;
        }

        if let Some(user_agent) = user_agent {
            config.user_agent = Some(user_agent);
        }

        if headers.is_some() {
            config.headers = headers;
        }
    });

    #[cfg(feature = "tui")]
    // The match below moves the command out of `args`, so the global
    // options the interface forwards to a privileged child are rebuilt
    // here, after the configuration path has been resolved.
    let forward_args = if matches!(args.command, cli::Commands::Tui) {
        let config_path = CONFIG_PATH
            .read()
            .map(|path| path.to_path_buf())
            .unwrap_or_default();
        global_args(&args, &config_path)
    } else {
        Vec::new()
    };

    match args.command {
        cli::Commands::DefConfig {
            repositories,
        } => generate_default_config(repositories.as_slice())?,
        command => {
            config::init()?;
            soar_dl::forge::set_instance_token_vars(
                get_config().forge_tokens.clone().unwrap_or_default(),
            );

            if let Some(ref profile) = args.profile {
                set_current_profile(profile)?;
            }

            setup_required_paths()?;

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
                    integrate,
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
                        integrate,
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
                    codeberg,
                    gitea,
                    ghcr,
                    exact_case,
                    extract,
                    extract_dir,
                    skip_existing,
                    force_overwrite,
                    connections,
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

                    let connections = connections
                        .or(get_config().download_concurrency)
                        .unwrap_or(8);
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
                        connections,
                    };

                    download(context, links, github, gitlab, codeberg, gitea, ghcr).await?;
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
                cli::Commands::Url {
                    url,
                    register,
                } => {
                    url_handler::handle(&ctx, url, register).await?;
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
                        json_output::emit(&paths)?;
                    } else {
                        let name = |suffix| path_env_var(suffix, config.is_system());
                        info!("{}={}", name("CONFIG"), paths.config);
                        info!("{}={}", name("PACKAGES_CONFIG"), paths.packages_config);
                        info!("{}={}", name("BIN"), paths.bin);
                        info!("{}={}", name("DB"), paths.db);
                        info!("{}={}", name("CACHE"), paths.cache);
                        info!("{}={}", name("PACKAGES"), paths.packages);
                        info!("{}={}", name("REPOSITORIES"), paths.repositories);
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
                        remove_broken_symlinks(&ctx)?;
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
                            open_in_editor(&editor, &config_path)?;
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
                    only_system,
                } => {
                    apply_packages(
                        &ctx,
                        prune,
                        dry_run,
                        yes,
                        packages_config,
                        no_verify,
                        only_system,
                    )
                    .await?;
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
                #[cfg(feature = "tui")]
                cli::Commands::Tui => {
                    if utils::json_enabled() {
                        return Err(SoarError::Custom("soar tui does not support --json".into()));
                    }
                    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
                        return Err(SoarError::Custom(
                            "soar tui needs an interactive terminal".into(),
                        ));
                    }
                    let no_color = !*COLOR.read().unwrap()
                        || env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
                    soar_tui::run(soar_tui::TuiOptions {
                        version: env!("CARGO_PKG_VERSION").to_string(),
                        no_color,
                        icons: get_config().display().icons(),
                        verbosity: args.verbose,
                        quiet: args.quiet,
                        forward_args,
                    })
                    .await?;
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
        // An aborted operation has already said what finished and what did
        // not, so a full report would only repeat it.
        if matches!(err, SoarError::Cancelled) {
            if !utils::json_enabled() {
                eprintln!("Aborted");
            }
            std::process::exit(130);
        }
        // Use miette's error display for Diagnostic errors
        eprintln!("{:?}", miette::Report::new(err));
        // Anything driving soar reads the exit code to know whether the work
        // happened, so a failure has to say so.
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(input: &'static str) -> impl FnOnce() -> SoarResult<String> {
        move || Ok(input.to_string())
    }

    #[test]
    fn stdin_words_become_list_entries() {
        let mut values = vec!["a".to_string(), "-".to_string(), "z".to_string()];
        splice_stdin(&mut values, words("b --system\n c")).unwrap();
        assert_eq!(values, ["a", "b", "--system", "c", "z"]);
    }

    #[test]
    fn stdin_is_read_only_for_a_dash() {
        let mut values = vec!["a".to_string()];
        splice_stdin(&mut values, || panic!("stdin read without a `-`")).unwrap();
        assert_eq!(values, ["a"]);
    }

    #[test]
    fn a_dash_is_not_expanded_outside_the_lists() {
        let mut args = Args::parse_from(["soar", "download", "https://example.com/f", "-o", "-"]);
        expand_stdin(&mut args.command).unwrap();
        match args.command {
            cli::Commands::Download {
                links,
                output,
                ..
            } => {
                assert_eq!(links, ["https://example.com/f"]);
                assert_eq!(output.as_deref(), Some("-"));
            }
            _ => panic!("expected download"),
        }
    }

    #[test]
    fn headers_must_be_name_colon_value() {
        let headers = parse_headers(&["X-Token: abc".to_string()]).unwrap();
        assert_eq!(headers.get("x-token").unwrap(), "abc");

        for bad in ["no-colon", "bad name: v", "X-Ok: bad\nvalue"] {
            assert!(parse_headers(&[bad.to_string()]).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn an_editor_may_carry_arguments() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config with space.toml");
        fs::write(&path, "a").unwrap();

        open_in_editor("sed -i s/a/b/", &path).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "b");

        assert!(open_in_editor("false", &path).is_err());
    }

    #[cfg(feature = "tui")]
    #[test]
    fn tui_requires_root_in_system_mode() {
        assert!(requires_root(&cli::Commands::Tui));
    }

    #[cfg(feature = "tui")]
    #[test]
    fn the_tui_child_gets_the_resolved_global_options() {
        let args = Args::try_parse_from(["soar", "-c", "x.toml", "-p", "work", "tui"]).unwrap();
        let forward: Vec<String> = global_args(&args, Path::new("/home/user/.config/soar/x.toml"))
            .into_iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        let config_at = forward.iter().position(|arg| arg == "--config").unwrap();
        let path = &forward[config_at + 1];
        assert!(Path::new(path).is_absolute(), "config path is {path}");
        assert!(path.ends_with("x.toml"), "config path is {path}");
        let profile_at = forward.iter().position(|arg| arg == "--profile").unwrap();
        assert_eq!(forward[profile_at + 1], "work");
        assert!(!forward.iter().any(|arg| arg == "--json"));
        assert!(!forward.iter().any(|arg| arg == "-S"));
    }

    #[cfg(feature = "tui")]
    #[test]
    fn global_options_not_given_stay_out_of_the_forwarded_arguments() {
        let args = Args::try_parse_from(["soar", "-4", "tui"]).unwrap();
        let forward = global_args(&args, Path::new("/unused/config.toml"));
        assert_eq!(forward, [OsString::from("-4")]);
    }
}
