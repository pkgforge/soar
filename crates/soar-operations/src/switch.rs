use std::path::PathBuf;

use soar_core::{
    database::models::{InstalledPackage, Package},
    error::SoarError,
    SoarResult,
};
use soar_db::repository::{
    core::{CoreRepository, SortDirection},
    metadata::MetadataRepository,
};
use soar_package::{formats::common::setup_portable_dir, integrate_package};
use tracing::debug;

use crate::{
    utils::{has_desktop_integration, mangle_package_symlinks},
    SoarContext, VariantInfo,
};

/// List all installed variants (pkg_ids) for a given package name.
pub fn list_variants(ctx: &SoarContext, name: &str) -> SoarResult<Vec<VariantInfo>> {
    debug!(name = name, "listing variants");
    let diesel_db = ctx.diesel_core_db()?;

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

    Ok(packages
        .into_iter()
        .map(|p| {
            let is_active = !p.unlinked;
            let package = p.into();
            VariantInfo {
                package,
                is_active,
            }
        })
        .collect())
}

/// Switch the active variant for a package name.
///
/// `selected_index` is the 0-based index into the list returned by [`list_variants`].
/// This unlinks all other variants and links the selected one, including
/// re-creating symlinks and desktop integration.
pub async fn switch_variant(
    ctx: &SoarContext,
    name: &str,
    selected_index: usize,
) -> SoarResult<()> {
    debug!(name = name, index = selected_index, "switching variant");
    let diesel_db = ctx.diesel_core_db()?;

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

    let selected_package = packages
        .into_iter()
        .nth(selected_index)
        .ok_or_else(|| SoarError::Custom("Invalid variant index".into()))?;

    let pkg_name = &selected_package.pkg_name;
    let pkg_id = &selected_package.pkg_id;
    let checksum = selected_package.checksum.as_deref();

    // Atomically unlink other variants and link the selected one so the DB
    // is never left in a state where all variants are unlinked.
    diesel_db.transaction(|conn| {
        CoreRepository::unlink_others_by_checksum(conn, pkg_name, pkg_id, checksum)?;
        CoreRepository::link_by_checksum(conn, pkg_name, pkg_id, checksum)
    })?;

    let config = ctx.config();
    let bin_dir = config.get_bin_path()?;
    let install_dir = PathBuf::from(&selected_package.installed_path);

    // Re-create symlinks
    let symlinks = mangle_package_symlinks(
        &install_dir,
        &bin_dir,
        selected_package.provides.as_deref(),
        &selected_package.pkg_name,
        &selected_package.version,
        None,
        None,
    )
    .await?;

    let actual_bin = symlinks.first().map(|(src, _)| src.as_path());

    // Check if desktop integration is needed
    let metadata_mgr = ctx.metadata_manager().await?;
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

    let installed_pkg: InstalledPackage = selected_package.into();

    let has_portable = installed_pkg.portable_path.is_some()
        || installed_pkg.portable_home.is_some()
        || installed_pkg.portable_config.is_some()
        || installed_pkg.portable_share.is_some()
        || installed_pkg.portable_cache.is_some();

    if !pkg.is_empty() && pkg.iter().all(|p| has_desktop_integration(p, config)) {
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

    Ok(())
}
