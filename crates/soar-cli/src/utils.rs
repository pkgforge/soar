use std::{
    collections::HashSet,
    fmt::Display,
    fs,
    io::Write,
    os::{unix, unix::fs::PermissionsExt as _},
    path::{Path, PathBuf},
    sync::{LazyLock, RwLock},
};

use indicatif::HumanBytes;
use nu_ansi_term::Color::{self, Blue, Cyan, Green, LightRed, Magenta, Red};
use serde::Serialize;
use soar_config::{
    config::get_config, display::DisplaySettings, packages::BinaryMapping,
    repository::get_platform_repositories,
};
use soar_core::{
    database::models::Package,
    error::{ErrorContext, SoarError},
    package::install::InstallTarget,
    SoarResult,
};
use soar_db::models::types::{PackageProvide, ProvideStrategy};
use soar_package::PackageExt;
use soar_utils::{fs::is_elf, system::platform};
use tracing::{error, info};

pub struct Icons;

impl Icons {
    pub const ARROW: &str = "→";
    pub const BROKEN: &str = "✗";
    pub const BUILD: &str = "🔨";
    pub const CALENDAR: &str = "📅";
    pub const CHECK: &str = "✓";
    pub const CHECKSUM: &str = "🔏";
    pub const CROSS: &str = "✗";
    pub const DESCRIPTION: &str = "📝";
    pub const HOME: &str = "🏠";
    pub const INSTALLED: &str = "✓";
    pub const LICENSE: &str = "📜";
    pub const LINK: &str = "🔗";
    pub const LOG: &str = "📄";
    pub const MAINTAINER: &str = "👤";
    pub const NOTE: &str = "📌";
    pub const NOT_INSTALLED: &str = "○";
    pub const PACKAGE: &str = "📦";
    pub const SCRIPT: &str = "📃";
    pub const SIZE: &str = "💾";
    pub const TYPE: &str = "📁";
    pub const VERSION: &str = "🏁";
    pub const WARNING: &str = "⚠";
}

pub fn icon_or<'a>(icon: &'a str, fallback: &'a str) -> &'a str {
    if get_config().display().icons() {
        icon
    } else {
        fallback
    }
}

pub fn display_settings() -> DisplaySettings {
    get_config().display()
}

pub fn term_width() -> usize {
    terminal_size::terminal_size()
        .map(|(w, _)| w.0 as usize)
        .unwrap_or(80)
}

pub static COLOR: LazyLock<RwLock<bool>> = LazyLock::new(|| RwLock::new(true));
pub static PROGRESS: LazyLock<RwLock<bool>> = LazyLock::new(|| RwLock::new(true));

pub fn progress_enabled() -> bool {
    *PROGRESS.read().unwrap()
}

pub fn interactive_ask(ques: &str) -> SoarResult<String> {
    print!("{ques}");

    std::io::stdout()
        .flush()
        .with_context(|| "flushing stdout stream".to_string())?;

    let mut response = String::new();
    std::io::stdin()
        .read_line(&mut response)
        .with_context(|| "reading input from stdin".to_string())?;

    Ok(response.trim().to_owned())
}

pub struct Colored<T: Display>(pub Color, pub T);

impl<T: Display> Display for Colored<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let color = COLOR.read().unwrap();
        if *color {
            write!(f, "{}", self.0.prefix())?;
            self.1.fmt(f)?;
            write!(f, "{}", self.0.suffix())
        } else {
            self.1.fmt(f)
        }
    }
}

pub fn vec_string<T: Display + Serialize>(value: Option<Vec<T>>) -> Option<String> {
    value.and_then(|json| serde_json::to_string(&json).ok())
}

pub fn get_valid_selection(max: usize) -> SoarResult<usize> {
    loop {
        let response = interactive_ask("Select a package: ")?;
        match response.parse::<usize>() {
            Ok(n) if n > 0 && n <= max => return Ok(n - 1),
            _ => error!("Invalid selection, please try again."),
        }
    }
}

pub fn confirm_action(message: &str) -> SoarResult<bool> {
    let response = interactive_ask(&format!("{} [y/N]: ", message))?;
    Ok(matches!(response.to_lowercase().as_str(), "y" | "yes"))
}

pub fn select_package_interactively<T: PackageExt>(
    pkgs: Vec<T>,
    package_name: &str,
) -> SoarResult<Option<T>> {
    select_package_interactively_with_installed(pkgs, package_name, &[])
}

pub fn select_package_interactively_with_installed<T: PackageExt>(
    pkgs: Vec<T>,
    package_name: &str,
    installed: &[(String, String, String)], // (pkg_id, repo_name, version)
) -> SoarResult<Option<T>> {
    info!("Showing available packages for {package_name}");
    for (idx, pkg) in pkgs.iter().enumerate() {
        let is_installed = installed.iter().any(|(pkg_id, repo_name, _version)| {
            pkg.pkg_id() == pkg_id && pkg.repo_name() == repo_name
        });
        let installed_marker = if is_installed {
            format!(" {}", Colored(Color::Yellow, "[installed]"))
        } else {
            String::new()
        };
        info!(
            "[{}] {}#{}:{} | {}{}",
            idx + 1,
            Colored(Blue, &pkg.pkg_name()),
            Colored(Cyan, &pkg.pkg_id()),
            Colored(Green, pkg.repo_name()),
            Colored(LightRed, pkg.version()),
            installed_marker
        );
    }

    let selection = get_valid_selection(pkgs.len())?;
    Ok(pkgs.into_iter().nth(selection))
}

