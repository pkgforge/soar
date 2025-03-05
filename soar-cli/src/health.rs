use std::path::Path;

use nu_ansi_term::Color::{Blue, Green, Red};
use rusqlite::prepare_and_bind;
use soar_core::{
    config::get_config,
    database::packages::{FilterCondition, PackageQueryBuilder},
    utils::{desktop_dir, icons_dir, process_dir},
    SoarResult,
};
use tracing::info;

use crate::{state::AppState, utils::Colored};

pub async fn display_health() -> SoarResult<()> {
    list_broken_packages().await?;
    println!("");
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
    let mut broken_symlinks = Vec::new();

    let mut collect_action = |path: &Path| -> SoarResult<()> {
        if !path.exists() {
            broken_symlinks.push(path.to_path_buf());
        }
        Ok(())
    };

    process_dir(&get_config().get_bin_path()?, None, &mut collect_action)?;
    process_dir(&desktop_dir(), Some("-soar"), &mut collect_action)?;
    process_dir(&icons_dir(), Some("-soar"), &mut collect_action)?;

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

pub fn remove_broken_packages() -> SoarResult<()> {
    let state = AppState::new();
    let core_db = state.core_db()?;

    let conn = core_db.lock()?;

    let mut stmt = prepare_and_bind!(conn, "DELETE FROM packages WHERE is_installed = false ");
    stmt.raw_execute()?;

    info!("Removed all broken packages");

    Ok(())
}
