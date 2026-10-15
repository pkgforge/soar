use std::{
    fmt::Display,
    io::{BufRead, Write},
    sync::{LazyLock, RwLock},
};

use nu_ansi_term::Color::{self, Blue, Green, LightRed, Magenta, Red};
use serde::Serialize;
use soar_config::{
    config::get_config, display::DisplaySettings, repository::get_platform_repositories,
};
use soar_core::{
    SoarResult,
    error::{ErrorContext, SoarError},
    package::install::InstallTarget,
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
    pub const MAINTAINER: &str = "👤";
    pub const NOTE: &str = "📌";
    pub const NOT_INSTALLED: &str = "○";
    pub const PACKAGE: &str = "📦";
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
/// Whether `--json` was passed, so output is machine readable.
pub static JSON: LazyLock<RwLock<bool>> = LazyLock::new(|| RwLock::new(false));

pub fn progress_enabled() -> bool {
    *PROGRESS.read().unwrap()
}

pub fn json_enabled() -> bool {
    *JSON.read().unwrap()
}

/// Prints `ques` and reads one line of answer from stdin, trimmed.
///
/// A closed stdin is an error rather than an empty answer, so a prompt that
/// repeats until it gets a valid answer stops instead of spinning.
pub fn interactive_ask(ques: &str) -> SoarResult<String> {
    print!("{ques}");

    std::io::stdout()
        .flush()
        .with_context(|| "flushing stdout stream".to_string())?;

    read_answer(&mut std::io::stdin().lock())
}

fn read_answer(input: &mut impl BufRead) -> SoarResult<String> {
    let mut response = String::new();
    let read = input
        .read_line(&mut response)
        .with_context(|| "reading input from stdin".to_string())?;
    if read == 0 {
        return Err(SoarError::Custom(
            "stdin closed before an answer was given".into(),
        ));
    }

    Ok(response.trim().to_owned())
}

/// Whether `answer` is an explicit yes. Anything else, an empty answer
/// included, is a no.
pub fn is_yes(answer: &str) -> bool {
    answer.eq_ignore_ascii_case("y") || answer.eq_ignore_ascii_case("yes")
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
    Ok(is_yes(&response))
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
    installed: &[(String, Option<String>, String)], // (pkg_name, pkg_family, repo_name)
) -> SoarResult<Option<T>> {
    info!("Showing available packages for {package_name}");
    for (idx, pkg) in pkgs.iter().enumerate() {
        // Matching on the id would treat every id-less package as identical,
        // since they all read as the same empty identity.
        let is_installed = installed.iter().any(|(pkg_name, pkg_family, repo_name)| {
            pkg.pkg_name() == pkg_name
                && pkg.pkg_family() == pkg_family.as_deref()
                && pkg.repo_name() == repo_name
        });
        let installed_marker = if is_installed {
            format!(" {}", Colored(Color::Yellow, "[installed]"))
        } else {
            String::new()
        };
        // The family is what tells two candidates of the same name apart, so
        // this is the one listing that cannot leave it out.
        let name = match pkg.pkg_family() {
            Some(family) if family != pkg.pkg_name() => {
                format!("{}/{}", family, pkg.pkg_name())
            }
            _ => pkg.pkg_name().to_string(),
        };
        info!(
            "[{}] {}:{} | {}{}",
            idx + 1,
            Colored(Blue, &name),
            Colored(Green, pkg.repo_name()),
            Colored(LightRed, pkg.version()),
            installed_marker
        );
    }

    let selection = get_valid_selection(pkgs.len())?;
    Ok(pkgs.into_iter().nth(selection))
}

/// Asks the user to pick one of `pkgs`, failing when there is nothing to pick.
pub fn select_one<T: PackageExt>(pkgs: Vec<T>, package_name: &str) -> SoarResult<T> {
    select_package_interactively(pkgs, package_name)?
        .ok_or_else(|| SoarError::Custom(format!("no package selected for {package_name}")))
}

pub fn pretty_package_size(ghcr_size: Option<u64>, size: Option<u64>) -> String {
    ghcr_size
        .map(|size| format!("{}", Colored(Magenta, format_bytes(size, 2))))
        .or_else(|| size.map(|size| format!("{}", Colored(Magenta, format_bytes(size, 2)))))
        .unwrap_or_default()
}

/// Lists `targets` and asks whether to go ahead with `action` on them.
///
/// An empty answer or a yes proceeds. A declined prompt returns `false` so
/// the caller can stop cleanly, and a closed stdin is an error.
pub fn ask_target_action(targets: &[InstallTarget], action: &str) -> SoarResult<bool> {
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
            "{}:{} ({})",
            Colored(Blue, &target.package.pkg_name),
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
        Colored(Green, "Y"),
        Colored(Red, "n")
    ))?;

    if !response.is_empty() && !is_yes(&response) {
        info!("Quitting");
        return Ok(false);
    }

    Ok(true)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_explicit_yes_proceeds() {
        for yes in ["y", "Y", "yes", "YES", "Yes"] {
            assert!(is_yes(yes), "{yes}");
        }
        for no in ["", "n", "no", "yep", "y es", "1"] {
            assert!(!is_yes(no), "{no:?}");
        }
    }

    #[test]
    fn a_closed_stdin_is_not_an_answer() {
        assert!(read_answer(&mut "".as_bytes()).is_err());
        assert_eq!(read_answer(&mut "\n".as_bytes()).unwrap(), "");
        assert_eq!(read_answer(&mut "  Yes \n".as_bytes()).unwrap(), "Yes");
    }
}
