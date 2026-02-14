use std::{
    collections::HashSet,
    fs,
    os::{unix, unix::fs::PermissionsExt},
    path::{Path, PathBuf},
};

use soar_config::{
    config::Config,
    packages::{BinaryMapping, PackageHooks, PackagesConfig, SandboxConfig},
};
use soar_core::{
    database::models::Package,
    error::{ErrorContext, SoarError},
    utils::substitute_placeholders,
    SoarResult,
};
use soar_db::models::types::{PackageProvide, ProvideStrategy};
use soar_utils::fs::is_elf;

/// Check if a package should have desktop integration (desktop files, icons).
pub fn has_desktop_integration(package: &Package, config: &Config) -> bool {
    match package.desktop_integration {
        Some(false) => false,
        _ => config.has_desktop_integration(&package.repo_name),
    }
}

/// Look up hooks and sandbox configuration for a package from packages.toml.
pub fn get_package_hooks(pkg_name: &str) -> (Option<PackageHooks>, Option<SandboxConfig>) {
    let config = match PackagesConfig::load(None) {
        Ok(c) => c,
        Err(_) => return (None, None),
    };

    config
        .resolved_packages()
        .into_iter()
        .find(|p| p.name == pkg_name)
        .map(|p| (p.hooks, p.sandbox))
        .unwrap_or((None, None))
}

/// Creates symlinks from installed package binaries to the bin directory.
pub async fn mangle_package_symlinks(
    install_dir: &Path,
    bin_dir: &Path,
    provides: Option<&[PackageProvide]>,
    pkg_name: &str,
    version: &str,
    entrypoint: Option<&str>,
    binaries: Option<&[BinaryMapping]>,
) -> SoarResult<Vec<(PathBuf, PathBuf)>> {
    let mut symlinks = Vec::new();

    if let Some(bins) = binaries {
        if !bins.is_empty() {
            for mapping in bins {
                let source_pattern = substitute_placeholders(&mapping.source, Some(version));
                let source_paths: Vec<PathBuf> = fs::read_dir(install_dir)
                    .with_context(|| format!("reading directory {}", install_dir.display()))?
                    .filter_map(|entry| entry.ok())
                    .filter(|entry| {
                        let name = entry.file_name();
                        fast_glob::glob_match(&source_pattern, name.to_string_lossy().to_string())
                    })
                    .map(|entry| entry.path())
                    .collect();

                if source_paths.is_empty() {
                    return Err(SoarError::Custom(format!(
                        "Binary source '{}' not found in package",
                        source_pattern
                    )));
                }

                let single_match = source_paths.len() == 1;
                for source_path in source_paths {
                    let link_name = if single_match {
                        mapping.link_as.as_deref()
                    } else {
                        None
                    }
                    .unwrap_or_else(|| {
                        source_path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or(&mapping.source)
                    });
                    let link_path = bin_dir.join(link_name);

                    set_executable(&source_path)?;

                    if link_path.is_symlink() || link_path.is_file() {
                        std::fs::remove_file(&link_path).with_context(|| {
                            format!("removing existing file/symlink at {}", link_path.display())
                        })?;
                    }

                    unix::fs::symlink(&source_path, &link_path).with_context(|| {
                        format!(
                            "creating symlink {} -> {}",
                            source_path.display(),
                            link_path.display()
                        )
                    })?;
                    symlinks.push((source_path, link_path));
                }
            }
            return Ok(symlinks);
        }
    }

    let mut processed_paths = HashSet::new();
    let provides = provides.unwrap_or_default();
    for provide in provides {
        let real_path = install_dir.join(provide.name.clone());
        let mut symlink_targets = Vec::new();

        if let Some(ref target) = provide.target {
            if provide.strategy.is_some() {
                let target_path = bin_dir.join(target);
                if processed_paths.insert(target_path.clone()) {
                    symlink_targets.push(target_path);
                }
            }
        };

        let needs_original_symlink = matches!(
            (provide.target.as_ref(), provide.strategy.clone()),
            (Some(_), Some(ProvideStrategy::KeepBoth)) | (None, _)
        );

        if needs_original_symlink {
            let original_path = bin_dir.join(&provide.name);
            if processed_paths.insert(original_path.clone()) {
                symlink_targets.push(original_path);
            }
        }

        for target_path in symlink_targets {
            if target_path.is_symlink() || target_path.is_file() {
                std::fs::remove_file(&target_path)
                    .with_context(|| format!("removing provide {}", target_path.display()))?;
            }
            unix::fs::symlink(&real_path, &target_path).with_context(|| {
                format!(
                    "creating symlink {} -> {}",
                    real_path.display(),
                    target_path.display()
                )
            })?;
            symlinks.push((real_path.clone(), target_path));
        }
    }

    if provides.is_empty() {
        let soar_syms = install_dir.join("SOAR_SYMS");
        let (is_syms, binaries_dir) = if soar_syms.is_dir() {
            (true, soar_syms.as_path())
        } else {
            (false, install_dir)
        };

        if let Some(executable) =
            find_executable(install_dir, binaries_dir, is_syms, pkg_name, entrypoint)?
        {
            set_executable(&executable)?;

            let symlink_name = bin_dir.join(pkg_name);
            if symlink_name.is_symlink() || symlink_name.is_file() {
                std::fs::remove_file(&symlink_name).with_context(|| {
                    format!(
                        "removing existing file/symlink at {}",
                        symlink_name.display()
                    )
                })?;
            }
            unix::fs::symlink(&executable, &symlink_name).with_context(|| {
                format!(
                    "creating symlink {} -> {}",
                    executable.display(),
                    symlink_name.display()
                )
            })?;
            symlinks.push((executable, symlink_name));
        }
    }
    Ok(symlinks)
}

