use std::{cell::RefCell, env, path::Path, rc::Rc};

use nu_ansi_term::Color::{Blue, Green, Red};
use soar_core::{
    config::get_config,
    database::packages::{FilterCondition, PackageQueryBuilder},
    package::remove::PackageRemover,
    utils::{desktop_dir, icons_dir, process_dir},
    SoarResult,
};
use tracing::{info, warn};

use crate::{state::AppState, utils::Colored};

pub async fn display_health() -> SoarResult<()> {
    let path_env = env::var("PATH")?;
    let bin_path = get_config().get_bin_path()?;
    if !path_env.split(':').any(|p| Path::new(p) == bin_path) {
        warn!(
            "{} is not in {1}. Please add it to {1} to use installed binaries.\n",
            Colored(Blue, bin_path.display()),
            Colored(Green, "PATH")
        );
    }

    list_broken_packages().await?;
    println!();
    list_broken_symlinks()?;
    Ok(())
}

pub async fn list_broken_packages() -> SoarResult<()> {
    let state = AppState::new();
    let core_db = state.core_db()?;

    let broken_packages = PackageQueryBuilder::new(core_db.clone())
        .where_and("is_installed", FilterCondition::Eq("0".to_string()))
        .load_installed()?
        .items;

    if broken_packages.is_empty() {
        info!("No broken packages found.");
        return Ok(());
    }

    info!("Broken Packages ({}):", broken_packages.len());

    for package in broken_packages {
        info!(
            pkg_name = package.pkg_name,
            pkg_id = package.pkg_id,
            "{}#{}: {}",
            Colored(Blue, &package.pkg_name),
            Colored(Blue, &package.pkg_id),
            Colored(Green, &package.installed_path)
        )
    }

    info!(
        "Broken packages can be uninstalled using command: {}",
        Colored(Green, "soar clean --broken")
    );

    Ok(())
}

pub fn list_broken_symlinks() -> SoarResult<()> {
    let broken_symlinks = Rc::new(RefCell::new(Vec::new()));

    let broken_symlinks_clone = Rc::clone(&broken_symlinks);
    let mut collect_action = |path: &Path| -> SoarResult<()> {
        if !path.exists() {
            broken_symlinks_clone.borrow_mut().push(path.to_path_buf());
        }
        Ok(())
    };

    let mut soar_files_action = |path: &Path| -> SoarResult<()> {
        if let Some(filename) = path.file_stem().and_then(|s| s.to_str()) {
            if filename.ends_with("-soar") && !path.exists() {
                broken_symlinks_clone.borrow_mut().push(path.to_path_buf());
            }
        }
        Ok(())
    };

    process_dir(&get_config().get_bin_path()?, &mut collect_action)?;
    process_dir(desktop_dir(), &mut soar_files_action)?;
    process_dir(icons_dir(), &mut soar_files_action)?;

    let broken_symlinks = Rc::try_unwrap(broken_symlinks)
        .unwrap_or_else(|rc| rc.borrow().clone().into())
        .into_inner();

    if broken_symlinks.is_empty() {
        info!("No broken symlinks found.");
        return Ok(());
    }

    info!("Broken Symlinks ({}):", broken_symlinks.len());

    for path in broken_symlinks {
        info!("{}", Colored(Red, &path.display()));
    }

    info!(
        "Broken symlinks can be removed using command: {}",
        Colored(Green, "soar clean --broken-symlinks")
    );

    Ok(())
}

pub async fn remove_broken_packages() -> SoarResult<()> {
    let state = AppState::new();
    let core_db = state.core_db()?;

    let broken_packages = PackageQueryBuilder::new(core_db.clone())
        .where_and("is_installed", FilterCondition::Eq("0".to_string()))
        .load_installed()?
        .items;

    if broken_packages.is_empty() {
        info!("No broken packages found.");
        return Ok(());
    }

    for package in broken_packages {
        let remover = PackageRemover::new(package.clone(), core_db.clone()).await;
        remover.remove().await?;

        info!("Removed {}#{}", package.pkg_name, package.pkg_id);
    }

    info!("Removed all broken packages");

    Ok(())
}
