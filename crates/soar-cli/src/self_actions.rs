use std::{
    env::{self, consts::ARCH},
    fs,
    io::{self, Write},
};

use semver::Version;
use soar_core::{
    error::{ErrorContext, SoarError},
    SoarResult,
};
use soar_dl::{
    download::Download,
    github::Github,
    traits::{Asset as _, Platform as _, Release as _},
    types::OverwriteMode,
};
use soar_utils::bytes::format_bytes;
use tracing::{debug, error, info};

use crate::{
    cli::SelfAction,
    progress::{create_progress_bar, handle_progress},
};

pub async fn process_self_action(action: &SelfAction) -> SoarResult<()> {
    let self_bin =
        env::current_exe().with_context(|| "Failed to get executable path".to_string())?;
    let self_version = env!("CARGO_PKG_VERSION");

    debug!("Executable path: {}", self_bin.display());

    match action {
        SelfAction::Update {
            yes,
        } => {
            let is_nightly = self_version.starts_with("nightly");
            debug!("Current version: {}", self_version);

            let target_nightly = match (env::var("SOAR_NIGHTLY"), env::var("SOAR_RELEASE")) {
                (Ok(_), Err(_)) => true,
                (Err(_), Ok(_)) => false,
                _ => is_nightly,
            };

            let releases = Github::fetch_releases("pkgforge/soar", None)?;

            let release = releases.iter().find(|release| {
                let is_nightly_release = release.tag().starts_with("nightly");
                debug!(
                    "Checking release: {}, Release Channel: {}",
                    release.tag(),
                    if is_nightly_release {
                        "nightly"
                    } else {
                        "stable"
                    }
                );

                if target_nightly {
                    is_nightly_release && release.name() != self_version
                } else {
                    let release_version = release.tag().trim_start_matches("v");
                    let parsed_release_version = Version::parse(release_version).ok();
                    let parsed_self_version = Version::parse(self_version).ok();

                    match (parsed_release_version, parsed_self_version) {
                        (Some(release_ver), Some(self_ver)) => {
                            let should_update = !is_nightly_release && release_ver > self_ver;
                            debug!(
                                "Comparing versions: release_ver={}, self_ver={}, should_update={}",
                                release_ver, self_ver, should_update
                            );
                            should_update
                        }
                        (_, None) => is_nightly,
                        _ => {
                            debug!(
                                "Skipping release {} due to invalid version.",
                                release_version
                            );
                            false
                        }
                    }
                }
            });

            if let Some(release) = release {
                let new_version = release.tag();

                if let Some(body) = release.body() {
                    if !body.is_empty() {
                        println!("\nRelease Notes for {}:\n", new_version);
                        println!("{}", body);
                        println!();
                    } else {
                        println!("No release notes available for this update.");
                    }
                } else {
                    println!("No release notes available for this update.");
                }

                if !yes {
                    print!("Update to {}? [y/N] ", new_version);
                    io::stdout().flush().map_err(|e| {
                        SoarError::IoError {
                            action: "flushing stdout".to_string(),
                            source: e,
                        }
                    })?;

                    let mut input = String::new();
                    io::stdin().read_line(&mut input).map_err(|e| {
                        SoarError::IoError {
                            action: "reading user input".to_string(),
                            source: e,
                        }
                    })?;

                    if !input.trim().to_lowercase().starts_with('y') {
                        info!("Update cancelled.");
                        return Ok(());
                    }
                }

                if target_nightly != is_nightly {
                    info!(
                        "Switching from {} to {} channel",
                        if is_nightly { "nightly" } else { "stable" },
                        if target_nightly { "nightly" } else { "stable" }
                    );
                } else {
                    info!("Found new update: {}", release.tag());
                }
                let assets = release.assets();
                let asset = assets
                    .iter()
                    .find(|a| {
                        a.name.contains(ARCH) && !a.name.contains("tar") && !a.name.contains("sum")
                    })
                    .ok_or_else(|| {
                        SoarError::Custom(format!("No matching asset found for {}", ARCH))
                    })?;

                debug!("Selected asset: {}", asset.name());

                if let Some(size) = asset.size() {
                    info!("Download size: {}", format_bytes(size, 2));
                }

                let progress_bar = create_progress_bar();
                progress_bar.set_prefix("Downloading");

                let dl = Download::new(asset.url())
                    .output(self_bin.to_string_lossy())
                    .overwrite(OverwriteMode::Force)
                    .progress({
                        let progress_bar = progress_bar.clone();
                        move |p| handle_progress(p, &progress_bar)
                    });

                debug!("Downloading update from: {}", asset.url());
                dl.execute()?;
                progress_bar.finish();
                info!("Soar updated to {}", release.tag());
            } else {
                eprintln!("No updates found.");
            }
        }
        SelfAction::Uninstall => {
            match fs::remove_file(self_bin) {
                Ok(_) => {
                    info!("Soar has been uninstalled successfully.");
                    info!("You should remove soar config and data files manually.");
                }
                Err(err) => {
                    error!("{}\nFailed to uninstall soar.", err.to_string());
                }
            };
        }
    };

    Ok(())
}
