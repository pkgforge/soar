use std::path::PathBuf;

use soar_core::{
    SoarResult,
    database::models::{InstalledPackage, Package},
    error::SoarError,
};
use soar_db::repository::{
    core::{CoreRepository, SortDirection},
    metadata::MetadataRepository,
};
use soar_package::{formats::common::setup_portable_dir, integrate_package};
use tracing::debug;

use crate::{
    SoarContext, VariantInfo,
    utils::{has_desktop_integration, mangle_package_symlinks},
};

/// List all installed variants (pkg_ids) for a given package name.
///
/// Installs that never finished are left out, since there is nothing to
/// switch to.
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
            Some(true),
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

/// Switch the active variant to the installed package with row id `id`.
///
/// `id` is the [`InstalledPackage::id`] of a variant from [`list_variants`].
/// This unlinks all other variants and links the selected one, including
/// re-creating symlinks and desktop integration.
pub async fn switch_variant(ctx: &SoarContext, id: u64) -> SoarResult<()> {
    debug!(id, "switching variant");
    let diesel_db = ctx.diesel_core_db()?;

    let row_id =
        i32::try_from(id).map_err(|_| SoarError::Custom(format!("Invalid variant id {id}")))?;
    let selected_package = diesel_db
        .with_conn(|conn| CoreRepository::find_by_id_with_portable(conn, row_id))?
        .filter(|p| p.is_installed)
        .ok_or_else(|| SoarError::Custom("The selected variant is no longer installed".into()))?;

    let pkg_name = &selected_package.pkg_name;
    let pkg_id = selected_package.pkg_id.as_deref();

    // Atomically unlink other variants and link the selected one so the DB
    // is never left in a state where all variants are unlinked.
    diesel_db.transaction(|conn| {
        CoreRepository::unlink_others(
            conn,
            pkg_name,
            &selected_package.repo_name,
            pkg_id,
            selected_package.pkg_family.as_deref(),
            None,
        )?;
        CoreRepository::link_by_row_id(conn, selected_package.id)
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
                Some(&selected_package.pkg_name),
                selected_package.pkg_id.as_deref(),
                selected_package.pkg_family.as_deref(),
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
            config,
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

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::PermissionsExt, path::Path, sync::Arc};

    use soar_config::config::Config;
    use soar_db::models::core::NewPackage;
    use soar_events::NullSink;

    use super::*;

    fn context(root: &Path) -> SoarContext {
        let mut config = Config::default_config::<&str>(&[]);
        config.db_path = Some(root.join("db").display().to_string());
        config.bin_path = Some(root.join("bin").display().to_string());
        fs::create_dir_all(root.join("bin")).unwrap();
        SoarContext::new(config, Arc::new(NullSink))
    }

    fn add(ctx: &SoarContext, root: &Path, repo: &str, installed: bool) -> u64 {
        let dir = root.join("packages").join(repo);
        fs::create_dir_all(dir.join("bin")).unwrap();
        let bin = dir.join("bin/foo");
        fs::write(&bin, "#!/bin/sh\n").unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        let dir = dir.display().to_string();
        let id = ctx
            .diesel_core_db()
            .unwrap()
            .with_conn(|conn| {
                CoreRepository::insert(
                    conn,
                    &NewPackage {
                        repo_name: repo,
                        pkg_id: Some(repo),
                        pkg_name: "foo",
                        version: "1",
                        installed_path: &dir,
                        installed_date: "2026-10-19 00:00:00",
                        profile: "default",
                        is_installed: installed,
                        unlinked: true,
                        ..Default::default()
                    },
                )
            })
            .unwrap();
        id as u64
    }

    #[tokio::test]
    async fn switching_follows_the_row_id_and_skips_unfinished_installs() {
        let root = tempfile::tempdir().unwrap();
        let ctx = context(root.path());
        let first = add(&ctx, root.path(), "first", true);
        let pending = add(&ctx, root.path(), "pending", false);
        let last = add(&ctx, root.path(), "last", true);

        let listed: Vec<u64> = list_variants(&ctx, "foo")
            .unwrap()
            .iter()
            .map(|v| v.package.id)
            .collect();
        assert_eq!(listed, [first, last]);

        assert!(switch_variant(&ctx, pending).await.is_err());
        assert!(switch_variant(&ctx, last + (1 << 32)).await.is_err());

        switch_variant(&ctx, last).await.unwrap();
        let active: Vec<u64> = list_variants(&ctx, "foo")
            .unwrap()
            .iter()
            .filter(|v| v.is_active)
            .map(|v| v.package.id)
            .collect();
        assert_eq!(active, [last]);
        assert_eq!(
            fs::read_link(root.path().join("bin/foo")).unwrap(),
            root.path().join("packages/last/bin/foo")
        );
    }
}
