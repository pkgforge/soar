use std::{fs, os, path::PathBuf};

use indicatif::HumanBytes;
use nu_ansi_term::Color::{Blue, Cyan, Magenta, Red};
use rusqlite::prepare_and_bind;
use soar_core::{
    config::get_config,
    database::{
        models::{InstalledPackage, Package, PackageExt},
        packages::{FilterCondition, PackageQueryBuilder, ProvideStrategy, SortDirection},
    },
    error::{ErrorContext, SoarError},
    package::formats::common::integrate_package,
    SoarResult,
};
use tracing::info;

use crate::{
    state::AppState,
    utils::{get_valid_selection, has_no_desktop_integration, Colored},
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
            active = package.unlinked == false,
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
                .then(|| String::new())
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
    let def_bin_path = bin_dir.join(&pkg_name);
    let install_dir = PathBuf::from(&selected_package.installed_path);
    let real_bin = install_dir.join(&pkg_name);

    if selected_package.should_create_original_symlink() {
        if def_bin_path.is_symlink() || def_bin_path.is_file() {
            if let Err(err) = fs::remove_file(&def_bin_path) {
                return Err(SoarError::Custom(format!(
                    "Failed to remove existing symlink: {}",
                    err
                )));
            }
        }
        os::unix::fs::symlink(&real_bin, &def_bin_path).with_context(|| {
            format!(
                "creating symlink {} -> {}",
                real_bin.display(),
                def_bin_path.display()
            )
        })?;
    }

    if let Some(provides) = &selected_package.provides {
        for provide in provides {
            if let Some(ref target) = provide.target {
                let real_path = install_dir.join(provide.name.clone());
                let is_symlink = match provide.strategy {
                    Some(ProvideStrategy::KeepTargetOnly) | Some(ProvideStrategy::KeepBoth) => true,
                    _ => false,
                };
                if is_symlink {
                    let target_name = bin_dir.join(&target);
                    if target_name.is_symlink() || target_name.is_file() {
                        std::fs::remove_file(&target_name).with_context(|| {
                            format!("removing provide from {}", target_name.display())
                        })?;
                    }
                    os::unix::fs::symlink(&real_path, &target_name).with_context(|| {
                        format!(
                            "creating symlink {} -> {}",
                            real_path.display(),
                            target_name.display()
                        )
                    })?;
                }
            }
        }
    }

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

    let (icon_path, desktop_path) = if pkg
        .iter()
        .any(|p| has_no_desktop_integration(&p.repo_name, p.notes.as_deref()))
    {
        (None, None)
    } else {
        integrate_package(&install_dir, &selected_package, None, None, None).await?
    };

    {
        let icon_path = icon_path.map(|path| path.to_string_lossy().into_owned());
        let desktop_path = desktop_path.map(|path| path.to_string_lossy().into_owned());
        let bin_path = def_bin_path.to_string_lossy();
        let mut stmt = prepare_and_bind!(
            tx,
            "UPDATE packages
            SET
                bin_path = $bin_path,
                icon_path = $icon_path,
                desktop_path = $desktop_path,
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
