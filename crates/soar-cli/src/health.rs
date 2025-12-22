use std::{cell::RefCell, env, path::Path, rc::Rc};

use nu_ansi_term::Color::{Blue, Cyan, Green, Red, Yellow};
use soar_config::config::get_config;
use soar_core::{package::remove::PackageRemover, SoarResult};
use soar_db::repository::core::CoreRepository;
use soar_utils::{
    error::FileSystemResult,
    fs::walk_dir,
    path::{desktop_dir, icons_dir},
};
use tabled::{builder::Builder, settings::{Panel, Style, Width, peaker::PriorityMax, themes::BorderCorrection}};
use tracing::info;

use crate::{state::AppState, utils::{icon_or, term_width, Colored, Icons}};

pub async fn display_health() -> SoarResult<()> {
    let path_env = env::var("PATH")?;
    let bin_path = get_config().get_bin_path()?;
    let path_ok = path_env.split(':').any(|p| Path::new(p) == bin_path);

    let broken_pkgs = get_broken_packages().await?;
    let broken_syms = get_broken_symlinks()?;

    let mut builder = Builder::new();

    let path_status = if path_ok {
        format!("{} Configured", Colored(Green, icon_or(Icons::CHECK, "OK")))
    } else {
        format!(
            "{} {} not in PATH",
            Colored(Yellow, icon_or(Icons::WARNING, "!")),
            Colored(Blue, bin_path.display())
        )
    };
    builder.push_record(["PATH".to_string(), path_status]);

    let pkg_status = if broken_pkgs.is_empty() {
        format!("{} None", Colored(Green, icon_or(Icons::CHECK, "OK")))
    } else {
        format!(
            "{} {} found",
            Colored(Red, icon_or(Icons::CROSS, "!")),
            Colored(Red, broken_pkgs.len())
        )
    };
    builder.push_record(["Broken Packages".to_string(), pkg_status]);

    let sym_status = if broken_syms.is_empty() {
        format!("{} None", Colored(Green, icon_or(Icons::CHECK, "OK")))
    } else {
        format!(
            "{} {} found",
            Colored(Red, icon_or(Icons::CROSS, "!")),
            Colored(Red, broken_syms.len())
        )
    };
    builder.push_record(["Broken Symlinks".to_string(), sym_status]);

    let table = builder.build()
        .with(Panel::header("System Health Check"))
        .with(Style::rounded())
        .with(BorderCorrection {})
        .with(Width::wrap(term_width()).priority(PriorityMax::default()))
        .to_string();

    info!("\n{table}");

    if !broken_pkgs.is_empty() {
        info!("\nBroken packages:");
        for pkg in &broken_pkgs {
            info!(
                "  {} {}#{}: {}",
                Icons::ARROW,
                Colored(Blue, &pkg.0),
                Colored(Cyan, &pkg.1),
                Colored(Yellow, &pkg.2)
            );
        }
        info!(
            "Run {} to remove",
            Colored(Green, "soar clean --broken")
        );
    }

    if !broken_syms.is_empty() {
        info!("\nBroken symlinks:");
        for path in &broken_syms {
            info!("  {} {}", Icons::ARROW, Colored(Yellow, path.display()));
        }
        info!(
            "Run {} to remove",
            Colored(Green, "soar clean --broken-symlinks")
        );
    }

    Ok(())
}

async fn get_broken_packages() -> SoarResult<Vec<(String, String, String)>> {
    let state = AppState::new();
    let diesel_db = state.diesel_core_db()?;

    let broken_packages = diesel_db.with_conn(|conn| CoreRepository::list_broken(conn))?;

    Ok(broken_packages
        .into_iter()
        .map(|p| (p.pkg_name, p.pkg_id, p.installed_path))
        .collect())
}

fn get_broken_symlinks() -> SoarResult<Vec<std::path::PathBuf>> {
    let broken_symlinks = Rc::new(RefCell::new(Vec::new()));

    let broken_symlinks_clone = Rc::clone(&broken_symlinks);
    let mut collect_action = |path: &Path| -> FileSystemResult<()> {
        if !path.exists() {
            broken_symlinks_clone.borrow_mut().push(path.to_path_buf());
        }
        Ok(())
    };

    let mut soar_files_action = |path: &Path| -> FileSystemResult<()> {
        if let Some(filename) = path.file_stem().and_then(|s| s.to_str()) {
            if filename.ends_with("-soar") && !path.exists() {
                broken_symlinks_clone.borrow_mut().push(path.to_path_buf());
            }
        }
        Ok(())
    };

    walk_dir(&get_config().get_bin_path()?, &mut collect_action)?;
    walk_dir(desktop_dir(), &mut soar_files_action)?;
    walk_dir(icons_dir(), &mut soar_files_action)?;

    Ok(Rc::try_unwrap(broken_symlinks)
        .unwrap_or_else(|rc| rc.borrow().clone().into())
        .into_inner())
}

pub async fn remove_broken_packages() -> SoarResult<()> {
    let state = AppState::new();
    let diesel_db = state.diesel_core_db()?.clone();

    let broken_packages = diesel_db.with_conn(|conn| CoreRepository::list_broken(conn))?;

    if broken_packages.is_empty() {
        info!("No broken packages found.");
        return Ok(());
    }

    for package in broken_packages {
        let pkg_name = package.pkg_name.clone();
        let pkg_id = package.pkg_id.clone();
        let installed_pkg = package.into();
        let remover = PackageRemover::new(installed_pkg, diesel_db.clone()).await;
        remover.remove().await?;

        info!("Removed {}#{}", pkg_name, pkg_id);
    }

    info!("Removed all broken packages");

    Ok(())
}
