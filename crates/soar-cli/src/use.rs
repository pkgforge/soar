use std::path::PathBuf;

use indicatif::HumanBytes;
use nu_ansi_term::Color::{Blue, Cyan, Magenta, Red};
use soar_config::config::get_config;
use soar_core::{database::models::Package, SoarResult};
use soar_db::repository::{
    core::{CoreRepository, SortDirection},
    metadata::MetadataRepository,
};
use soar_package::{formats::common::setup_portable_dir, integrate_package};
use tracing::info;

use crate::{
    state::AppState,
    utils::{get_valid_selection, has_desktop_integration, mangle_package_symlinks, Colored},
};

pub async fn use_alternate_package(name: &str) -> SoarResult<()> {
    let state = AppState::new();
    let diesel_db = state.diesel_core_db()?;

    let packages = diesel_db.with_conn(|conn| {
        CoreRepository::list_filtered(
            conn,
            None,
            Some(name),
            None,
            None,
            None,
            None,
            None,
            Some(SortDirection::Asc),
        )
    })?;

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
            Colored(Magenta, HumanBytes(package.size as u64)),
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

    let pkg_name = &selected_package.pkg_name;
    let pkg_id = &selected_package.pkg_id;
    let checksum = selected_package.checksum.as_deref();

    diesel_db.transaction(|conn| {
        CoreRepository::unlink_others_by_checksum(conn, pkg_name, pkg_id, checksum)
    })?;

    let bin_dir = get_config().get_bin_path()?;
    let install_dir = PathBuf::from(&selected_package.installed_path);

    let symlinks = mangle_package_symlinks(
        &install_dir,
        &bin_dir,
        selected_package.provides.as_deref(),
        &selected_package.pkg_name,
        None,
    )
    .await?;

    let actual_bin = symlinks.first().map(|(src, _)| src.as_path());

    let metadata_mgr = state.metadata_manager().await?;
    let pkg: Vec<Package> = metadata_mgr
        .query_repo(&selected_package.repo_name, |conn| {
            MetadataRepository::find_filtered(
                conn,
                Some(name),
                Some(&selected_package.pkg_id),
                None,
                Some(1),
                None,
            )
        })?
        .unwrap_or_default()
        .into_iter()
        .map(|p| {
            let mut package: Package = p.into();
            package.repo_name = selected_package.repo_name.clone();
            package
        })
        .collect();

    let installed_pkg: soar_core::database::models::InstalledPackage = selected_package.into();

    let has_portable = installed_pkg.portable_path.is_some()
        || installed_pkg.portable_home.is_some()
        || installed_pkg.portable_config.is_some()
        || installed_pkg.portable_share.is_some()
        || installed_pkg.portable_cache.is_some();

    if !pkg.is_empty() && pkg.iter().all(has_desktop_integration) {
        integrate_package(
            &install_dir,
            &installed_pkg,
            actual_bin,
            installed_pkg.portable_path.as_deref(),
            installed_pkg.portable_home.as_deref(),
            installed_pkg.portable_config.as_deref(),
            installed_pkg.portable_share.as_deref(),
            installed_pkg.portable_cache.as_deref(),
        )
        .await?;
    } else if has_portable {
        let bin_path = actual_bin
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| install_dir.join(&installed_pkg.pkg_name));
        setup_portable_dir(
            &bin_path,
            &installed_pkg,
            installed_pkg.portable_path.as_deref(),
            installed_pkg.portable_home.as_deref(),
            installed_pkg.portable_config.as_deref(),
            installed_pkg.portable_share.as_deref(),
            installed_pkg.portable_cache.as_deref(),
        )?;
    }

    diesel_db.transaction(|conn| {
        CoreRepository::link_by_checksum(
            conn,
            &installed_pkg.pkg_name,
            &installed_pkg.pkg_id,
            installed_pkg.checksum.as_deref(),
        )
    })?;

    info!(
        "Switched to {}#{}",
        installed_pkg.pkg_name, installed_pkg.pkg_id
    );

    Ok(())
}
