use std::{
    collections::HashSet,
    fmt::Display,
    fs,
    io::Write,
    os::unix,
    path::{Path, PathBuf},
    sync::{LazyLock, RwLock},
};

use indicatif::HumanBytes;
use nu_ansi_term::Color::{self, Blue, Cyan, Green, LightRed, Magenta, Red};
use serde::Serialize;
use soar_core::{
    config::get_config,
    database::{
        models::{Package, PackageExt},
        packages::{PackageProvide, ProvideStrategy},
    },
    error::{ErrorContext, SoarError},
    package::install::InstallTarget,
    repositories::get_platform_repositories,
    utils::get_platform,
    SoarResult,
};
use soar_dl::utils::{is_elf, FileMode};
use tracing::{error, info};

pub static COLOR: LazyLock<RwLock<bool>> = LazyLock::new(|| RwLock::new(true));

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

pub fn select_package_interactively<T: PackageExt>(
    pkgs: Vec<T>,
    package_name: &str,
) -> SoarResult<Option<T>> {
    info!("Multiple packages found for {package_name}");
    for (idx, pkg) in pkgs.iter().enumerate() {
        info!(
            "[{}] {}#{}:{} | {}",
            idx + 1,
            Colored(Blue, &pkg.pkg_name()),
            Colored(Cyan, &pkg.pkg_id()),
            Colored(Green, pkg.repo_name()),
            Colored(LightRed, pkg.version())
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

pub fn get_file_mode(skip_existing: bool, force_overwrite: bool) -> FileMode {
    if force_overwrite {
        FileMode::ForceOverwrite
    } else if skip_existing {
        FileMode::SkipExisting
    } else {
        FileMode::PromptOverwrite
    }
}

pub async fn mangle_package_symlinks(
    install_dir: &Path,
    bin_dir: &Path,
    provides: Option<&[PackageProvide]>,
) -> SoarResult<Vec<(PathBuf, PathBuf)>> {
    let mut symlinks = Vec::new();

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
        let soar_syms = Path::new("SOAR_SYMS");
        let (is_syms, binaries_dir) = if soar_syms.is_dir() {
            (true, soar_syms)
        } else {
            (false, install_dir)
        };
        for entry in fs::read_dir(binaries_dir).with_context(|| {
            format!(
                "reading install directory {} for ELF detection",
                install_dir.display()
            )
        })? {
            let path = entry
                .with_context(|| {
                    format!(
                        "reading entry in directory {} for ELF detection",
                        install_dir.display()
                    )
                })?
                .path();
            if path.is_file() && (is_syms || is_elf(&path).await) {
                if let Some(file_name) = path.file_name() {
                    let symlink_target_path = bin_dir.join(file_name);
                    if symlink_target_path.is_symlink() || symlink_target_path.is_file() {
                        std::fs::remove_file(&symlink_target_path).with_context(|| {
                            format!(
                                "removing existing file/symlink at {}",
                                symlink_target_path.display()
                            )
                        })?;
                    }
                    unix::fs::symlink(&path, &symlink_target_path).with_context(|| {
                        format!(
                            "creating ELF symlink {} -> {}",
                            path.display(),
                            symlink_target_path.display()
                        )
                    })?;
                    symlinks.push((path.clone(), symlink_target_path.clone()));
                }
            }
        }
    }
    Ok(symlinks)
}

pub fn parse_default_repos_arg(arg: &str) -> SoarResult<String> {
    let repo = arg.trim().to_lowercase();
    let platform = get_platform();

    let supported_repos: Vec<&str> = get_platform_repositories()
        .into_iter()
        .filter(|repo| repo.platforms.contains(&platform.as_str()))
        .map(|repo| repo.name)
        .collect();

    if supported_repos.contains(&repo.as_str()) {
        Ok(repo)
    } else {
        Err(SoarError::Custom(format!(
            "Invalid repository '{}'. Valid options for this platform ({}) are: {}",
            repo,
            platform,
            supported_repos.join(", ")
        )))
    }
}
