use std::{
    fmt::Display,
    io::Write,
    sync::{LazyLock, RwLock},
};

use indicatif::HumanBytes;
use nu_ansi_term::Color::{self, Magenta};
use serde::Serialize;
use soar_core::{config::get_config, database::models::PackageExt, SoarResult};
use tracing::{error, info};

pub static COLOR: LazyLock<RwLock<bool>> = LazyLock::new(|| RwLock::new(true));

pub fn interactive_ask(ques: &str) -> SoarResult<String> {
    print!("{}", ques);

    std::io::stdout().flush()?;

    let mut response = String::new();
    std::io::stdin().read_line(&mut response)?;

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
            "[{}] {}#{}-{}:{}",
            idx + 1,
            pkg.pkg_name(),
            pkg.pkg_id(),
            pkg.version(),
            pkg.repo_name()
        );
    }

    let selection = get_valid_selection(pkgs.len())?;
    Ok(pkgs.into_iter().nth(selection))
}

pub fn has_no_desktop_integration(repo_name: &str, notes: Option<&[String]>) -> bool {
    !get_config().has_desktop_integration(repo_name)
        || notes.map_or(false, |all| {
            all.iter().any(|note| note == "NO_DESKTOP_INTEGRATION")
        })
}

pub fn pretty_package_size(ghcr_size: Option<u64>, size: Option<u64>) -> String {
    ghcr_size
        .map(|size| format!("{}", Colored(Magenta, HumanBytes(size))))
        .or_else(|| size.map(|size| format!("{}", Colored(Magenta, HumanBytes(size)))))
        .unwrap_or_default()
}
