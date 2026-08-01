use std::{fs, path::Path, process::Command, sync::Arc};

use soar_core::{
    database::models::Package,
    error::{ErrorContext, SoarError},
    package::{install::apply_file_layout, query::PackageQuery},
    utils::get_extract_dir,
    SoarResult,
};
use soar_db::repository::metadata::MetadataRepository;
use soar_dl::{download::Download, oci::OciDownload, types::OverwriteMode};
use soar_utils::{
    hash::{calculate_checksum, hash_string},
    version::compare_versions,
};
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
    no_verify: bool,
) -> SoarResult<PrepareRunResult> {
    debug!(package_name = package_name, "preparing run");
    let config = ctx.config();
    let cache_bin = config.get_cache_path()?.join("bin");

    let query = PackageQuery::try_from(package_name)?;
    let package_name = query.name.as_deref().unwrap_or(package_name);
    let repo_name = query.repo_name.as_deref().or(repo_name);
    let pkg_id = query.pkg_id.as_deref().or(pkg_id);
    let family = query.family.as_deref();
    let version = query.version.as_deref();

    let metadata_mgr = ctx.metadata_manager().await?;

    let packages: Vec<Package> = if let Some(repo_name) = repo_name {
        metadata_mgr
            .query_repo(repo_name, |conn| {
                MetadataRepository::find_filtered(
                    conn,
                    Some(package_name),
                    pkg_id,
                    family,
                    version,
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
                family,
                version,
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

    let mut packages: Vec<Package> = if let Some(version) = version {
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
            // Several versions of one package are not a choice to put to the
            // caller: running a command means running the current one, unless
            // an explicit @version says otherwise.
            let identity = |p: &Package| {
                (
                    p.pkg_name.clone(),
                    p.pkg_id.clone(),
                    p.pkg_family.clone(),
                    p.repo_name.clone(),
                )
            };
            let first = identity(&packages[0]);
            if packages.iter().all(|p| identity(p) == first) {
                let newest = packages
                    .into_iter()
                    .max_by(|a, b| compare_versions(&a.version, &b.version))
                    .unwrap();
                packages = vec![newest];
            } else {
                return Ok(PrepareRunResult::Ambiguous(AmbiguousPackage {
                    query: package_name.to_string(),
                    candidates: packages,
                }));
            }
        }
    }

    let package = packages.into_iter().next().unwrap().resolve(version);

    // Named like an install. A package that published a checksum is keyed by
    // it, so identical content is shared and different content never is;
    // without one the key is the identity the package was resolved from,
    // repository included.
    let suffix = package
        .bsum
        .as_deref()
        .filter(|s| s.len() >= 12)
        .map(|s| s[..12].to_string())
        .unwrap_or_else(|| {
            let source = package
                .pkg_id
                .as_deref()
                .or(package.ghcr_pkg.as_deref())
                .unwrap_or(package.download_url.as_str());
            hash_string(&format!(
                "{}:{}:{}:{}:{}",
                package.repo_name,
                package.pkg_family.as_deref().unwrap_or_default(),
                package.pkg_name,
                package.version,
                source
            ))[..12]
                .to_string()
        });
    let cache_dir = cache_bin.join(format!(
        "{}-{}-{}",
        package.pkg_name, package.version, suffix
    ));
    let output_path = cache_dir.join(&package.pkg_name);

    // Refuse to execute a package whose integrity cannot be checked. OCI
    // artifacts are digest-verified during download, so they are exempt.
    if !no_verify && package.bsum.is_none() && package.ghcr_blob.is_none() {
        return Err(SoarError::Custom(format!(
            "Refusing to run {}: no checksum to verify integrity (use --no-verify to override)",
            package.pkg_name
        )));
    }

    // A laid-out package keeps its binary, not the artifact it came from, so a
    // cache hit is that binary. Where the binary is the artifact itself the
    // published checksum still describes it, and a cache entry is only reused
    // once it matches; a binary taken out of an archive has no checksum of its
    // own, and the content-addressed directory is what stands in for one.
    let laid_out = package.files.as_deref().and_then(|files| {
        files.iter().find_map(|f| {
            f.to.strip_prefix("bin/")
                .filter(|rest| !rest.contains('/'))
                .map(|_| (cache_dir.join(&f.to), f.source.is_empty()))
        })
    });
    if let Some((binary, is_artifact)) = laid_out.as_ref().filter(|(p, _)| p.exists()) {
        let verified = match package.bsum {
            Some(ref bsum) if *is_artifact && !no_verify => {
                let matches = calculate_checksum(binary)? == *bsum;
                if !matches {
                    debug!(
                        package = %package.pkg_name,
                        "cached binary checksum mismatch; re-downloading"
                    );
                    fs::remove_dir_all(&cache_dir).ok();
                }
                matches
            }
            _ => true,
        };
        if verified {
            return Ok(PrepareRunResult::Ready {
                path: binary.clone(),
                downloaded: false,
            });
        }
    }

    // Reuse a cached binary only after re-verifying it against the expected
    // checksum, so a stale or tampered cache entry is never executed blindly.
    if output_path.exists() {
        match package.bsum {
            Some(ref bsum) if !no_verify => {
                let checksum = calculate_checksum(&output_path)?;
                if checksum == *bsum {
                    return Ok(PrepareRunResult::Ready {
                        path: output_path,
                        downloaded: false,
                    });
                }
                debug!(
                    package = %package.pkg_name,
                    "cached binary checksum mismatch; re-downloading"
                );
                fs::remove_file(&output_path).ok();
            }
            _ => {
                return Ok(PrepareRunResult::Ready {
                    path: output_path,
                    downloaded: false,
                })
            }
        }
    }

    fs::create_dir_all(&cache_dir)
        .with_context(|| format!("creating directory {}", cache_dir.display()))?;

    let op_id = next_op_id();
    let progress_callback =
        create_progress_bridge(ctx.events().clone(), op_id, package.pkg_name.clone());

    download_to_cache(
        &package,
        &output_path,
        &cache_dir,
        no_verify,
        progress_callback,
    )?;

    // The artifact may be an archive, in which case the thing to execute is
    // wherever the package says it is rather than the download itself.
    if let Some(files) = package.files.as_deref().filter(|f| !f.is_empty()) {
        apply_file_layout(files, &cache_dir, &output_path)?;
        if let Some(binary) = files.iter().find_map(|f| {
            f.to.strip_prefix("bin/")
                .filter(|rest| !rest.contains('/'))
                .map(|_| cache_dir.join(&f.to))
        }) {
            if binary.exists() {
                return Ok(PrepareRunResult::Ready {
                    path: binary,
                    downloaded: true,
                });
            }
        }
    }

    Ok(PrepareRunResult::Ready {
        path: output_path,
        downloaded: true,
    })
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
    package: &Package,
    output_path: &Path,
    cache_bin: &Path,
    no_verify: bool,
    progress_callback: Arc<dyn Fn(soar_dl::types::Progress) + Send + Sync>,
) -> SoarResult<()> {
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
        if !no_verify {
            if let Some(ref bsum) = package.bsum {
                dl = dl.checksum(bsum.clone());
            }
        }
        dl = dl.progress(move |p| {
            cb(p);
        });

        let file_name = dl.execute()?;
        if extract_dir.exists() {
            fs::remove_file(&file_name).ok();

            let mut extracted = Vec::new();
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
                extracted.push(to);
            }

            fs::remove_dir_all(&extract_dir).ok();

            if extracted.is_empty() {
                return Err(SoarError::Custom(format!(
                    "Archive contained no files for '{}'",
                    output_path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                )));
            }

            if !output_path.exists() {
                if extracted.len() == 1 {
                    // Single extracted file didn't match the package name; rename it.
                    fs::rename(&extracted[0], output_path).with_context(|| {
                        format!(
                            "renaming {} to {}",
                            extracted[0].display(),
                            output_path.display()
                        )
                    })?;
                } else if extracted.len() > 1 {
                    return Err(SoarError::Custom(format!(
                        "Archive extracted {} files but none matched '{}'. Extracted: {}",
                        extracted.len(),
                        output_path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy(),
                        extracted
                            .iter()
                            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )));
                }
            }
        }
    }

    Ok(())
}
