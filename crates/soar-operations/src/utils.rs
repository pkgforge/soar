use std::{
    collections::{HashMap, HashSet},
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
    utils::{shared_link_targets, substitute_placeholders},
    SoarResult,
};
use soar_db::models::types::{PackageFile, PackageProvide};
use soar_utils::fs::is_elf;
use tracing::{debug, warn};

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

/// Creates the bin-directory symlinks declared by a package's `provides`.
///
/// Provides whose name or target is not a safe single path component are skipped
/// (see [`PackageProvide::is_safe`]) so untrusted metadata cannot escape
/// `install_dir`/`bin_dir` when building symlink paths. Returns the created
/// `(source, link)` pairs.
fn create_provide_symlinks(
    install_dir: &Path,
    bin_dir: &Path,
    provides: &[PackageProvide],
) -> SoarResult<Vec<(PathBuf, PathBuf)>> {
    let mut symlinks = Vec::new();
    let mut processed_paths = HashSet::new();
    for provide in provides {
        if !provide.is_safe() {
            warn!(
                provide = provide.name,
                "skipping provide with unsafe path component"
            );
            continue;
        }
        let real_path = install_dir.join(provide.name.clone());

        for name in provide.bin_symlink_names() {
            let target_path = bin_dir.join(name);
            if !processed_paths.insert(target_path.clone()) {
                continue;
            }
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
    Ok(symlinks)
}

/// Link a package's man pages and completions where the system looks for them.
///
/// A destination already holding something soar did not put there is left
/// alone: a distro package or a hand-written completion outranks ours.
pub fn link_shared_files(
    install_dir: &Path,
    bin_dir: &Path,
    shells: &[String],
) -> SoarResult<Vec<(PathBuf, PathBuf)>> {
    // A link pointing anywhere inside soar's own package tree is soar's,
    // including one left by an older version of this package: an install
    // directory carries its version, so the path never matches the new one.
    let packages_root = install_dir.parent().unwrap_or(install_dir);
    let mut linked = Vec::new();
    for (relative, destination, enabled) in shared_link_targets(bin_dir, shells) {
        if !enabled {
            continue;
        }
        let source_root = install_dir.join(relative);
        if !source_root.is_dir() {
            continue;
        }
        for source in walk_files(&source_root) {
            let Ok(rest) = source.strip_prefix(&source_root) else {
                continue;
            };
            let link = destination.join(rest);
            if let Some(parent) = link.parent() {
                if fs::create_dir_all(parent).is_err() {
                    continue;
                }
            }
            match fs::read_link(&link) {
                // ours, from this package or an older version of it
                Ok(target) if target.starts_with(packages_root) => {
                    fs::remove_file(&link).ok();
                }
                Ok(_) | Err(_) if link.exists() || link.is_symlink() => {
                    debug!(path = %link.display(), "leaving a file soar does not own");
                    continue;
                }
                _ => {}
            }
            if unix::fs::symlink(&source, &link).is_ok() {
                linked.push((source, link));
            }
        }
    }
    Ok(linked)
}

/// Every regular file under `dir`, recursively, skipping symlinks and the
/// bookkeeping entries soar writes alongside a package.
fn walk_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_symlink() {
            continue;
        }
        if path.is_dir() {
            out.extend(walk_files(&path));
        } else if !entry.file_name().to_string_lossy().starts_with('.') {
            out.push(path);
        }
    }
    out
}

