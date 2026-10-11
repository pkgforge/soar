use std::{
    collections::{HashMap, HashSet},
    fs,
    os::unix,
    path::{Path, PathBuf},
};

use soar_config::{
    config::Config,
    packages::{BinaryMapping, PackageHooks, PackagesConfig, SandboxConfig},
};
use soar_core::{
    database::{
        connection::DieselDatabase,
        models::{InstalledPackage, Package},
    },
    error::{ErrorContext, SoarError},
    package::{local::LocalPackage, url::UrlPackage},
    utils::{shared_link_targets, substitute_placeholders},
    SoarResult,
};
use soar_db::{
    models::types::{PackageFile, PackageProvide},
    repository::core::{CoreRepository, SortDirection},
};
use soar_utils::{
    fs::{is_elf, make_executable_contained},
    path::is_safe_component,
};
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

/// Whether soar may replace whatever stands at `link` in a bin directory.
///
/// Only a link soar owns, resolving into its own packages tree, or a
/// genuinely dangling link. Anything else belongs to someone else and is
/// left alone.
///
/// Targets are resolved before comparing: a raw target can smuggle `..`
/// past a lexical prefix check. Anything unresolvable stays put. A target
/// that exists but cannot be read is not dangling: `exists` hides every
/// metadata error, so only `NotFound` permits replacement.
fn replaceable_link(link: &Path, packages_root: &Path) -> bool {
    match fs::read_link(link) {
        Ok(target) => {
            match fs::metadata(link) {
                Ok(_) => {
                    let absolute = if target.is_absolute() {
                        target
                    } else {
                        link.parent().unwrap_or_else(|| Path::new(".")).join(target)
                    };
                    fs::canonicalize(&absolute)
                        .and_then(|resolved| {
                            fs::canonicalize(packages_root).map(|root| resolved.starts_with(root))
                        })
                        .unwrap_or(false)
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                    debug!(path = %link.display(), target = %target.display(), "claiming a dangling link");
                    true
                }
                Err(_) => false,
            }
        }
        Err(_) => fs::metadata(link).is_err_and(|err| err.kind() == std::io::ErrorKind::NotFound),
    }
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
    let packages_root = install_dir.parent().unwrap_or(install_dir);
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
            if target_path.exists() || target_path.is_symlink() {
                if !replaceable_link(&target_path, packages_root) {
                    warn!(
                        path = %target_path.display(),
                        "leaving a file soar does not own"
                    );
                    continue;
                }
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
    // Bin links point into this tree, so its parent measures ownership.
    let packages_root = install_dir.parent().unwrap_or(install_dir);

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
                let Some(name) = path.strip_prefix("bin/") else {
                    continue;
                };
                // `bin/..` holds no slash but still escapes the bin
                // directory, so a slash check is not enough.
                if !is_safe_component(name) {
                    warn!(entry = name, "skipping file list entry with unsafe name");
                    continue;
                }
                let source_path = install_dir.join(path);
                if !source_path.exists() {
                    continue;
                }
                let link_path = bin_dir.join(name);
                if link_path.exists() || link_path.is_symlink() {
                    if !replaceable_link(&link_path, packages_root) {
                        warn!(
                            path = %link_path.display(),
                            "leaving a file soar does not own"
                        );
                        continue;
                    }
                    std::fs::remove_file(&link_path).with_context(|| {
                        format!("removing existing file/symlink at {}", link_path.display())
                    })?;
                }
                set_executable(&source_path, install_dir)?;
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
                    // `link_as` comes from package metadata; ungated it
                    // escapes the bin directory.
                    if !is_safe_component(link_name) {
                        warn!(
                            link_as = link_name,
                            source = %source_path.display(),
                            "skipping binary mapping with unsafe link name"
                        );
                        continue;
                    }
                    let link_path = bin_dir.join(link_name);
                    if !claimed.insert(link_path.clone()) {
                        warn!(
                            link = %link_path.display(),
                            source = %source_path.display(),
                            "skipping a second source for the same command"
                        );
                        continue;
                    }

                    set_executable(&source_path, install_dir)?;

                    if link_path.exists() || link_path.is_symlink() {
                        if !replaceable_link(&link_path, packages_root) {
                            warn!(
                                path = %link_path.display(),
                                "leaving a file soar does not own"
                            );
                            continue;
                        }
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
            set_executable(&executable, install_dir)?;

            // Upstream refuses these too; this holds for direct callers.
            if !is_safe_component(pkg_name) {
                return Err(SoarError::Custom(format!(
                    "Refusing to link binary: package name is not a valid path component: {pkg_name}"
                )));
            }
            let symlink_name = bin_dir.join(pkg_name);
            if symlink_name.exists() || symlink_name.is_symlink() {
                if !replaceable_link(&symlink_name, packages_root) {
                    warn!(
                        path = %symlink_name.display(),
                        "leaving a file soar does not own"
                    );
                    return Ok(symlinks);
                }
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

/// Adds the executable bits to `path`, which must live under `dir`.
///
/// `path` may itself be a symlink: recipe aliases are relative links, and
/// an integrated file is the user's own. The chmod stays bound to open
/// descriptors: a symlinked directory on the way down is only descended
/// through when it resolves inside `dir`, and an alias resolving outside
/// is linked as-is with a warning. Anything else non-regular is refused.
fn set_executable(path: &Path, dir: &Path) -> SoarResult<()> {
    if !make_executable_contained(dir, path)? {
        // The target keeps its mode; the caller links it as-is.
        warn!(
            path = %path.display(),
            "leaving mode alone outside the package tree; make it executable yourself"
        );
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
        if path.is_symlink() {
            continue;
        }
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

/// How many packages a repository offers under each name.
pub type NameCounts = HashMap<(String, String), usize>;

pub fn is_installed(
    map: &InstalledIndex,
    offered: &NameCounts,
    repo_name: &str,
    pkg_name: &str,
    pkg_family: Option<&str>,
) -> bool {
    let key = (repo_name.to_string(), pkg_name.to_string());
    let Some(rows) = map.get(&key) else {
        return false;
    };

    if rows
        .iter()
        .any(|(family, installed)| *installed && family.as_deref() == pkg_family)
    {
        return true;
    }

    // Metadata can drop or rename the family a package was installed under,
    // leaving nothing to match on. Where the repository offers the name once
    // and one package is held under it, there is nothing else either could
    // mean. Anything less certain is left unmatched rather than guessed.
    if offered.get(&key).copied().unwrap_or(0) != 1 {
        return false;
    }

    let mut held = rows.iter().filter(|(_, installed)| *installed);
    held.next().is_some() && held.next().is_none()
}

/// The installed packages a URL or local path refers to.
///
/// A package installed from a URL is named by that URL as readily as by the
/// name derived from it, and the URL is what the caller has. An update
/// replaces the recorded source with the one it fetched, so the URL a package
/// was installed from stops matching it; it still names the package it
/// produced, which is what the caller means by it.
pub fn installed_from_source(
    diesel_db: &DieselDatabase,
    source: &str,
) -> SoarResult<Vec<InstalledPackage>> {
    let installed: Vec<InstalledPackage> = diesel_db
        .with_conn(|conn| CoreRepository::find_by_download_url(conn, source))?
        .into_iter()
        .map(Into::into)
        .filter(|ip: &InstalledPackage| ip.is_installed)
        .collect();
    if !installed.is_empty() {
        return Ok(installed);
    }

    let Some((name, family)) = package_from_source(source) else {
        return Ok(Vec::new());
    };
    Ok(diesel_db
        .with_conn(|conn| {
            CoreRepository::list_filtered(
                conn,
                Some("local"),
                Some(&name),
                None,
                None,
                Some(true),
                None,
                None,
                Some(SortDirection::Asc),
            )
        })?
        .into_iter()
        .map(Into::into)
        .filter(|ip: &InstalledPackage| ip.pkg_family.as_deref() == family.as_deref())
        .collect())
}

/// The package name and family a URL or local path installs as.
fn package_from_source(source: &str) -> Option<(String, Option<String>)> {
    let package = if LocalPackage::is_local(source) {
        LocalPackage::from_path(source, None, None, None, None)
            .ok()?
            .to_package()
    } else {
        UrlPackage::from_remote(source, None, None, None, None)
            .ok()?
            .to_package()
    };
    Some((package.pkg_name, package.pkg_family))
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        fs::{self, File},
        path::PathBuf,
    };

    use soar_config::packages::BinaryMapping;
    use soar_db::models::types::{PackageFile, PackageProvide};
    use tempfile::{tempdir, TempDir};

    use super::{
        create_provide_symlinks, is_installed, mangle_package_symlinks, replaceable_link,
        InstalledIndex, NameCounts,
    };
    /// One installed package of `name`, recorded under `family`.
    fn installed_as(name: &str, family: Option<&str>, installed: bool) -> InstalledIndex {
        HashMap::from([(
            ("soarpkgs".to_string(), name.to_string()),
            vec![(family.map(str::to_string), installed)],
        )])
    }

    /// A repository offering `name` that many times over.
    fn offering(name: &str, times: usize) -> NameCounts {
        HashMap::from([(("soarpkgs".to_string(), name.to_string()), times)])
    }

    #[test]
    fn a_family_dropped_by_later_metadata_still_matches_what_was_installed() {
        // Installed when the metadata named the family after the package
        // itself. The repository has since stopped carrying one.
        let index = installed_as("widget", Some("widget"), true);

        assert!(is_installed(
            &index,
            &offering("widget", 1),
            "soarpkgs",
            "widget",
            None
        ));
        assert!(is_installed(
            &index,
            &offering("widget", 1),
            "soarpkgs",
            "widget",
            Some("widget")
        ));
    }

    #[test]
    fn a_family_that_tells_two_packages_apart_still_has_to_match() {
        // busybox is not the package's own name, so it goes on counting.
        let index = installed_as("cat", Some("busybox"), true);

        assert!(is_installed(
            &index,
            &offering("cat", 2),
            "soarpkgs",
            "cat",
            Some("busybox")
        ));
        assert!(!is_installed(
            &index,
            &offering("cat", 2),
            "soarpkgs",
            "cat",
            Some("toybox")
        ));
        assert!(!is_installed(
            &index,
            &offering("cat", 2),
            "soarpkgs",
            "cat",
            None
        ));
    }

    #[test]
    fn a_family_renamed_outright_matches_where_the_name_stands_for_one_package() {
        // Installed under a family the metadata no longer carries at all, so
        // there is nothing left to match on but the name.
        let index = installed_as("newpipe", Some("some.long.upstream-appimage"), true);

        assert!(is_installed(
            &index,
            &offering("newpipe", 1),
            "soarpkgs",
            "newpipe",
            None
        ));
    }

    #[test]
    fn a_name_the_repository_offers_twice_is_never_matched_on_the_name_alone() {
        let index = installed_as("cat", Some("busybox"), true);

        assert!(!is_installed(
            &index,
            &offering("cat", 2),
            "soarpkgs",
            "cat",
            None
        ));
    }

    #[test]
    fn a_name_held_twice_over_is_not_matched_on_the_name_alone() {
        // Two builds of the name held at once, so which was meant is unclear
        // however few the repository offers.
        let index: InstalledIndex = HashMap::from([(
            ("soarpkgs".to_string(), "cat".to_string()),
            vec![
                (Some("busybox".to_string()), true),
                (Some("toybox".to_string()), true),
            ],
        )]);

        assert!(!is_installed(
            &index,
            &offering("cat", 1),
            "soarpkgs",
            "cat",
            None
        ));
    }

    #[test]
    fn a_package_only_ever_uninstalled_is_not_installed() {
        let index = installed_as("widget", Some("widget"), false);

        assert!(!is_installed(
            &index,
            &offering("widget", 1),
            "soarpkgs",
            "widget",
            None
        ));
    }

    fn setup() -> (TempDir, PathBuf, PathBuf) {
        let root = tempdir().unwrap();
        let install = root.path().join("install");
        let bin = root.path().join("bin");
        fs::create_dir_all(&install).unwrap();
        fs::create_dir_all(&bin).unwrap();
        (root, install, bin)
    }

    /// A live target that only lexically sits under the packages root is
    /// someone else's link: `..` resolves outside it.
    #[test]
    fn a_dotdot_target_is_not_owned() {
        use std::os::unix::fs::symlink;
        let root = tempdir().unwrap();
        let packages = root.path().join("packages");
        let install = packages.join("pkg-1.0");
        let bin = root.path().join("bin");
        fs::create_dir_all(&install).unwrap();
        fs::create_dir_all(&bin).unwrap();
        let outside = root.path().join("outside");
        fs::write(&outside, b"mine").unwrap();
        let link = bin.join("tool");
        symlink(format!("{}/../../outside", install.display()), &link).unwrap();
        assert!(
            link.exists(),
            "target resolves, so the link is not dangling"
        );
        assert!(!replaceable_link(&link, &packages));
        assert!(link.symlink_metadata().is_ok(), "the link stays");
    }

    /// A link whose target cannot even be statted is not dangling: a loop
    /// reads as missing through `exists` but must stay put.
    #[test]
    fn an_unreadable_target_is_not_dangling() {
        use std::os::unix::fs::symlink;
        let root = tempdir().unwrap();
        let packages = root.path().join("packages");
        let bin = root.path().join("bin");
        fs::create_dir_all(&packages).unwrap();
        fs::create_dir_all(&bin).unwrap();
        let a = bin.join("a");
        let b = bin.join("b");
        symlink(&b, &a).unwrap();
        symlink(&a, &b).unwrap();
        assert!(!a.exists(), "a loop never resolves");
        assert!(!replaceable_link(&a, &packages));
        assert!(a.symlink_metadata().is_ok(), "the link stays");
    }

    #[test]
    fn a_target_inside_the_root_is_owned() {
        use std::os::unix::fs::symlink;
        let root = tempdir().unwrap();
        let packages = root.path().join("packages");
        let install = packages.join("pkg-1.0");
        let bin = root.path().join("bin");
        fs::create_dir_all(&install).unwrap();
        fs::create_dir_all(&bin).unwrap();
        let target = install.join("tool");
        fs::write(&target, b"#!/bin/sh\n").unwrap();
        let link = bin.join("tool");
        symlink(&target, &link).unwrap();
        assert!(replaceable_link(&link, &packages));
    }

    fn is_symlink(path: &std::path::Path) -> bool {
        path.symlink_metadata()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
    }

    fn permission_bits(path: &std::path::Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        path.metadata().unwrap().permissions().mode() & 0o777
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

    #[test]
    fn leaves_a_foreign_file_for_provide_names() {
        let (_root, install, bin) = setup();
        File::create(install.join("clipcat")).unwrap();
        let keep = bin.join("clipcat");
        fs::write(&keep, b"mine").unwrap();

        let provides = vec![PackageProvide::from_string("clipcat")];
        let created = create_provide_symlinks(&install, &bin, &provides).unwrap();

        assert!(created.is_empty());
        assert_eq!(fs::read(&keep).unwrap(), b"mine");
    }

    #[test]
    fn replaces_an_owned_provide_symlink() {
        use std::os::unix::fs::symlink;
        let (_root, install, bin) = setup();
        let target = install.join("clipcat");
        File::create(&target).unwrap();
        symlink(&target, bin.join("clipcat")).unwrap();

        let provides = vec![PackageProvide::from_string("clipcat")];
        let created = create_provide_symlinks(&install, &bin, &provides).unwrap();

        assert_eq!(created.len(), 1);
        assert_eq!(fs::read_link(bin.join("clipcat")).unwrap(), target);
    }

    #[tokio::test]
    async fn a_foreign_file_in_bin_dir_survives_mangling() {
        let (_root, install, bin) = setup();
        fs::write(install.join("tool"), b"#!/bin/sh\n").unwrap();
        let keep = bin.join("tool");
        fs::write(&keep, b"mine").unwrap();

        let binaries = vec![BinaryMapping {
            source: "tool".to_string(),
            link_as: None,
        }];
        let created = mangle_package_symlinks(
            &install,
            &bin,
            None,
            "pkg",
            "1.0",
            None,
            Some(&binaries),
            None,
            None,
        )
        .await
        .unwrap();

        assert!(created.is_empty());
        assert_eq!(fs::read(&keep).unwrap(), b"mine");
    }

    #[tokio::test]
    async fn a_foreign_symlink_in_bin_dir_survives_mangling() {
        use std::os::unix::fs::symlink;
        let (_root, install, bin) = setup();
        let other = tempdir().unwrap();
        let foreign_target = other.path().join("tool");
        fs::write(&foreign_target, b"foreign").unwrap();
        fs::write(install.join("tool"), b"#!/bin/sh\n").unwrap();
        symlink(&foreign_target, bin.join("tool")).unwrap();

        let binaries = vec![BinaryMapping {
            source: "tool".to_string(),
            link_as: None,
        }];
        let created = mangle_package_symlinks(
            &install,
            &bin,
            None,
            "pkg",
            "1.0",
            None,
            Some(&binaries),
            None,
            None,
        )
        .await
        .unwrap();

        assert!(created.is_empty());
        assert_eq!(fs::read_link(bin.join("tool")).unwrap(), foreign_target);
    }

    #[tokio::test]
    async fn an_owned_link_is_replaced() {
        use std::os::unix::fs::symlink;
        let (_root, install, bin) = setup();
        let source = install.join("tool");
        fs::write(&source, b"#!/bin/sh\n").unwrap();
        symlink(&source, bin.join("tool")).unwrap();

        let binaries = vec![BinaryMapping {
            source: "tool".to_string(),
            link_as: None,
        }];
        let created = mangle_package_symlinks(
            &install,
            &bin,
            None,
            "pkg",
            "1.0",
            None,
            Some(&binaries),
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(created.len(), 1);
        assert_eq!(fs::read_link(bin.join("tool")).unwrap(), source);
    }

    #[tokio::test]
    async fn a_dangling_link_is_claimed() {
        use std::os::unix::fs::symlink;
        let (_root, install, bin) = setup();
        let source = install.join("tool");
        fs::write(&source, b"#!/bin/sh\n").unwrap();
        symlink("/nonexistent/soar-test-target", bin.join("tool")).unwrap();

        let binaries = vec![BinaryMapping {
            source: "tool".to_string(),
            link_as: None,
        }];
        let created = mangle_package_symlinks(
            &install,
            &bin,
            None,
            "pkg",
            "1.0",
            None,
            Some(&binaries),
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(created.len(), 1);
        assert_eq!(fs::read_link(bin.join("tool")).unwrap(), source);
    }

    #[tokio::test]
    async fn a_symlinked_source_is_never_chmodded_through() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let (_root, install, bin) = setup();
        let other = tempdir().unwrap();
        let victim = other.path().join("tool");
        fs::write(&victim, b"#!/bin/sh\n").unwrap();
        fs::set_permissions(&victim, fs::Permissions::from_mode(0o644)).unwrap();
        let packaged_bin = install.join("bin");
        fs::create_dir_all(&packaged_bin).unwrap();
        symlink(&victim, packaged_bin.join("tool")).unwrap();

        let files = vec![PackageFile {
            source: String::new(),
            to: "bin/tool".to_string(),
            alias: Vec::new(),
        }];
        let created = mangle_package_symlinks(
            &install,
            &bin,
            None,
            "pkg",
            "1.0",
            None,
            None,
            None,
            Some(&files),
        )
        .await
        .unwrap();

        assert_eq!(created.len(), 1);
        assert_eq!(permission_bits(&victim), 0o644);
        assert_eq!(
            fs::read_link(bin.join("tool")).unwrap(),
            packaged_bin.join("tool")
        );
    }

    #[tokio::test]
    async fn an_alias_beside_its_target_still_links() {
        use std::os::unix::fs::symlink;
        let (_root, install, bin) = setup();
        let packaged_bin = install.join("bin");
        fs::create_dir_all(&packaged_bin).unwrap();
        fs::write(packaged_bin.join("tool"), b"#!/bin/sh\n").unwrap();
        symlink("tool", packaged_bin.join("alias")).unwrap();

        let files = vec![PackageFile {
            source: String::new(),
            to: "bin/alias".to_string(),
            alias: Vec::new(),
        }];
        let created = mangle_package_symlinks(
            &install,
            &bin,
            None,
            "pkg",
            "1.0",
            None,
            None,
            None,
            Some(&files),
        )
        .await
        .unwrap();

        assert_eq!(created.len(), 1);
        assert_eq!(
            fs::read_link(bin.join("alias")).unwrap(),
            packaged_bin.join("alias")
        );
    }

    /// `bin/..` slips past a slash check; the gate must see through it.
    #[tokio::test]
    async fn a_dotdot_file_list_entry_never_leaves_the_bin_dir() {
        let (root, install, bin) = setup();
        let victim = root.path().join("evil");
        File::create(&victim).unwrap();

        let files = vec![PackageFile {
            source: String::new(),
            to: "bin/../evil".to_string(),
            alias: Vec::new(),
        }];
        let created = mangle_package_symlinks(
            &install,
            &bin,
            None,
            "pkg",
            "1.0",
            None,
            None,
            None,
            Some(&files),
        )
        .await
        .unwrap();

        assert!(created.is_empty());
        assert!(
            victim.symlink_metadata().unwrap().file_type().is_file(),
            "unsafe entry must not replace the file outside bin"
        );
    }

    /// `link_as` is free-form metadata; unchecked it escapes the bin dir.
    #[tokio::test]
    async fn an_unsafe_link_as_never_leaves_the_bin_dir() {
        let (root, install, bin) = setup();
        fs::write(install.join("tool"), b"#!/bin/sh\n").unwrap();
        let victim = root.path().join("evil");
        File::create(&victim).unwrap();

        let binaries = vec![BinaryMapping {
            source: "tool".to_string(),
            link_as: Some("../evil".to_string()),
        }];
        let created = mangle_package_symlinks(
            &install,
            &bin,
            None,
            "pkg",
            "1.0",
            None,
            Some(&binaries),
            None,
            None,
        )
        .await
        .unwrap();

        assert!(created.is_empty());
        assert!(
            victim.symlink_metadata().unwrap().file_type().is_file(),
            "unsafe link name must not replace the file outside bin"
        );
    }

    /// The fallback link deletes first, so an unsafe name is refused here.
    #[tokio::test]
    async fn an_unsafe_package_name_refuses_the_fallback_link() {
        let (_root, install, bin) = setup();
        // ELF magic only: discovery reads the header, never executes.
        fs::write(install.join("tool"), [0x7f, b'E', b'L', b'F']).unwrap();

        let err = mangle_package_symlinks(
            &install, &bin, None, "../evil", "1.0", None, None, None, None,
        )
        .await
        .unwrap_err();

        assert!(
            err.to_string().contains("not a valid path component"),
            "{err}"
        );
    }
}
