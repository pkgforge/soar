use std::path::PathBuf;

use indicatif::HumanBytes;
use nu_ansi_term::Color::{Blue, Cyan, Magenta, Red};
use rusqlite::prepare_and_bind;
use soar_core::{
    config::get_config,
    database::{
        models::{InstalledPackage, Package},
        packages::{FilterCondition, PackageQueryBuilder, SortDirection},
    },
    package::formats::common::integrate_package,
    SoarResult,
};
use tracing::info;

use crate::{
    state::AppState,
    utils::{get_valid_selection, has_desktop_integration, mangle_package_symlinks, Colored},
};

pub async fn use_alternate_package(name: &str) -> SoarResult<()> {
    let state = AppState::new();
    let db = state.core_db()?;

    let packages = PackageQueryBuilder::new(db.clone())
        .where_and("pkg_name", FilterCondition::Eq(name.to_string()))
        .sort_by("id", SortDirection::Asc)
        .load_installed()?
        .items;

    if packages.is_empty() {
        info!("Package is not installed");
        return Ok(());
    }

    for (idx, package) in packages.iter().enumerate() {
        info!(
            active = !package.unlinked,
            pkg_name = package.pkg_name,
            pkg_id = package.pkg_id,
            repo_name = package.repo_name,
            pkg_type = package.pkg_type,
            version = package.version,
            size = package.size,
            "[{}] {}#{}:{} ({}-{}) ({}){}",
            idx + 1,
            Colored(Blue, &package.pkg_name),
            Colored(Cyan, &package.pkg_id),
            Colored(Cyan, &package.repo_name),
            package
                .pkg_type
                .as_ref()
                .map(|pkg_type| format!(":{}", Colored(Magenta, &pkg_type)))
                .unwrap_or_default(),
            Colored(Magenta, &package.version),
            Colored(Magenta, HumanBytes(package.size)),
            package
                .unlinked
                .then(String::new)
                .unwrap_or_else(|| format!(" {}", Colored(Red, "*")))
        );
    }

    if packages.len() == 1 {
        return Ok(());
    }

    let selection = get_valid_selection(packages.len())?;
    let selected_package = packages.into_iter().nth(selection).unwrap();

    let InstalledPackage {
        pkg_name,
        pkg_id,
        checksum,
        ..
    } = &selected_package;

    let mut conn = db.lock().unwrap();

    let tx = conn.transaction()?;

    {
        let mut stmt = prepare_and_bind!(
            tx,
            "UPDATE packages
            SET
                unlinked = true
            WHERE
                pkg_name = $pkg_name
                AND pkg_id != $pkg_id
                AND checksum != $checksum
            "
        );
        stmt.raw_execute()?;
    }

    let bin_dir = get_config().get_bin_path()?;
    let install_dir = PathBuf::from(&selected_package.installed_path);

    let _ = mangle_package_symlinks(&install_dir, &bin_dir, selected_package.provides.as_deref())
        .await?;

    // TODO: handle portable_dirs
    let repo_db = state.repo_db().await?;
    let pkg: Vec<Package> = PackageQueryBuilder::new(repo_db.clone())
        .where_and(
            "repo_name",
            FilterCondition::Eq(selected_package.repo_name.clone()),
        )
        .where_and("pkg_name", FilterCondition::Eq(name.to_string()))
        .where_and(
            "pkg_id",
            FilterCondition::Eq(selected_package.pkg_id.clone()),
        )
        .limit(1)
        .load()?
        .items;

    if pkg.iter().all(has_desktop_integration) {
        integrate_package(&install_dir, &selected_package, None, None, None, None).await?;
    }

    {
        let mut stmt = prepare_and_bind!(
            tx,
            "UPDATE packages
            SET
                unlinked = false
            WHERE
                pkg_name = $pkg_name
                AND pkg_id == $pkg_id
                AND checksum == $checksum"
        );
        stmt.raw_execute()?;
    }

    tx.commit()?;

    info!("Switched to {}#{}", pkg_name, pkg_id);

    Ok(())
}