pub fn has_desktop_integration(package: &Package) -> bool {
    match package.desktop_integration {
        Some(false) => false,
        _ => get_config().has_desktop_integration(&package.repo_name),
    }
}

pub fn pretty_package_size(ghcr_size: Option<u64>, size: Option<u64>) -> String {
    ghcr_size
        .map(|size| format!("{}", Colored(Magenta, HumanBytes(size))))
        .or_else(|| size.map(|size| format!("{}", Colored(Magenta, HumanBytes(size)))))
        .unwrap_or_default()
}

pub fn ask_target_action(targets: &[InstallTarget], action: &str) -> SoarResult<()> {
    info!(
        "\n{}\n",
        Colored(
            Green,
            format!(
                "These are the packages that would be {}:",
                if action == "install" {
                    "installed"
                } else {
                    "updated"
                }
            )
        )
    );
    for target in targets {
        info!(
            "{}#{}:{} ({})",
            Colored(Blue, &target.package.pkg_name),
            Colored(Cyan, &target.package.pkg_id),
            Colored(Green, &target.package.repo_name),
            Colored(LightRed, &target.package.version)
        )
    }

    info!(
        "Total: {} packages. Estimated download size: {}\n",
        targets.len(),
        HumanBytes(targets.iter().fold(0, |acc, target| {
            acc + target
                .package
                .ghcr_size
                .or(target.package.size)
                .unwrap_or_default()
        }))
    );
    let response = interactive_ask(&format!(
        "Would you like to {} these packages? [{}/{}] ",
        action,
        Colored(Green, "Yes"),
        Colored(Red, "No")
    ))?
    .to_lowercase();
    let response = response.trim();

    if !response.is_empty() && response != "y" {
        info!("Quitting");
        std::process::exit(0);
    }

    Ok(())
}

pub async fn mangle_package_symlinks(
    install_dir: &Path,
    bin_dir: &Path,
    provides: Option<&[PackageProvide]>,
    pkg_name: &str,
    entrypoint: Option<&str>,
    binaries: Option<&[BinaryMapping]>,
) -> SoarResult<Vec<(PathBuf, PathBuf)>> {
    let mut symlinks = Vec::new();

    // If binaries array is provided, use it for symlink creation
    if let Some(bins) = binaries {
        if !bins.is_empty() {
            for mapping in bins {
                let source_path = install_dir.join(&mapping.source);
                let link_path = bin_dir.join(&mapping.link_as);

                if !source_path.exists() {
                    return Err(SoarError::Custom(format!(
                        "Binary source '{}' not found in package",
                        mapping.source
                    )));
                }

                let metadata = fs::metadata(&source_path)
                    .with_context(|| format!("reading metadata for {}", source_path.display()))?;
                let mut perms = metadata.permissions();
                let mode = perms.mode();
                if mode & 0o111 == 0 {
                    perms.set_mode(mode | 0o755);
                    fs::set_permissions(&source_path, perms).with_context(|| {
                        format!(
                            "setting executable permissions on {}",
                            source_path.display()
                        )
                    })?;
                }

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
            let metadata = fs::metadata(&executable)
                .with_context(|| format!("reading metadata for {}", executable.display()))?;
            let mut perms = metadata.permissions();
            let mode = perms.mode();
            if mode & 0o111 == 0 {
                perms.set_mode(mode | 0o755);
                fs::set_permissions(&executable, perms).with_context(|| {
                    format!("setting executable permissions on {}", executable.display())
                })?;
            }

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

/// Find executable in the install directory using fallback logic.
///
/// Priority order:
/// 1. If entrypoint is specified, use it directly
/// 2. Exact package name match (case-sensitive)
/// 3. Case-insensitive package name match (filename or stem)
/// 4. Search in fallback directories: bin/, usr/bin/, usr/local/bin/
/// 5. Recursive search for matching executable
/// 6. Any ELF file found
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

pub fn parse_default_repos_arg(arg: &str) -> SoarResult<String> {
    let repo = arg.trim().to_lowercase();
    let supported_repos: Vec<&str> = get_platform_repositories()
        .into_iter()
        .filter(|repo| repo.platforms.contains(&platform().as_str()))
        .map(|repo| repo.name)
        .collect();

    if supported_repos.contains(&repo.as_str()) {
        Ok(repo)
    } else {
        Err(SoarError::Custom(format!(
            "Invalid repository '{}'. Valid options for this platform ({}) are: {}",
            repo,
            platform(),
            supported_repos.join(", ")
        )))
    }
}