/// Creates symlinks from installed package binaries to the bin directory.
#[allow(clippy::too_many_arguments)]
pub async fn mangle_package_symlinks(
    install_dir: &Path,
    bin_dir: &Path,
    provides: Option<&[PackageProvide]>,
    pkg_name: &str,
    version: &str,
    entrypoint: Option<&str>,
    binaries: Option<&[BinaryMapping]>,
    arch_map: Option<&HashMap<String, String>>,
    files: Option<&[PackageFile]>,
) -> SoarResult<Vec<(PathBuf, PathBuf)>> {
    let mut symlinks = Vec::new();

    // A package laid out by its file list has already said what its commands
    // are: everything in `bin/`.
    let listed: Option<Vec<String>> = files.filter(|f| !f.is_empty()).map(|files| {
        files
            .iter()
            .flat_map(|f| std::iter::once(&f.to).chain(f.alias.iter()))
            .cloned()
            .collect()
    });
    // A package installed before the file list existed carries none, so its own
    // `bin/` stands in, but only where nothing else describes the package.
    // Reading the directory otherwise publishes every executable an archive
    // happens to ship, which for a toolchain is dozens of them.
    let listed = listed.or_else(|| {
        if entrypoint.is_some()
            || binaries.is_some_and(|b| !b.is_empty())
            || provides.is_some_and(|p| !p.is_empty())
        {
            return None;
        }
        // Read rather than walk: an alias is a symlink, which walking skips.
        let entries: Vec<String> = fs::read_dir(install_dir.join("bin"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| format!("bin/{}", e.file_name().to_string_lossy()))
            .collect();
        (!entries.is_empty()).then_some(entries)
    });
    if let Some(listed) = listed {
        for path in &listed {
            {
                let Some(name) = path.strip_prefix("bin/").filter(|n| !n.contains('/')) else {
                    continue;
                };
                let source_path = install_dir.join(path);
                if !source_path.exists() {
                    continue;
                }
                let link_path = bin_dir.join(name);
                set_executable(&source_path)?;
                if link_path.is_symlink() || link_path.is_file() {
                    std::fs::remove_file(&link_path).with_context(|| {
                        format!("removing existing file/symlink at {}", link_path.display())
                    })?;
                }
                unix::fs::symlink(&source_path, &link_path)
                    .with_context(|| format!("creating symlink {}", link_path.display()))?;
                symlinks.push((source_path, link_path));
            }
        }
        return Ok(symlinks);
    }

    if let Some(bins) = binaries {
        if !bins.is_empty() {
            // Walked once: the tree does not change while the mappings are
            // resolved against it.
            let present = walk_files(install_dir);
            let rel_of = |path: &PathBuf| {
                path.strip_prefix(install_dir)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .to_string()
            };
            let matching = |pat: &str| -> Vec<PathBuf> {
                present
                    .iter()
                    .filter(|p| fast_glob::glob_match(pat, rel_of(p)))
                    .cloned()
                    .collect()
            };
            // One name cannot stand for two files, so the first mapping to
            // claim it keeps it.
            let mut claimed: HashSet<PathBuf> = HashSet::new();
            for mapping in bins {
                let source_pattern =
                    substitute_placeholders(&mapping.source, Some(version), arch_map);
                // Try the most specific reading of the pattern first. Falling
                // straight back to the file name would pick an arbitrary one
                // when an archive ships the same binary for several
                // architectures, each under its own directory.

                let mut source_paths = matching(&source_pattern);
                // An archive with a single top-level directory has it promoted
                // away, so the recorded path still carries a component the
                // installed tree no longer has.
                if source_paths.is_empty() {
                    if let Some((_, rest)) = source_pattern.split_once('/') {
                        source_paths = matching(rest);
                    }
                }
                // Last resort: the artifact was rearranged and only the name
                // survives. Several files can answer to it, so `link_as` is
                // dropped below and each keeps its own name.
                if source_paths.is_empty() {
                    source_paths = present
                        .iter()
                        .filter(|p| {
                            p.file_name()
                                .map(|n| {
                                    fast_glob::glob_match(
                                        &source_pattern,
                                        n.to_string_lossy().to_string(),
                                    )
                                })
                                .unwrap_or(false)
                        })
                        .cloned()
                        .collect();
                }

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
                    if !claimed.insert(link_path.clone()) {
                        warn!(
                            link = %link_path.display(),
                            source = %source_path.display(),
                            "skipping a second source for the same command"
                        );
                        continue;
                    }

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

    let provides = provides.unwrap_or_default();
    symlinks.extend(create_provide_symlinks(install_dir, bin_dir, provides)?);

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
        perms.set_mode(mode | 0o111);
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

/// What identifies a package apart from its version: repository, name, id and
/// family. Two entries sharing this are two versions of one package.
pub type PackageKey = (String, String, Option<String>, Option<String>);

/// Installed packages by repository and name, each with the family it was
/// installed under, so a package sharing a name is not mistaken for it.
pub type InstalledIndex = HashMap<(String, String), Vec<(Option<String>, bool)>>;

pub fn is_installed(
    map: &InstalledIndex,
    repo_name: &str,
    pkg_name: &str,
    pkg_family: Option<&str>,
) -> bool {
    map.get(&(repo_name.to_string(), pkg_name.to_string()))
        .is_some_and(|rows| {
            rows.iter()
                .any(|(family, installed)| *installed && family.as_deref() == pkg_family)
        })
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{self, File},
        path::PathBuf,
    };

    use soar_db::models::types::PackageProvide;
    use tempfile::{tempdir, TempDir};

    use super::create_provide_symlinks;

    fn setup() -> (TempDir, PathBuf, PathBuf) {
        let root = tempdir().unwrap();
        let install = root.path().join("install");
        let bin = root.path().join("bin");
        fs::create_dir_all(&install).unwrap();
        fs::create_dir_all(&bin).unwrap();
        (root, install, bin)
    }

    fn is_symlink(path: &std::path::Path) -> bool {
        path.symlink_metadata()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
    }

    #[test]
    fn creates_symlink_for_plain_provide() {
        let (_root, install, bin) = setup();
        File::create(install.join("clipcat")).unwrap();

        let provides = vec![PackageProvide::from_string("clipcat")];
        let created = create_provide_symlinks(&install, &bin, &provides).unwrap();

        let link = bin.join("clipcat");
        assert_eq!(created, vec![(install.join("clipcat"), link.clone())]);
        assert_eq!(fs::read_link(&link).unwrap(), install.join("clipcat"));
    }

    #[test]
    fn creates_both_symlinks_for_keep_both() {
        let (_root, install, bin) = setup();
        File::create(install.join("clipcatd")).unwrap();

        let provides = vec![PackageProvide::from_string("clipcatd==clipcat")];
        let created = create_provide_symlinks(&install, &bin, &provides).unwrap();

        assert_eq!(created.len(), 2);
        assert!(is_symlink(&bin.join("clipcatd")));
        assert!(is_symlink(&bin.join("clipcat")));
    }

    #[test]
    fn skips_unsafe_target() {
        let (root, install, bin) = setup();
        // bin.join("../victim") resolves to root/victim, outside bin.
        let victim = root.path().join("victim");
        File::create(&victim).unwrap();

        let provides = vec![PackageProvide::from_string("clipcat=>../victim")];
        let created = create_provide_symlinks(&install, &bin, &provides).unwrap();

        assert!(created.is_empty());
        assert!(
            victim.symlink_metadata().unwrap().file_type().is_file(),
            "unsafe target must not be replaced by a symlink"
        );
    }

    #[test]
    fn skips_unsafe_symlink_to_bin_name() {
        let (root, install, bin) = setup();
        let victim = root.path().join("evil");
        File::create(&victim).unwrap();

        let provides = vec![PackageProvide::from_string("@../evil")];
        let created = create_provide_symlinks(&install, &bin, &provides).unwrap();

        assert!(created.is_empty());
        assert!(victim.symlink_metadata().unwrap().file_type().is_file());
    }
}
