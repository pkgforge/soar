use std::{fmt::Display, io::Write};

use nu_ansi_term::Color;
use serde::Serialize;
use soar_core::{database::models::Package, SoarResult};
use tracing::{error, info};

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
        write!(f, "{}", self.0.prefix())?;
        self.1.fmt(f)?;
        write!(f, "{}", self.0.suffix())
    }
}

pub fn vec_string<T: Display + Serialize>(value: Option<Vec<T>>) -> Option<String> {
    value.and_then(|json| serde_json::to_string(&json).ok())
}

fn get_valid_selection(max: usize) -> SoarResult<usize> {
    loop {
        let response = interactive_ask("Select a package: ")?;
        match response.parse::<usize>() {
            Ok(n) if n > 0 && n <= max => return Ok(n - 1),
            _ => error!("Invalid selection, please try again."),
        }
    }
}

pub fn select_package_interactively(
    pkgs: Vec<Package>,
    package_name: &str,
) -> SoarResult<Option<Package>> {
    info!("Multiple packages found for {package_name}");
    for (idx, pkg) in pkgs.iter().enumerate() {
        info!(
            "[{}] {}#{}-{}:{}",
            idx + 1,
            pkg.pkg_name,
            pkg.pkg_id,
            pkg.version,
            pkg.repo_name
        );
    }

    let selection = get_valid_selection(pkgs.len())?;
    Ok(pkgs.into_iter().nth(selection))
}
