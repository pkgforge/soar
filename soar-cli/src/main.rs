use std::{env, fs, io::Read, process::Command};

use clap::Parser;
use cli::Args;
use download::download;
use health::{display_health, remove_broken_packages};
use inspect::{inspect_log, InspectType};
use install::install_packages;
use list::{list_installed_packages, list_packages, query_package, search_packages};
use logging::setup_logging;
use remove::remove_packages;
use run::run_package;
use self_actions::process_self_action;
use soar_core::{
    config::{
        generate_default_config, get_config, set_current_profile, Config, CONFIG, CONFIG_PATH,
    },
    error::{ErrorContext, SoarError},
    utils::{build_path, cleanup_cache, remove_broken_symlinks, setup_required_paths},
    SoarResult,
};
use state::AppState;
use tracing::{error, info, warn};
use update::update_packages;
use use_package::use_alternate_package;
use utils::{Colored, COLOR};

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
mod self_actions;
mod state;
mod update;
#[path = "use.rs"]
mod use_package;
mod utils;

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

    if let Some(ref c) = args.config {
        {
            let mut config_path = CONFIG_PATH.write().unwrap();
            let path = build_path(c)?;
            let path = if path.is_absolute() {
                path
            } else {
                env::current_dir()
                    .with_context(|| "retrieving current directory".into())?
                    .join(path)
            };
            *config_path = path;
        }

        let new_config = Config::new()?;
        let mut config = CONFIG.write().unwrap();
        *config = new_config;
    }

    if let Some(ref profile) = args.profile {
        set_current_profile(profile)?;
    }

    setup_required_paths().unwrap();

    match args.command {
        cli::Commands::Install {
            packages,
            force,
            yes,
            portable,
            portable_home,
            portable_config,
            no_notes,
            binary_only,
            ask,
        } => {
            if portable.is_some() && (portable_home.is_some() || portable_config.is_some()) {
                error!("--portable cannot be used with --portable-home or --portable-config");
                std::process::exit(1);
            }

            let portable = portable.map(|p| p.unwrap_or_default());
            let portable_home = portable_home.map(|p| p.unwrap_or_default());
            let portable_config = portable_config.map(|p| p.unwrap_or_default());

            install_packages(
                &packages,
                force,
                yes,
                portable,
                portable_home,
                portable_config,
                no_notes,
                binary_only,
                ask,
            )
            .await?;
        }
        cli::Commands::Search {
            query,
            case_sensitive,
            limit,
        } => {
            search_packages(query, case_sensitive, limit).await?;
        }
        cli::Commands::Query { query } => {
            query_package(query).await?;
        }
        cli::Commands::Remove { packages } => {
            remove_packages(&packages).await?;
        }
        cli::Commands::Sync => {
            let state = AppState::new();
            state.sync().await?;
            info!("All repositories up to date");
        }
        cli::Commands::Update {
            packages,
            keep,
            ask,
        } => {
            update_packages(packages, keep, ask).await?;
        }
        cli::Commands::ListInstalledPackages { repo_name, count } => {
            list_installed_packages(repo_name, count).await?;
        }
        cli::Commands::ListPackages { repo_name } => {
            list_packages(repo_name).await?;
        }
        cli::Commands::Log { package } => inspect_log(&package, InspectType::BuildLog).await?,
        cli::Commands::Inspect { package } => {
            inspect_log(&package, InspectType::BuildScript).await?
        }
        cli::Commands::Run {
            yes,
            command,
            pkg_id,
            repo_name,
        } => {
            run_package(
                command.as_ref(),
                yes,
                repo_name.as_deref(),
                pkg_id.as_deref(),
            )
            .await?;
        }
        cli::Commands::Use { package_name } => {
            use_alternate_package(&package_name).await?;
        }
        cli::Commands::Download {
            links,
            yes,
            output,
            regex_patterns,
            match_keywords,
            exclude_keywords,
            github,
            gitlab,
            ghcr,
            exact_case,
        } => {
            download(
                links,
                github,
                gitlab,
                ghcr,
                regex_patterns,
                match_keywords,
                exclude_keywords,
                output,
                yes,
                exact_case,
            )
            .await?;
        }
        cli::Commands::Health => display_health().await?,
        cli::Commands::DefConfig { external } => generate_default_config(external)?,
        cli::Commands::Env => {
            let config = get_config();

            info!(
                "[{}]: These values are not configurable via environment \
                variables. They are derived from the config file or defaults.",
                Colored(nu_ansi_term::Color::Yellow, "NOTE")
            );

            info!("SOAR_CONFIG={}", CONFIG_PATH.read()?.display());
            info!("SOAR_BIN={}", config.get_bin_path()?.display());
            info!("SOAR_DB={}", config.get_db_path()?.display());
            info!("SOAR_CACHE={}", config.get_cache_path()?.display());
            info!("SOAR_PACKAGE={}", config.get_packages_path(None)?.display());
            info!(
                "SOAR_REPOSITORIES={}",
                config.get_repositories_path()?.display()
            );
        }
        cli::Commands::SelfCmd { action } => {
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
                remove_broken_packages().await?;
            }
        }
        cli::Commands::Config { edit } => {
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
                            format!("executing command {} {}", editor, config_path.display())
                        })?;
                }
                None => {
                    let content = match fs::read_to_string(&*config_path) {
                        Ok(v) => v,
                        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                            warn!("Config file {} not found", config_path.display());
                            let def_config = Config::default();
                            toml::to_string_pretty(&def_config)?
                        }
                        Err(err) => {
                            return Err(SoarError::IoError {
                                action: "reading config".to_string(),
                                source: err,
                            })
                        }
                    };
                    info!("{}", content);
                    return Ok(());
                }
            };
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    if let Err(err) = handle_cli().await {
        error!("{}", err);
    };
}
