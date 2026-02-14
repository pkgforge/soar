use std::{
    fmt::Display,
    io::Write,
    sync::{LazyLock, RwLock},
};

use nu_ansi_term::Color::{self, Blue, Cyan, Green, LightRed, Magenta, Red};
use serde::Serialize;
use soar_config::{
    config::get_config, display::DisplaySettings, repository::get_platform_repositories,
};
use soar_core::{
    error::{ErrorContext, SoarError},
    package::install::InstallTarget,
    SoarResult,
};
use soar_package::PackageExt;
use soar_utils::{bytes::format_bytes, system::platform};
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

pub fn pretty_package_size(ghcr_size: Option<u64>, size: Option<u64>) -> String {
    ghcr_size
        .map(|size| format!("{}", Colored(Magenta, format_bytes(size, 2))))
        .or_else(|| size.map(|size| format!("{}", Colored(Magenta, format_bytes(size, 2)))))
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
        format_bytes(
            targets.iter().fold(0, |acc, target| {
                acc + target
                    .package
                    .ghcr_size
                    .or(target.package.size)
                    .unwrap_or_default()
            }),
            2
        )
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
