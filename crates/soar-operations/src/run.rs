use std::{fs, path::Path, process::Command, sync::Arc};

use soar_core::{
    database::models::Package,
    error::{ErrorContext, SoarError},
    package::query::PackageQuery,
    utils::get_extract_dir,
    SoarResult,
};
use soar_db::repository::metadata::MetadataRepository;
use soar_dl::{download::Download, oci::OciDownload, types::OverwriteMode};
use soar_events::SoarEvent;
use soar_utils::hash::calculate_checksum;
use tracing::debug;

use crate::{
    progress::{create_progress_bridge, next_op_id},
    AmbiguousPackage, PrepareRunResult, RunResult, SoarContext,
};

/// Resolve a package and download it to the cache if needed.
///
/// Returns [`PrepareRunResult::Ready`] with the path to the cached binary,
/// or [`PrepareRunResult::Ambiguous`] if multiple candidates match.
pub async fn prepare_run(
    ctx: &SoarContext,
    package_name: &str,
    repo_name: Option<&str>,
    pkg_id: Option<&str>,
) -> SoarResult<PrepareRunResult> {
    debug!(package_name = package_name, "preparing run");
    let config = ctx.config();
    let cache_bin = config.get_cache_path()?.join("bin");

    let query = PackageQuery::try_from(package_name)?;
    let package_name = query.name.as_deref().unwrap_or(package_name);
    let repo_name = query.repo_name.as_deref().or(repo_name);
    let pkg_id = query.pkg_id.as_deref().or(pkg_id);
    let version = query.version.as_deref();

    let output_path = cache_bin.join(package_name);
    if output_path.exists() {
        return Ok(PrepareRunResult::Ready(output_path));
    }

    let metadata_mgr = ctx.metadata_manager().await?;

    let packages: Vec<Package> = if let Some(repo_name) = repo_name {
        metadata_mgr
            .query_repo(repo_name, |conn| {
                MetadataRepository::find_filtered(
                    conn,
                    Some(package_name),
                    pkg_id,
                    None,
                    None,
                    None,
                )
            })?
            .unwrap_or_default()
            .into_iter()
            .map(|p| {
                let mut pkg: Package = p.into();
                pkg.repo_name = repo_name.to_string();
                pkg
            })
            .collect()
    } else {
        metadata_mgr.query_all_flat(|repo_name, conn| {
            let pkgs = MetadataRepository::find_filtered(
                conn,
                Some(package_name),
                pkg_id,
                None,
                None,
                None,
            )?;
            Ok(pkgs
                .into_iter()
                .map(|p| {
                    let mut pkg: Package = p.into();
                    pkg.repo_name = repo_name.to_string();
                    pkg
                })
                .collect())
        })?
    };

    let packages: Vec<Package> = if let Some(version) = version {
        packages
            .into_iter()
            .filter(|p| p.has_version(version))
            .collect()
    } else {
        packages
    };

    match packages.len() {
        0 => return Err(SoarError::PackageNotFound(package_name.to_string())),
        1 => {}
        _ => {
            return Ok(PrepareRunResult::Ambiguous(AmbiguousPackage {
                query: package_name.to_string(),
                candidates: packages,
            }));
        }
    }

    let package = packages.into_iter().next().unwrap().resolve(version);

    fs::create_dir_all(&cache_bin)
        .with_context(|| format!("creating directory {}", cache_bin.display()))?;

    let op_id = next_op_id();
    let progress_callback = create_progress_bridge(
        ctx.events().clone(),
        op_id,
        package.pkg_name.clone(),
        package.pkg_id.clone(),
    );

    download_to_cache(ctx, &package, &output_path, &cache_bin, progress_callback)?;

    // Checksum verification
    let checksum = calculate_checksum(&output_path)?;
    if let Some(ref bsum) = package.bsum {
        if checksum != *bsum {
            ctx.events().emit(SoarEvent::Log {
                level: soar_events::LogLevel::Warning,
                message: format!(
                    "Checksum mismatch for {}: expected {}, got {}",
                    package.pkg_name, bsum, checksum
                ),
            });
            return Err(SoarError::InvalidChecksum);
        }
    }

    Ok(PrepareRunResult::Ready(output_path))
}

/// Execute a binary with the given arguments.
pub fn execute_binary(path: &Path, args: &[String]) -> SoarResult<RunResult> {
    debug!(path = %path.display(), args = ?args, "executing binary");

    let status = Command::new(path)
        .args(args)
        .status()
        .with_context(|| format!("executing command {}", path.display()))?;

    Ok(RunResult {
        exit_code: status.code().unwrap_or(-1),
    })
}

fn download_to_cache(
    ctx: &SoarContext,
    package: &Package,
    output_path: &Path,
    cache_bin: &Path,
    progress_callback: Arc<dyn Fn(soar_dl::types::Progress) + Send + Sync>,
) -> SoarResult<()> {
    let _ = ctx;
    if let Some(ref url) = package.ghcr_blob {
        let cb = progress_callback.clone();
        let mut dl = OciDownload::new(url.as_str())
            .output(output_path.to_string_lossy())
            .overwrite(OverwriteMode::Force);
        dl = dl.progress(move |p| {
            cb(p);
        });
        dl.execute()?;
    } else {
        let extract_dir = get_extract_dir(cache_bin);
        let cb = progress_callback.clone();
        let mut dl = Download::new(&package.download_url)
            .output(output_path.to_string_lossy())
            .overwrite(OverwriteMode::Force)
            .extract(true)
            .extract_to(&extract_dir);
        dl = dl.progress(move |p| {
            cb(p);
        });

        let file_name = dl.execute()?;
        if extract_dir.exists() {
            fs::remove_file(file_name).ok();

            for entry in fs::read_dir(&extract_dir)
                .with_context(|| format!("reading {} directory", extract_dir.display()))?
            {
                let entry = entry.with_context(|| {
                    format!("reading entry from directory {}", extract_dir.display())
                })?;
                let from = entry.path();
                let to = cache_bin.join(entry.file_name());
                fs::rename(&from, &to)
                    .with_context(|| format!("renaming {} to {}", from.display(), to.display()))?;
            }

            fs::remove_dir_all(&extract_dir).ok();
        }
    }

    Ok(())
}
