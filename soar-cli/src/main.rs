use std::{
    env,
    fs::{self, File},
    io::Read,
};

use clap::Parser;
use cli::Args;
use download::download;
use inspect::{inspect_log, InspectType};
use install::install_packages;
use list::{list_installed_packages, list_packages, query_package, search_packages};
use logging::setup_logging;
use remove::remove_packages;
use run::run_package;
use self_actions::process_self_action;
use soar_core::{
    config::{generate_default_config, get_config, set_current_profile},
    metadata::fetch_metadata,
    utils::{cleanup_cache, remove_broken_symlinks, setup_required_paths},
    SoarResult,
};
use tracing::{error, info};
use update::update_packages;
use use_package::use_alternate_package;
use utils::COLOR;

mod cli;
mod download;
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
    let self_bin = args.first().unwrap().clone();
    let self_version = env!("CARGO_PKG_VERSION");

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

    if let Some(ref profile) = args.profile {
        set_current_profile(profile)?;
    }

    if args.no_color {
        let mut color = COLOR.write().unwrap();
        *color = false;
    }

    match args.command {
        cli::Commands::Install {
            packages,
            force,
            yes,
            portable,
            portable_home,
            portable_config,
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
            let config = get_config();
            for repo in &config.repositories {
                let db_file = repo.get_path()?.join("metadata.db");
                if !db_file.exists() {
                    fs::create_dir_all(repo.get_path()?)?;
                    File::create(&db_file)?;
                }
                info!("Fetching metadata from {}", repo.url);
                fetch_metadata(repo.clone()).await?;
            }
            info!("All repositories up to date");
        }
        cli::Commands::Update { packages } => {
            update_packages(packages).await?;
        }
        cli::Commands::ListInstalledPackages { repo_name } => {
            list_installed_packages(repo_name).await?;
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
            )
            .await?;
        }
        cli::Commands::Health => unreachable!(),
        cli::Commands::DefConfig => generate_default_config()?,
        cli::Commands::Env => {
            let config = get_config();
            info!("SOAR_BIN={}", config.get_bin_path()?.display());
            info!("SOAR_DB={}", config.get_db_path()?.display());
            info!("SOAR_CACHE={}", config.get_cache_path()?.display());
            info!("SOAR_PACKAGE={}", config.get_packages_path()?.display());
            info!(
                "SOAR_REPOSITORIES={}",
                config.get_repositories_path()?.display()
            );
        }
        cli::Commands::SelfCmd { action } => {
            process_self_action(&action, self_bin, self_version).await?;
        }
        cli::Commands::Clean {
            cache,
            broken_symlinks,
        } => {
            if cache {
                cleanup_cache()?;
            }
            if broken_symlinks {
                remove_broken_symlinks()?;
            }
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    setup_required_paths().unwrap();

    if let Err(err) = handle_cli().await {
        error!("{}", err);
    };
}