fn set_executable(path: &Path) -> SoarResult<()> {
    let metadata =
        fs::metadata(path).with_context(|| format!("reading metadata for {}", path.display()))?;
    let mut perms = metadata.permissions();
    let mode = perms.mode();
    if mode & 0o111 == 0 {
        perms.set_mode(mode | 0o755);
        fs::set_permissions(path, perms)
            .with_context(|| format!("setting executable permissions on {}", path.display()))?;
    }
    Ok(())
}

fn find_executable(
    install_dir: &Path,
    binaries_dir: &Path,
    is_syms: bool,
    pkg_name: &str,
    entrypoint: Option<&str>,
) -> SoarResult<Option<PathBuf>> {
    if let Some(entry) = entrypoint {
        let entrypoint_path = install_dir.join(entry);
        if entrypoint_path.is_file() {
            return Ok(Some(entrypoint_path));
        }
        if binaries_dir != install_dir {
            let entrypoint_in_syms = binaries_dir.join(entry);
            if entrypoint_in_syms.is_file() {
                return Ok(Some(entrypoint_in_syms));
            }
        }
    }

    let files: Vec<PathBuf> = fs::read_dir(binaries_dir)
        .with_context(|| {
            format!(
                "reading directory {} for executable discovery",
                binaries_dir.display()
            )
        })?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file() && (is_syms || is_elf(p)))
        .collect();

    let pkg_name_lower = pkg_name.to_lowercase();

    if let Some(found) = find_matching_executable(&files, pkg_name, &pkg_name_lower) {
        return Ok(Some(found));
    }

    let fallback_dirs = ["bin", "usr/bin", "usr/local/bin"];
    for fallback in fallback_dirs {
        let fallback_path = install_dir.join(fallback);
        if fallback_path.is_dir() {
            let exact_path = fallback_path.join(pkg_name);
            if exact_path.is_file() && is_elf(&exact_path) {
                return Ok(Some(exact_path));
            }
            if let Ok(entries) = fs::read_dir(&fallback_path) {
                let fallback_files: Vec<PathBuf> = entries
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| p.is_file() && is_elf(p))
                    .collect();
                if let Some(found) =
                    find_matching_executable(&fallback_files, pkg_name, &pkg_name_lower)
                {
                    return Ok(Some(found));
                }
            }
        }
    }

    let mut all_files = Vec::new();
    collect_executables_recursive(install_dir, &mut all_files);

    if let Some(found) = find_matching_executable(&all_files, pkg_name, &pkg_name_lower) {
        return Ok(Some(found));
    }

    Ok(all_files.into_iter().next())
}

fn collect_executables_recursive(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            collect_executables_recursive(&path, files);
        } else if path.is_file() && is_elf(&path) {
            files.push(path);
        }
    }
}

fn find_matching_executable(
    files: &[PathBuf],
    pkg_name: &str,
    pkg_name_lower: &str,
) -> Option<PathBuf> {
    files
        .iter()
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n == pkg_name)
                .unwrap_or(false)
        })
        .or_else(|| {
            files.iter().find(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.to_lowercase() == *pkg_name_lower)
                    .unwrap_or(false)
            })
        })
        .or_else(|| {
            files.iter().find(|p| {
                p.file_stem()
                    .and_then(|n| n.to_str())
                    .map(|n| n.to_lowercase() == *pkg_name_lower)
                    .unwrap_or(false)
            })
        })
        .cloned()
}
