use std::{
    env::{self, consts::ARCH},
    fs,
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
    types::{OverwriteMode, Progress},
};
use tracing::{debug, error, info};

use crate::cli::SelfAction;

pub async fn process_self_action(action: &SelfAction) -> SoarResult<()> {
    let self_bin =
        env::current_exe().with_context(|| "Failed to get executable path".to_string())?;
    let self_version = env!("CARGO_PKG_VERSION");

    debug!("Executable path: {}", self_bin.display());

    match action {
        SelfAction::Update => {
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
                        SoarError::Custom(format!("No matching asset fund for {}", ARCH))
                    })?;

                debug!("Selected asset: {}", asset.name());

                let dl = Download::new(asset.url())
                    .output(self_bin.to_string_lossy())
                    .overwrite(OverwriteMode::Force)
                    .progress(|p| {
                        match p {
                            Progress::Starting {
                                total,
                            } => {
                                info!("Downloading update ({} bytes)...", total);
                            }
                            Progress::Chunk {
                                current,
                                total,
                            } => {
                                if current % (1024 * 1024) == 0 {
                                    let pct = (current as f64 / total as f64 * 100.0) as u8;
                                    debug!("Progress: {}%", pct);
                                }
                            }
                            Progress::Complete {
                                ..
                            } => {
                                debug!("Download complete");
                            }
                            _ => {}
                        }
                    });

                debug!("Downloading update from: {}", asset.url());
                dl.execute()?;
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
